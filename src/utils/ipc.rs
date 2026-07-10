use std::error::Error;
use tokio::sync::mpsc;
use zbus::names::WellKnownName;
use zbus::{Connection, interface, proxy};

pub const DBUS_NAME: &str = "fi.joonastuomi.Constellations";
pub const DBUS_PATH: &str = "/fi/joonastuomi/Constellations";

pub struct IpcInterface {
    tx: mpsc::UnboundedSender<String>,
}

#[interface(name = "fi.joonastuomi.Constellations.Ipc")]
impl IpcInterface {
    async fn handle_callback(&self, uri: String) {
        // Forward every URI unchanged; classification (OIDC callback vs Matrix
        // permalink) happens at the consumer side in the app layer. This lets
        // the single-instance relay carry both OIDC login completions and
        // `matrix.to` / `matrix:` permalinks handed to us by the desktop.
        tracing::info!("Received URI via D-Bus IPC: {uri}");
        let _ = self.tx.send(uri);
    }
}

#[proxy(
    interface = "fi.joonastuomi.Constellations.Ipc",
    default_service = "fi.joonastuomi.Constellations",
    default_path = "/fi/joonastuomi/Constellations"
)]
pub trait Ipc {
    fn handle_callback(&self, uri: String) -> zbus::Result<()>;
}

pub async fn start_server(tx: mpsc::UnboundedSender<String>) -> Result<Connection, Box<dyn Error>> {
    let connection = Connection::session().await?;
    connection
        .object_server()
        .at(DBUS_PATH, IpcInterface { tx })
        .await?;
    let name = WellKnownName::try_from(DBUS_NAME)?;
    connection.request_name(name).await?;
    Ok(connection)
}

pub async fn call_handle_callback(uri: String) -> Result<(), Box<dyn Error>> {
    let connection = Connection::session().await?;
    let proxy = IpcProxy::new(&connection).await?;
    proxy.handle_callback(uri).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use tokio::sync::mpsc;

    #[tokio::test]
    #[serial]
    async fn test_call_handle_callback() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        // Start the server which claims the DBus name
        let _server_conn = start_server(tx).await.expect("Failed to start DBus server");

        // The valid callback URI must start with fi.joonastuomi.constellations:/callback
        let valid_uri = "fi.joonastuomi.constellations:/callback?code=12345".to_string();
        call_handle_callback(valid_uri.clone())
            .await
            .expect("Failed to call proxy");

        // The server should receive the URI on the mpsc channel
        let received = rx.recv().await.expect("Did not receive URI on channel");
        assert_eq!(received, valid_uri);
    }

    #[tokio::test]
    #[serial]
    async fn test_call_handle_callback_forwards_non_oidc_uri() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        // Start the server which claims the DBus name
        let _server_conn = start_server(tx).await.expect("Failed to start DBus server");

        // A non-OIDC URI (e.g. a Matrix permalink) is now forwarded unchanged;
        // classification happens at the consumer side, not in the IPC layer.
        let permalink = "https://matrix.to/#/!abc:example.org".to_string();
        call_handle_callback(permalink.clone())
            .await
            .expect("Failed to call proxy");

        let received = rx.recv().await.expect("Did not receive URI on channel");
        assert_eq!(received, permalink);
    }

    #[tokio::test]
    #[serial]
    async fn test_start_server_dbus_error() {
        // Save the original DBUS_SESSION_BUS_ADDRESS
        let original_dbus_address = env::var("DBUS_SESSION_BUS_ADDRESS").ok();

        // Mock a DBus error by setting an invalid session bus address
        unsafe {
            env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent");

            let (tx, _rx) = mpsc::unbounded_channel();
            let result = start_server(tx).await;

            // Restore the original address to not affect other tests
            if let Some(addr) = original_dbus_address {
                env::set_var("DBUS_SESSION_BUS_ADDRESS", addr);
            } else {
                env::remove_var("DBUS_SESSION_BUS_ADDRESS");
            }
            // The function should return an error since the session bus is unreachable
            assert!(
                result.is_err(),
                "Expected an error when DBUS_SESSION_BUS_ADDRESS is invalid"
            );
        }
    }
}
