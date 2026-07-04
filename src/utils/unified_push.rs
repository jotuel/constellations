use std::sync::mpsc;
use tokio::runtime::Handle;
use tracing::{error, info, warn};
use unifiedpush::{PushEvent, UnifiedPush};
use unifiedpush_storage_preferences::{UnifiedPushStoragePreferences, preferences::AppInfo};

pub async fn run_headless_notification_handler() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting UnifiedPush headless handler...");

    let storage = UnifiedPushStoragePreferences::new(AppInfo {
        name: "fi.joonastuomi.Constellations",
        author: "Joonas Tuomi",
    });

    let (tx, rx) = mpsc::channel();
    let handle = Handle::current();

    let _up = UnifiedPush::new("fi.joonastuomi.Constellations", storage, tx, handle)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize UnifiedPush client: {:?}", e))?;

    // Wait briefly for incoming D-Bus message calls routed by KUnifiedPush.
    // Since KUnifiedPush activated us, we expect the Message call immediately.
    match tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Ok(event) = rx.recv() {
            match event {
                PushEvent::Message { .. } => {
                    info!("UnifiedPush message received in headless mode. Syncing...");
                    if let Err(e) = perform_background_sync_and_notify().await {
                        error!("Background sync failed: {:?}", e);
                    }
                    break;
                }
                PushEvent::NewEndpoint { endpoint, .. } => {
                    info!(
                        "UnifiedPush received new endpoint in headless mode: {}",
                        endpoint.endpoint
                    );
                    if let Err(e) = register_endpoint_if_logged_in(&endpoint.endpoint).await {
                        warn!(
                            "Could not register new endpoint on homeserver (headless): {:?}",
                            e
                        );
                    }
                }
                PushEvent::Unregistered { .. } => {
                    info!("UnifiedPush unregistered in headless mode.");
                }
                _ => {}
            }
        }
    })
    .await
    {
        Ok(_) => info!("Headless push processing completed."),
        Err(_) => {
            warn!(
                "Headless push processing timed out waiting for event; trying generic fallback sync."
            );
            // Even if we timeout, let's try a background sync anyway in case D-Bus was activated
            // but the message event wasn't delivered on this channel in time.
            if let Err(e) = perform_background_sync_and_notify().await {
                error!("Generic background sync failed: {:?}", e);
            }
        }
    }

    Ok(())
}

async fn perform_background_sync_and_notify() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = dirs::data_dir()
        .map(|d| d.join("fi.joonastuomi.Constellations"))
        .ok_or_else(|| anyhow::anyhow!("No data directory found"))?;

    // Create MatrixEngine (which loads credentials from Keyring)
    let engine = match crate::matrix::MatrixEngine::new(data_dir).await {
        Ok(e) => e,
        Err(e) => {
            show_fallback_notification().await;
            return Err(e.into());
        }
    };

    let did_restore = engine.restore_session().await.unwrap_or(false);
    if !did_restore {
        show_fallback_notification().await;
        return Err(anyhow::anyhow!("No active session to restore").into());
    }

    // Start sync and wait for events to be processed (3 seconds is enough)
    engine.start_sync().await?;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    Ok(())
}

async fn show_fallback_notification() {
    let summary = crate::fl!("notification-new-message");
    let body = crate::fl!("notification-body");
    let _ = notify_rust::Notification::new()
        .appname("Constellations")
        .summary(&summary)
        .body(&body)
        .show_async()
        .await;
}

async fn register_endpoint_if_logged_in(endpoint: &str) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = dirs::data_dir()
        .map(|d| d.join("fi.joonastuomi.Constellations"))
        .ok_or_else(|| anyhow::anyhow!("No data directory found"))?;

    let engine = crate::matrix::MatrixEngine::new(data_dir).await?;
    let did_restore = engine.restore_session().await.unwrap_or(false);
    if did_restore {
        register_pusher_internal(&engine, endpoint).await?;
    }
    Ok(())
}

pub async fn register_pusher_internal(
    engine: &crate::matrix::MatrixEngine,
    endpoint: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use matrix_sdk::ruma::api::client::push::{Pusher, PusherIds, PusherInit, PusherKind};
    use matrix_sdk::ruma::push::HttpPusherData;

    let client = engine.client().await;

    let ids = PusherIds::new(
        endpoint.to_string(),
        "fi.joonastuomi.Constellations".to_string(),
    );
    let kind = PusherKind::Http(HttpPusherData::new(endpoint.to_string()));
    let init = PusherInit {
        ids,
        kind,
        app_display_name: "Constellations".to_string(),
        device_display_name: "Linux Desktop".to_string(),
        profile_tag: None,
        lang: "en".to_string(),
    };
    let pusher = Pusher::from(init);

    client.pusher().set(pusher, false).await?;
    info!("Registered pusher on homeserver for endpoint: {}", endpoint);
    Ok(())
}

pub fn start_unified_push_listener(engine: crate::matrix::MatrixEngine) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build current-thread runtime");

        rt.block_on(async move {
            info!("Starting UnifiedPush listener in background thread...");
            let storage = UnifiedPushStoragePreferences::new(AppInfo {
                name: "fi.joonastuomi.Constellations",
                author: "Joonas Tuomi",
            });

            let (tx, rx) = mpsc::channel();
            let handle = Handle::current();

            let _up = match UnifiedPush::new("fi.joonastuomi.Constellations", storage, tx, handle)
                .await
            {
                Ok(u) => u,
                Err(e) => {
                    error!("Failed to initialize UnifiedPush: {:?}", e);
                    return;
                }
            };

            if !_up.try_use_default_distributor().await {
                warn!("No default UnifiedPush distributor found on the system.");
            }

            _up.register("default", Some("Matrix Push Notification Connector"), None)
                .await;

            while let Ok(event) = rx.recv() {
                match event {
                    PushEvent::NewEndpoint { endpoint, .. } => {
                        info!("UnifiedPush received new endpoint: {}", endpoint.endpoint);
                        if let Err(e) = register_pusher_internal(&engine, &endpoint.endpoint).await
                        {
                            error!("Failed to register pusher on Matrix homeserver: {:?}", e);
                        }
                    }
                    PushEvent::Message { .. } => {
                        // When the GUI is active, it handles live sync and messages directly,
                        // so we do not need to perform a background sync when push message arrives.
                        info!("UnifiedPush message received while GUI is running (ignored).");
                    }
                    PushEvent::Unregistered { .. } => {
                        info!("UnifiedPush unregistered.");
                    }
                    _ => {}
                }
            }
        });
    });
}
