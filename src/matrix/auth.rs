use super::*;

impl MatrixEngine {
    fn should_bypass_keyring() -> bool {
        cfg!(test) && std::env::var("CONSTELLATIONS_TEST_KEYRING").is_err()
    }

    async fn save_session_to_keyring(session_data: &SessionData) -> Result<()> {
        let secret = serde_json::to_vec(session_data)?;

        let keyring = match if Self::should_bypass_keyring() {
            Err(anyhow::anyhow!("Bypassing keyring in test"))
        } else {
            Keyring::new().await.map_err(|e| e.into())
        } {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize Keyring: {}. Session storage disabled.",
                    e
                );
                return Err(e);
            }
        };

        let mut attributes = HashMap::new();
        attributes.insert("app_id", "fi.joonastuomi.Constellations");
        attributes.insert("type", "matrix-session");

        match keyring
            .create_item("Constellations Matrix Session", &attributes, &secret, true)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::warn!(
                    "Failed to create session item in Keyring: {}. Session storage disabled.",
                    e
                );
                Err(e.into())
            }
        }
    }

    pub(super) async fn spawn_session_change_handler(&self, client: Client) {
        let mut subscriber = client.subscribe_to_session_changes();
        let homeserver = client.homeserver().to_string();

        let handle = tokio::spawn(async move {
            loop {
                match subscriber.recv().await {
                    Ok(change) => match change {
                        SessionChange::TokensRefreshed => {
                            info!("Session tokens refreshed, updating keyring...");

                            if let Some(session) = client.oauth().user_session() {
                                let session_data = SessionData {
                                    homeserver: homeserver.clone(),
                                    user_id: session.meta.user_id.to_string(),
                                    access_token: session.tokens.access_token.to_string(),
                                    refresh_token: session.tokens.refresh_token.clone(),
                                    id_token: None,
                                    device_id: session.meta.device_id.to_string(),
                                    is_oidc: true,
                                    client_id: client.oauth().client_id().map(|id| id.to_string()),
                                };

                                if let Err(e) = Self::save_session_to_keyring(&session_data).await {
                                    error!("Failed to update session in keyring: {}", e);
                                } else {
                                    info!("Successfully updated session in keyring.");
                                }
                            } else if let Some(session) = client.matrix_auth().session() {
                                let session_data = SessionData {
                                    homeserver: homeserver.clone(),
                                    user_id: session.meta.user_id.to_string(),
                                    access_token: session.tokens.access_token.to_string(),
                                    refresh_token: session.tokens.refresh_token.clone(),
                                    id_token: None,
                                    device_id: session.meta.device_id.to_string(),
                                    is_oidc: false,
                                    client_id: None,
                                };

                                if let Err(e) = Self::save_session_to_keyring(&session_data).await {
                                    error!("Failed to update session in keyring: {}", e);
                                } else {
                                    info!("Successfully updated session in keyring.");
                                }
                            } else {
                                error!("Session tokens refreshed but client has no session!");
                            }
                        }
                        SessionChange::UnknownToken { .. } => {
                            error!("Session token is no longer valid!");
                        }
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        error!("Session change subscriber lagged by {} messages", n);
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        info!("Session change subscriber closed.");
                        break;
                    }
                }
            }
        });

        let mut inner = self.inner.write().await;
        if let Some(old_handle) = inner.session_change_handle.take() {
            old_handle.abort();
        }
        inner.session_change_handle = Some(handle);
        drop(inner);
    }

    pub async fn register(&self, homeserver: &str, username: &str, password: &str) -> Result<()> {
        let homeserver_url = sanitize_homeserver_url(homeserver);

        let client = {
            let mut inner = self.inner.write().await;
            if let Some(handle) = inner.sync_handle.take() {
                handle.abort();
            }
            if let Some(handle) = inner.session_change_handle.take() {
                handle.abort();
            }
            let data_dir = inner.data_dir.clone();
            Self::reset_store(&data_dir);
            let new_client = Self::setup_client(data_dir, &homeserver_url).await?;
            inner.client = new_client.clone();
            new_client
        };

        use matrix_sdk::ruma::api::client::account::register::v3::Request as RegisterRequest;
        let mut request = RegisterRequest::new();
        request.username = Some(username.to_string());
        request.password = Some(password.to_string());
        request.initial_device_display_name = Some("Constellations Matrix Client".to_string());

        client
            .matrix_auth()
            .register(request)
            .await
            .context("Failed to register")?;

        let sync_service: Arc<SyncService> =
            Arc::new(SyncService::builder(client.clone()).build().await?);
        let room_list_service = sync_service.room_list_service();

        // Save session to oo7
        if let Some(session) = client.matrix_auth().session() {
            let session_data = SessionData {
                homeserver: homeserver_url,
                user_id: session.meta.user_id.to_string(),
                access_token: session.tokens.access_token.to_string(),
                refresh_token: session.tokens.refresh_token.clone(),
                id_token: None,
                device_id: session.meta.device_id.to_string(),
                is_oidc: false,
                client_id: None,
            };

            Self::save_session_to_keyring(&session_data).await?;
        }

        self.setup_event_handlers(&client);

        let mut inner = self.inner.write().await;
        inner.client = client.clone();
        // New client → possibly new homeserver → reset the cached search
        // support decision so it's probed again on the next search.
        inner.server_search_supported = None;
        inner.sync_service = Some(sync_service);
        inner.room_list_service = Some(room_list_service);

        drop(inner);
        self.spawn_session_change_handler(client).await;

        Ok(())
    }

    pub async fn login(&self, homeserver: &str, username: &str, password: &str) -> Result<()> {
        let homeserver_url = sanitize_homeserver_url(homeserver);

        let client = {
            let mut inner = self.inner.write().await;
            if let Some(handle) = inner.sync_handle.take() {
                handle.abort();
            }
            if let Some(handle) = inner.session_change_handle.take() {
                handle.abort();
            }
            let data_dir = inner.data_dir.clone();
            // Fresh login → drop any stale Olm account from a previous session
            // so matrix-sdk can create a new device identity cleanly.
            Self::reset_store(&data_dir);
            let new_client = Self::setup_client(data_dir, &homeserver_url).await?;
            inner.client = new_client.clone();
            new_client
        };

        client
            .matrix_auth()
            .login_username(username, password)
            .initial_device_display_name("Constellations Matrix Client")
            .send()
            .await
            .context("Failed to login")?;

        let sync_service: Arc<SyncService> =
            Arc::new(SyncService::builder(client.clone()).build().await?);
        let room_list_service = sync_service.room_list_service();

        // Save session to oo7
        if let Some(session) = client.matrix_auth().session() {
            let session_data = SessionData {
                homeserver: homeserver_url,
                user_id: session.meta.user_id.to_string(),
                access_token: session.tokens.access_token.to_string(),
                refresh_token: session.tokens.refresh_token.clone(),
                id_token: None,
                device_id: session.meta.device_id.to_string(),
                is_oidc: false,
                client_id: None,
            };

            Self::save_session_to_keyring(&session_data).await?;
        }

        self.setup_event_handlers(&client);

        let mut inner = self.inner.write().await;
        inner.client = client.clone();
        // New client → possibly new homeserver → reset the cached search
        // support decision so it's probed again on the next search.
        inner.server_search_supported = None;
        inner.sync_service = Some(sync_service);
        inner.room_list_service = Some(room_list_service);

        drop(inner);
        self.spawn_session_change_handler(client).await;

        Ok(())
    }

    async fn load_session_secret() -> Option<Vec<u8>> {
        let keyring = match if Self::should_bypass_keyring() {
            Err(anyhow::anyhow!("Bypassing keyring in test"))
        } else {
            Keyring::new().await.map_err(|e| e.into())
        } {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize Keyring for restore: {}. File-based fallback disabled.",
                    e
                );
                return None;
            }
        };

        let mut attributes = HashMap::new();
        attributes.insert("app_id", "fi.joonastuomi.Constellations");
        attributes.insert("type", "matrix-session");

        match keyring.search_items(&attributes).await {
            Ok(items) => {
                if let Some(item) = items.first()
                    && let Ok(secret) = item.secret().await
                {
                    return Some(secret.to_vec());
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to search Keyring items for restore: {}. File-based fallback disabled.",
                    e
                );
            }
        }

        None
    }

    async fn restore_client_session(client: &Client, session_data: SessionData) -> Result<()> {
        if session_data.is_oidc {
            let client_id = session_data
                .client_id
                .unwrap_or_else(|| OIDC_CLIENT_ID.to_string());
            let client_id = matrix_sdk::authentication::oauth::ClientId::new(client_id);

            client.oauth().restore_registered_client(client_id.clone());
            client
                .oauth()
                .restore_session(
                    matrix_sdk::authentication::oauth::OAuthSession {
                        client_id,
                        user: matrix_sdk::authentication::oauth::UserSession {
                            meta: matrix_sdk::SessionMeta {
                                user_id: UserId::parse(session_data.user_id)?,
                                device_id: OwnedDeviceId::from(session_data.device_id),
                            },
                            tokens: SessionTokens {
                                access_token: session_data.access_token,
                                refresh_token: session_data.refresh_token,
                            },
                        },
                    },
                    matrix_sdk::store::RoomLoadSettings::default(),
                )
                .await?;
        } else {
            let matrix_session = MatrixSession {
                meta: matrix_sdk::SessionMeta {
                    user_id: UserId::parse(session_data.user_id)?,
                    device_id: OwnedDeviceId::from(session_data.device_id),
                },
                tokens: SessionTokens {
                    access_token: session_data.access_token,
                    refresh_token: session_data.refresh_token,
                },
            };
            client
                .matrix_auth()
                .restore_session(
                    matrix_session,
                    matrix_sdk::store::RoomLoadSettings::default(),
                )
                .await?;
        }
        Ok(())
    }

    pub async fn restore_session(&self) -> Result<bool> {
        let Some(secret) = Self::load_session_secret().await else {
            return Ok(false);
        };

        let session_data: SessionData = serde_json::from_slice(&secret)?;

        let data_dir = self.inner.read().await.data_dir.clone();
        let client = Self::setup_client(data_dir, &session_data.homeserver).await?;

        Self::restore_client_session(&client, session_data).await?;

        let sync_service: Arc<SyncService> =
            Arc::new(SyncService::builder(client.clone()).build().await?);
        let room_list_service = sync_service.room_list_service();

        self.setup_event_handlers(&client);

        let mut inner = self.inner.write().await;
        inner.client = client.clone();
        // New client → possibly new homeserver → reset the cached search
        // support decision so it's probed again on the next search.
        inner.server_search_supported = None;
        inner.sync_service = Some(sync_service);
        inner.room_list_service = Some(room_list_service);

        drop(inner);
        self.spawn_session_change_handler(client).await;

        Ok(true)
    }

    pub async fn logout(&self) -> Result<()> {
        let keyring = match if Self::should_bypass_keyring() {
            Err(anyhow::anyhow!("Bypassing keyring in test"))
        } else {
            Keyring::new().await.map_err(|e| e.into())
        } {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!("Failed to initialize Keyring for logout: {}.", e);
                return Ok(());
            }
        };

        let mut session_attributes = HashMap::new();
        session_attributes.insert("app_id", "fi.joonastuomi.Constellations");
        session_attributes.insert("type", "matrix-session");

        if let Ok(items) = keyring.search_items(&session_attributes).await {
            let futures = items.iter().map(|item| item.delete());
            let _ = futures::future::join_all(futures).await;
        }

        let mut pass_attributes = HashMap::new();
        pass_attributes.insert("app_id", "fi.joonastuomi.Constellations");
        pass_attributes.insert("type", "store-passphrase");

        if let Ok(items) = keyring.search_items(&pass_attributes).await {
            let futures = items.iter().map(|item| item.delete());
            let _ = futures::future::join_all(futures).await;
        }

        let mut inner = self.inner.write().await;
        if let Some(handle) = inner.sync_handle.take() {
            handle.abort();
        }
        if let Some(sync_service) = inner.sync_service.take() {
            let _ = sync_service.stop().await;
        }
        inner.room_list_service = None;
        inner.room_list_controller = None;
        inner.timelines.clear();
        inner.threaded_timelines.clear();
        inner.space_hierarchy = SpaceHierarchy::new();

        // Try logging out properly from Matrix
        let _ = inner.client.matrix_auth().logout().await;

        let store_path = inner.data_dir.join("matrix-store");
        let _ = std::fs::remove_dir_all(&store_path);

        Ok(())
    }

    pub async fn login_oidc(&self, homeserver: &str) -> Result<Url> {
        let homeserver_url = sanitize_homeserver_url(homeserver);

        let client = {
            let mut inner = self.inner.write().await;
            if let Some(handle) = inner.sync_handle.take() {
                handle.abort();
            }
            if let Some(handle) = inner.session_change_handle.take() {
                handle.abort();
            }
            let data_dir = inner.data_dir.clone();
            Self::reset_store(&data_dir);
            let new_client = Self::setup_client(data_dir, &homeserver_url).await?;
            inner.client = new_client.clone();
            new_client
        };

        // Register the OAuth client dynamically (RFC 7591). We must NOT call
        // `restore_registered_client()` here with a hardcoded ID: that sets the
        // ID locally without contacting the server, which makes the SDK skip
        // registration, and the homeserver (MAS on matrix.org, etc.) then
        // rejects the unknown `client_id` in the authorization URL. Passing the
        // registration data to `login()` lets the SDK POST our metadata to the
        // server's registration endpoint and use the server-assigned client ID.
        let redirect_uri = Url::parse(OIDC_CALLBACK_URL)?;
        let registration_data = oauth_registration_data()?;
        let login_url = client
            .oauth()
            .login(redirect_uri, None, Some(registration_data), None)
            .build()
            .await
            .map_err(classify_oidc_start_error)?
            .url;

        let mut inner = self.inner.write().await;
        inner.oidc_client = Some(client);

        Ok(login_url)
    }

    pub async fn complete_oidc_login(&self, callback_url: Url) -> Result<()> {
        let client = {
            let mut inner = self.inner.write().await;
            inner
                .oidc_client
                .take()
                .context("No OIDC login in progress")?
        };

        client
            .oauth()
            .finish_login(callback_url.into())
            .await
            .context("Failed to complete OIDC login")?;

        self.finalize_oauth_login(client).await?;
        Ok(())
    }

    async fn finalize_oauth_login(&self, client: Client) -> Result<String> {
        let sync_service: Arc<SyncService> =
            Arc::new(SyncService::builder(client.clone()).build().await?);
        let room_list_service = sync_service.room_list_service();

        self.setup_event_handlers(&client);

        let user_id = client
            .user_id()
            .context("OAuth login finished but client has no user id")?
            .to_string();

        // Save session to oo7
        if let Some(session) = client.oauth().user_session() {
            let session_data = SessionData {
                homeserver: client.homeserver().to_string(),
                user_id: session.meta.user_id.to_string(),
                access_token: session.tokens.access_token.to_string(),
                refresh_token: session.tokens.refresh_token.clone(),
                id_token: None,
                device_id: session.meta.device_id.to_string(),
                is_oidc: true,
                client_id: client.oauth().client_id().map(|id| id.to_string()),
            };

            Self::save_session_to_keyring(&session_data).await?;
        }

        let mut inner = self.inner.write().await;
        inner.client = client.clone();
        // New client → possibly new homeserver → reset the cached search
        // support decision so it's probed again on the next search.
        inner.server_search_supported = None;
        inner.sync_service = Some(sync_service);
        inner.room_list_service = Some(room_list_service);

        drop(inner);
        self.spawn_session_change_handler(client).await;

        Ok(user_id)
    }

    pub async fn start_qr_login(
        &self,
        homeserver: &str,
    ) -> Result<mpsc::UnboundedReceiver<QrLoginProgress>> {
        let homeserver_url = sanitize_homeserver_url(homeserver);

        let client = {
            let mut inner = self.inner.write().await;
            if let Some(handle) = inner.sync_handle.take() {
                handle.abort();
            }
            if let Some(handle) = inner.session_change_handle.take() {
                handle.abort();
            }
            if let Some(handle) = inner.qr_login_handle.take() {
                handle.abort();
            }
            let data_dir = inner.data_dir.clone();
            Self::reset_store(&data_dir);
            let new_client = Self::setup_client(data_dir, &homeserver_url).await?;
            inner.client = new_client.clone();
            new_client
        };

        // Register the OAuth client dynamically (see `login_oidc` for why a
        // hardcoded static client ID does not work).
        let registration_data = oauth_registration_data()?;

        let (tx, rx) = mpsc::unbounded_channel::<QrLoginProgress>();
        let engine = self.clone();
        let tx_result = tx.clone();

        let handle = tokio::spawn(async move {
            // `login` borrows `oauth` which borrows `client`, so both must
            // outlive the login future. Keep them on this task's stack.
            let oauth = client.oauth();
            let login = oauth
                .login_with_qr_code(Some(&registration_data))
                .generate();
            let mut progress = login.subscribe_to_progress();

            // Forward SDK progress to the UI until the stream ends.
            while let Some(state) = progress.next().await {
                if let Some(mapped) = QrLoginProgress::from_sdk(state)
                    && tx.send(mapped).is_err()
                {
                    // Receiver dropped (UI cancelled); stop forwarding.
                    break;
                }
            }

            // Drive the login future to completion. On success the SDK has
            // already finished the OAuth login + E2EE secret import; we just
            // wire up the sync service / keyring and start sliding sync.
            let result = login.await;
            let finished = match result {
                Ok(()) => match engine.finalize_oauth_login(client).await {
                    Ok(user_id) => match engine.start_sync().await {
                        Ok(()) => QrLoginProgress::Finished(Ok(user_id)),
                        Err(e) => QrLoginProgress::Finished(Err(e.to_string())),
                    },
                    Err(e) => QrLoginProgress::Finished(Err(e.to_string())),
                },
                Err(e) => QrLoginProgress::Finished(Err(e.to_string())),
            };
            let _ = tx_result.send(finished);
        });

        let mut inner = self.inner.write().await;
        inner.qr_login_handle = Some(handle);

        Ok(rx)
    }

    pub async fn cancel_qr_login(&self) {
        let mut inner = self.inner.write().await;
        if let Some(handle) = inner.qr_login_handle.take() {
            handle.abort();
        }
    }

    pub(crate) async fn get_or_create_store_passphrase() -> Result<String> {
        let keyring = match if Self::should_bypass_keyring() {
            Err(anyhow::anyhow!("Bypassing keyring in test"))
        } else {
            Keyring::new().await.map_err(|e| e.into())
        } {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize Keyring: {}. Passphrase storage disabled.",
                    e
                );
                return Err(e);
            }
        };

        let mut attributes = HashMap::new();
        attributes.insert("app_id", "fi.joonastuomi.Constellations");
        attributes.insert("type", "store-passphrase");

        match keyring.search_items(&attributes).await {
            Ok(items) => {
                if let Some(item) = items.first()
                    && let Ok(secret) = item.secret().await
                    && let Ok(passphrase) = String::from_utf8(secret.to_vec())
                {
                    return Ok(passphrase);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to search items in Keyring: {}. Passphrase storage disabled.",
                    e
                );
                return Err(e.into());
            }
        }

        let mut buf = [0u8; 32];
        SysRng
            .try_fill_bytes(&mut buf)
            .context("Failed to generate secure random bytes for store passphrase")?;

        let passphrase: String = buf.iter().map(|b| format!("{:02x}", b)).collect();

        match keyring
            .create_item(
                "Constellations Store Passphrase",
                &attributes,
                passphrase.as_bytes(),
                true,
            )
            .await
        {
            Ok(_) => Ok(passphrase),
            Err(e) => {
                tracing::warn!(
                    "Failed to create item in Keyring: {}. Passphrase storage disabled.",
                    e
                );
                Err(e.into())
            }
        }
    }

    fn reset_store(data_dir: &std::path::Path) {
        let store_path = data_dir.join("matrix-store");
        let search_index_path = data_dir.join("search-index");
        if store_path.exists() {
            tracing::info!(
                "Resetting crypto store at {} before a fresh login.",
                store_path.display()
            );
            if let Err(e) = std::fs::remove_dir_all(&store_path) {
                tracing::warn!("Failed to remove crypto store: {e}");
            }
        }
        if search_index_path.exists()
            && let Err(e) = std::fs::remove_dir_all(&search_index_path)
        {
            tracing::warn!("Failed to remove search index: {e}");
        }
    }

    pub(super) async fn setup_client(data_dir: PathBuf, homeserver_url: &str) -> Result<Client> {
        let store_path = data_dir.join("matrix-store");
        let search_index_path = data_dir.join("search-index");

        if !tokio::fs::try_exists(&data_dir).await.unwrap_or(false) {
            tokio::fs::create_dir_all(&data_dir).await?;
        }

        if !tokio::fs::try_exists(&store_path).await.unwrap_or(false)
            && tokio::fs::try_exists(&search_index_path)
                .await
                .unwrap_or(false)
        {
            tracing::info!(
                "Fresh SQLite store, clearing existing search index path to prevent mismatched keys."
            );
            let _ = tokio::fs::remove_dir_all(&search_index_path).await;
        }

        let passphrase = Self::get_or_create_store_passphrase().await?;

        let mut key_mismatch = false;
        if tokio::fs::try_exists(&search_index_path)
            .await
            .unwrap_or(false)
            && let Ok(mut entries) = tokio::fs::read_dir(&search_index_path).await
        {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                    let key_path = entry.path().join("seshat-index.key");
                    if tokio::fs::try_exists(&key_path).await.unwrap_or(false)
                        && let Ok(bytes) = tokio::fs::read(&key_path).await
                        && matrix_sdk_store_encryption::StoreCipher::import(&passphrase, &bytes)
                            .is_err()
                    {
                        tracing::warn!(
                            "Mismatched search index encryption key in room {:?}. Clearing search index.",
                            entry.file_name()
                        );
                        key_mismatch = true;
                        break;
                    }
                }
            }
        }

        if key_mismatch {
            let _ = tokio::fs::remove_dir_all(&search_index_path).await;
        }

        let build_client = |path: PathBuf, search_path: PathBuf, pass: String| {
            Client::builder()
                .homeserver_url(homeserver_url)
                .sqlite_store(path, Some(&pass))
                .search_index_store(
                    matrix_sdk::search_index::SearchIndexStoreKind::EncryptedDirectory(
                        search_path,
                        pass,
                    ),
                )
                .handle_refresh_tokens()
        };

        let client = match build_client(
            store_path.clone(),
            search_index_path.clone(),
            passphrase.clone(),
        )
        .build()
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize stores (possibly corrupted cipher): {}. Recreating store.",
                    e
                );
                let _ = std::fs::remove_dir_all(&store_path);
                let _ = std::fs::remove_dir_all(&search_index_path);
                build_client(store_path, search_index_path, passphrase)
                    .build()
                    .await?
            }
        };

        if let Some(machine) = client.olm_machine_for_testing().await.as_ref() {
            machine.set_room_key_requests_enabled(true);
            machine.set_room_key_forwarding_enabled(true);
        }

        Ok(client)
    }
}
