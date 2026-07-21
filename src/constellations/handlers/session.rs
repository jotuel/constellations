use crate::matrix;
use crate::{AuthFlow, Constellations, MediaSource, Message, QrLoginStep, Url, redact_url};
use cosmic::{Action, Application, Task};
use futures::stream::StreamExt;

impl Constellations {
    pub fn handle_engine_ready(
        &mut self,
        res: Result<matrix::MatrixEngine, matrix::SyncError>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok(engine) => {
                self.matrix = Some(engine.clone());
                crate::unified_push::start_unified_push_listener(engine.clone());
                Task::perform(
                    async move {
                        let did_restore = engine.restore_session().await.unwrap_or(false);
                        if did_restore {
                            let user_id = engine.client().await.user_id().map(|u| u.to_string());
                            let sync_res = engine.start_sync().await;
                            (user_id, sync_res)
                        } else {
                            (
                                None,
                                Err(matrix::SyncError::Generic(
                                    "No session to restore".to_string(),
                                )),
                            )
                        }
                    },
                    |(user_id, sync_res)| {
                        if let Some(uid) = user_id {
                            Action::from(Message::UserReady(Some(uid), sync_res))
                        } else {
                            Action::from(Message::UserReady(None, sync_res))
                        }
                    },
                )
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-init-engine", error = e.to_string()).to_string(),
                );
                self.is_initializing = false;
                Task::none()
            }
        }
    }

    pub fn handle_user_ready(
        &mut self,
        user_id: Option<String>,
        sync_res: Result<(), matrix::SyncError>,
    ) -> Task<Action<Message>> {
        self.user_id = user_id;
        self.is_initializing = false;
        let title_task = self.update_title();
        if self.user_id.is_none() {
            return title_task;
        }

        match sync_res {
            Ok(_) => {}
            Err(matrix::SyncError::MissingSlidingSyncSupport) => {
                self.sync_status = matrix::SyncStatus::MissingSlidingSyncSupport;
            }
            Err(e) => {
                self.sync_status = matrix::SyncStatus::Error(e.to_string());
            }
        }
        let mut tasks = Vec::new();
        tasks.push(title_task);

        if let Some(matrix) = &self.matrix {
            let matrix_ignored = matrix.clone();
            tasks.push(Task::perform(
                async move { matrix_ignored.ignored_users().await.unwrap_or_default() },
                |users| {
                    Message::UserSettings(crate::settings::user::Message::IgnoredUsersLoaded(Ok(
                        users,
                    )))
                    .into()
                },
            ));

            let mut media_fetches = Vec::new();
            for room in self.room_list.iter() {
                if let Some(avatar_url) = &room.avatar_url
                    && !self.media_cache.contains_key(avatar_url)
                {
                    let matrix_clone = matrix.clone();
                    let url_str = avatar_url.clone();
                    let uri = matrix_sdk::ruma::OwnedMxcUri::from(avatar_url.as_str());
                    let source = MediaSource::Plain(uri);
                    media_fetches.push(async move {
                        let res = matrix_clone
                            .fetch_media(source)
                            .await
                            .map_err(|e| e.to_string());
                        (url_str, res)
                    });
                }
            }
            if !media_fetches.is_empty() {
                tasks.push(Task::perform(
                    async move {
                        futures::stream::iter(media_fetches)
                            .buffer_unordered(10)
                            .collect::<Vec<_>>()
                            .await
                    },
                    |results| Message::MediaFetchedBatch(results).into(),
                ));
            }
        }

        // Replay a permalink that arrived before the session was restored.
        if let Some(link) = self.pending_link.take()
            && self.matrix.is_some()
        {
            tasks.push(Task::done(Action::from(Message::OpenMatrixLink(link))));
        }

        Task::batch(tasks)
    }

    pub fn handle_toggle_login_mode(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.is_registering_mode = !self.is_registering_mode;
        self.error = None;
        Task::none()
    }

    pub fn handle_submit_register(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.is_registering = true;
            self.error = None;
            self.sync_status = matrix::SyncStatus::Disconnected;
            let matrix = matrix.clone();
            let homeserver = self.login_homeserver.clone();
            let username = self.login_username.clone();
            let password = std::mem::take(&mut self.login_password);

            Task::perform(
                async move {
                    matrix.register(&homeserver, &username, &password).await?;
                    let user_id = matrix
                        .client()
                        .await
                        .user_id()
                        .map(|u| u.to_string())
                        .ok_or_else(|| {
                            anyhow::anyhow!("Failed to get user ID after registration")
                        })?;
                    matrix.start_sync().await?;
                    Ok(user_id)
                },
                |res: Result<String, anyhow::Error>| {
                    Action::from(Message::RegisterFinished(
                        res.map_err(matrix::SyncError::from),
                    ))
                },
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_register_finished(
        &mut self,
        res: Result<String, matrix::SyncError>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.is_registering = false;
        match res {
            Ok(user_id) => {
                self.user_id = Some(user_id);
                self.login_homeserver.clear();
                self.login_username.clear();
                self.login_password.clear();
                self.error = None;
                self.update_title()
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-registration", error = e.to_string()).to_string(),
                );
                Task::none()
            }
        }
    }

    pub fn handle_submit_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.auth_flow = AuthFlow::Password;
            self.error = None;
            self.sync_status = matrix::SyncStatus::Disconnected;
            let matrix = matrix.clone();
            let homeserver = self.login_homeserver.clone();
            let username = self.login_username.clone();
            let password = std::mem::take(&mut self.login_password);

            Task::perform(
                async move {
                    matrix.login(&homeserver, &username, &password).await?;
                    let user_id = matrix
                        .client()
                        .await
                        .user_id()
                        .map(|u| u.to_string())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get user ID after login"))?;
                    matrix.start_sync().await?;
                    Ok(user_id)
                },
                |res: Result<String, anyhow::Error>| {
                    Action::from(Message::LoginFinished(res.map_err(matrix::SyncError::from)))
                },
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_login_finished(
        &mut self,
        res: Result<String, matrix::SyncError>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.auth_flow = AuthFlow::Idle;
        match res {
            Ok(user_id) => self.user_id = Some(user_id.clone()),
            Err(matrix::SyncError::MissingSlidingSyncSupport) => {
                self.sync_status = matrix::SyncStatus::MissingSlidingSyncSupport;
            }
            Err(e) => {
                self.set_error(crate::fl!("error-failed-login", error = e.to_string()).to_string());
            }
        }
        Task::none()
    }

    pub fn handle_submit_oidc_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.auth_flow = AuthFlow::Oidc;
            self.error = None;
            let matrix = matrix.clone();
            let homeserver = self.login_homeserver.clone();
            Task::perform(
                async move {
                    matrix
                        .login_oidc(&homeserver)
                        .await
                        .map_err(|e| e.to_string())
                },
                |res| Action::from(Message::OidcLoginStarted(res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_oidc_login_started(
        &mut self,
        res: Result<Url, String>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok(url) => {
                tracing::info!("Opening URL: {}", redact_url(&url));
                let _ = open::that(url.as_str());
            }
            Err(e) => {
                self.auth_flow = AuthFlow::Idle;
                // Distinguish "OAuth not supported by this homeserver" from
                // other failures so the message can guide the user (e.g. use a
                // password login instead).
                if e == crate::matrix::OIDC_NOT_SUPPORTED_SENTINEL {
                    self.set_error(crate::fl!("error-oidc-not-supported").to_string());
                } else {
                    self.set_error(
                        crate::fl!("error-failed-oidc-login", error = e.to_string()).to_string(),
                    );
                }
            }
        }
        Task::none()
    }

    pub fn handle_oidc_callback(
        &mut self,
        url: Url,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.auth_flow = AuthFlow::Oidc;
            self.error = None;
            let matrix = matrix.clone();
            Task::perform(
                async move {
                    matrix.complete_oidc_login(url).await?;
                    let user_id = matrix
                        .client()
                        .await
                        .user_id()
                        .map(|u| u.to_string())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get user ID after OIDC login"))?;
                    matrix.start_sync().await?;
                    Ok(user_id)
                },
                |res: Result<String, anyhow::Error>| {
                    Action::from(Message::LoginFinished(res.map_err(matrix::SyncError::from)))
                },
            )
        } else {
            Task::none()
        }
    }

    /// Open a Matrix permalink (room/alias/user/event/join) handed to us via
    /// argv, the URI scheme, or (later) in-app paste.
    ///
    /// For not-yet-loaded event targets this only scrolls if the event is
    /// already in `timeline_items`; the not-yet-loaded fetch path is Phase 3.
    pub fn handle_logout(&mut self) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            let matrix = matrix.clone();
            return Task::perform(
                async move {
                    let _ = matrix.logout().await;
                },
                |_| Action::from(Message::LogoutFinished),
            );
        }
        Task::none()
    }

    pub fn handle_logout_finished(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.user_id = None;
        self.matrix = None;
        self.sync_status = matrix::SyncStatus::Disconnected;
        self.room_list.clear();
        self.selected_room = None;
        self.timeline_items.clear();
        self.recompute_thread_counts();
        self.auth_flow = AuthFlow::Idle;
        self.login_password.clear();
        self.error = None;
        self.selected_space = None;
        self.is_sync_indicator_active = false;
        self.is_loading_more = false;
        self.joined_room_ids.clear();
        Task::none()
    }

    pub fn handle_start_qr_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.auth_flow = AuthFlow::Qr {
            step: QrLoginStep::Initiating,
        };
        self.error = None;
        self.qr_code_bytes = None;
        self.qr_check_code_sender = None;
        self.qr_user_code = None;
        self.qr_check_code_input.clear();

        let Some(matrix) = self.matrix.clone() else {
            return Task::none();
        };
        let mut hs = self.login_homeserver.trim().to_string();
        if hs.is_empty() {
            hs = "https://matrix.org".to_string();
        }
        if !hs.starts_with("http://") && !hs.starts_with("https://") {
            hs = format!("https://{}", hs);
        }

        // Stream MSC4108 QR-login progress from the background task into the
        // MVU loop. The state machine first awaits `start_qr_login` (which
        // builds the client and spawns the login task), then drains the
        // progress receiver until it closes.
        enum QrStreamState {
            Starting(matrix::MatrixEngine, String),
            Draining(tokio::sync::mpsc::UnboundedReceiver<matrix::QrLoginProgress>),
            Done,
        }

        let stream = cosmic::iced::futures::stream::unfold(
            QrStreamState::Starting(matrix, hs),
            |state| async move {
                match state {
                    QrStreamState::Starting(matrix, hs) => match matrix.start_qr_login(&hs).await {
                        Ok(rx) => Some((None, QrStreamState::Draining(rx))),
                        Err(e) => Some((
                            Some(matrix::QrLoginProgress::Finished(Err(e.to_string()))),
                            QrStreamState::Done,
                        )),
                    },
                    QrStreamState::Draining(mut rx) => match rx.recv().await {
                        Some(progress) => Some((Some(progress), QrStreamState::Draining(rx))),
                        None => Some((None, QrStreamState::Done)),
                    },
                    QrStreamState::Done => None,
                }
            },
        )
        .filter_map(|opt| async move { opt });

        Task::run(stream, |progress| {
            Action::from(Message::QrLoginProgress(progress))
        })
    }

    pub fn handle_cancel_qr_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.auth_flow = AuthFlow::Idle;
        self.qr_code_bytes = None;
        self.qr_check_code_sender = None;
        self.qr_user_code = None;
        self.qr_check_code_input.clear();

        if let Some(matrix) = self.matrix.clone() {
            Task::perform(async move { matrix.cancel_qr_login().await }, |_| {
                Action::from(Message::NoOp)
            })
        } else {
            Task::none()
        }
    }

    pub fn handle_qr_login_progress(
        &mut self,
        progress: matrix::QrLoginProgress,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        use matrix::QrLoginProgress as P;
        match progress {
            P::QrReady(bytes) => {
                self.qr_code_bytes = Some(bytes);
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::ShowingQr,
                };
                Task::none()
            }
            P::QrScanned(sender) => {
                self.qr_check_code_sender = Some(sender);
                self.qr_check_code_input.clear();
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::AwaitingCheckCode,
                };
                Task::none()
            }
            P::WaitingForToken { user_code } => {
                self.qr_user_code = Some(user_code);
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::Authenticating,
                };
                Task::none()
            }
            P::SyncingSecrets => {
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::SyncingSecrets,
                };
                Task::none()
            }
            P::Finished(res) => {
                self.qr_code_bytes = None;
                self.qr_check_code_sender = None;
                self.qr_user_code = None;
                self.qr_check_code_input.clear();
                match res {
                    Ok(user_id) => {
                        self.auth_flow = AuthFlow::Qr {
                            step: QrLoginStep::Success,
                        };
                        // Sliding sync already started inside the background
                        // task (after finalize_oauth_login). Route through the
                        // shared login-finished path: sets user_id and resets
                        // auth_flow to Idle.
                        self.handle_login_finished(Ok(user_id))
                    }
                    Err(e) => {
                        self.auth_flow = AuthFlow::Qr {
                            step: QrLoginStep::Error,
                        };
                        self.set_error(
                            crate::fl!("error-failed-qr-login", error = e.to_string()).to_string(),
                        );
                        Task::none()
                    }
                }
            }
        }
    }

    pub fn handle_qr_check_code_changed(
        &mut self,
        code: String,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        // Keep only digits, max two.
        let filtered: String = code
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(2)
            .collect();
        self.qr_check_code_input = filtered;
        Task::none()
    }

    pub fn handle_submit_qr_check_code(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        let Some(sender) = self.qr_check_code_sender.take() else {
            return Task::none();
        };
        let code_str = std::mem::take(&mut self.qr_check_code_input);
        // Parse the two-digit check code; on failure, surface an error and
        // return to the QR-display step so the user can retry.
        let parsed: Result<u8, _> = code_str.parse();
        match parsed {
            Ok(code) => {
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::Authenticating,
                };
                Task::perform(async move { sender.send(code).await }, |res| match res {
                    Ok(()) => Action::from(Message::NoOp),
                    Err(e) => {
                        tracing::warn!("Failed to submit QR check code: {e}");
                        Action::from(Message::NoOp)
                    }
                })
            }
            Err(_) => {
                self.set_error(crate::fl!("login-qr-check-code-invalid").to_string());
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::ShowingQr,
                };
                Task::none()
            }
        }
    }
}
