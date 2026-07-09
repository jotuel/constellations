#![recursion_limit = "256"]

pub mod constellations;
mod matrix;
pub mod settings;
pub mod utils;
mod view;

pub use constellations::{AuthFlow, Constellations, MenuAct, Message, QrLoginStep, SettingsPanel};
pub use cosmic::Core;
pub use matrix_sdk::ruma::OwnedRoomId;
pub use matrix_sdk::ruma::events::room::MediaSource;
pub use url::Url;
pub use utils::item::ConstellationsItem;
pub use utils::preview::{PreviewEvent, parse_markdown, parse_plain_text};
pub use utils::{
    ApplyVectorDiffExt, contains_ignore_ascii_case, fuzzy_match_ignore_case, redact_url,
};

pub use utils::i18n;
pub(crate) use utils::ipc;
pub use utils::item;
pub use utils::preview;
pub use utils::rich_text;
pub use utils::unified_push;

use anyhow::Result;
use mimalloc::MiMalloc;
use std::sync::LazyLock;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
pub const CONSTELLATIONS_ICON: &[u8] = include_bytes!("../res/const.svg");

pub static TIMELINE_ID: LazyLock<cosmic::iced::widget::Id> =
    LazyLock::new(cosmic::iced::widget::Id::unique);
pub static THREADED_TIMELINE_ID: LazyLock<cosmic::iced::widget::Id> =
    LazyLock::new(cosmic::iced::widget::Id::unique);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    LazyLock::force(&i18n::LOAD_LOCALIZATION);

    let env_filter = if cfg!(debug_assertions) {
        "matrix_sdk=debug,matrix_sdk_ui=debug,constellations=debug"
    } else {
        "warn"
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();
    let args: Vec<String> = std::env::args().collect();
    let is_notify = args.iter().any(|arg| arg == "--notify");
    // Accept `argv[1]` if it is our own URI scheme (OIDC callback or an
    // app-wrapped permalink) OR a raw Matrix permalink (`matrix.to` / `matrix:`).
    let uri = args
        .get(1)
        .filter(|u| {
            u.starts_with("fi.joonastuomi.Constellations://") || utils::permalink::parse(u).is_ok()
        })
        .cloned();

    let rt = tokio::runtime::Runtime::new()?;
    let is_running = rt.block_on(async {
        let connection = match zbus::Connection::session().await {
            Ok(conn) => conn,
            Err(_) => return false,
        };
        let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
            Ok(proxy) => proxy,
            Err(_) => return false,
        };
        dbus.name_has_owner(ipc::DBUS_NAME.try_into().unwrap())
            .await
            .unwrap_or(false)
    });

    if is_running {
        if is_notify {
            tracing::info!("App is already running; delegate push to active instance.");
            return Ok(());
        }
        if let Some(uri) = uri {
            rt.block_on(async {
                if let Err(e) = ipc::call_handle_callback(uri).await {
                    tracing::error!("Failed to send URI to existing instance: {}", e);
                }
            });
        }
        tracing::info!("Another instance is already running, exiting.");
        return Ok(());
    }

    if is_notify {
        // Run the headless push handler on a dedicated thread with its own
        // runtime. unifiedpush's internals call `block_on`, which panics with
        // "Cannot start a runtime from within a runtime" if invoked from inside
        // the main multi-thread runtime via `rt.block_on`.
        let handle = std::thread::spawn(|| {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to build headless runtime: {e}");
                    return;
                }
            };
            rt.block_on(async {
                if let Err(e) = unified_push::run_headless_notification_handler().await {
                    tracing::error!("Failed to run headless notification handler: {e}");
                }
            });
        });
        if handle.join().is_err() {
            return Err(anyhow::anyhow!("headless notification handler thread panicked").into());
        }
        return Ok(());
    }

    let _guard = rt.enter();

    cosmic::app::run::<Constellations>(cosmic::app::Settings::default(), uri)?;
    Ok(())
}
