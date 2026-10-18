//! The Wi-Fi name, read from a fake `NetworkManager` over a peer-to-peer
//! D-Bus connection: an ethernet connection, then a Wi-Fi one.

use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::zvariant::OwnedObjectPath;

fn path(text: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(text).unwrap()
}

struct Manager {
    active: Vec<&'static str>,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        self.active.iter().map(|p| path(p)).collect()
    }
}

struct Active {
    kind: &'static str,
    device: &'static str,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> String {
        self.kind.to_owned()
    }

    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(self.device)]
    }
}

struct Wireless;

#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[allow(clippy::unused_self, reason = "D-Bus properties take &self")]
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        path("/org/freedesktop/NetworkManager/AccessPoint/7")
    }
}

struct AccessPoint;

#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[allow(clippy::unused_self, reason = "D-Bus properties take &self")]
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        b"UTBM-Office".to_vec()
    }
}

fn connect(active: Vec<&'static str>) -> (Connection, Connection) {
    let (server_end, client_end) = UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let server = thread::spawn(move || {
        Builder::unix_stream(server_end)
            .server(guid)
            .unwrap()
            .p2p()
            .serve_at("/org/freedesktop/NetworkManager", Manager { active })
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/ActiveConnection/1",
                Active {
                    kind: "802-3-ethernet",
                    device: "/org/freedesktop/NetworkManager/Devices/1",
                },
            )
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/ActiveConnection/2",
                Active {
                    kind: "802-11-wireless",
                    device: "/org/freedesktop/NetworkManager/Devices/2",
                },
            )
            .unwrap()
            .serve_at("/org/freedesktop/NetworkManager/Devices/2", Wireless)
            .unwrap()
            .serve_at("/org/freedesktop/NetworkManager/AccessPoint/7", AccessPoint)
            .unwrap()
            .build()
            .unwrap()
    });
    let client = Builder::unix_stream(client_end)
        .p2p()
        .method_timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    (server.join().unwrap(), client)
}

#[test]
fn the_wifi_name_is_read() {
    let (_server, client) = connect(vec![
        "/org/freedesktop/NetworkManager/ActiveConnection/1",
        "/org/freedesktop/NetworkManager/ActiveConnection/2",
    ]);
    assert_eq!(
        deskcheck_network::wifi_name_with(&client),
        Ok(Some("UTBM-Office".to_owned()))
    );
}

#[test]
fn without_wifi_there_is_no_name() {
    let (_server, client) = connect(vec!["/org/freedesktop/NetworkManager/ActiveConnection/1"]);
    assert_eq!(deskcheck_network::wifi_name_with(&client), Ok(None));
}
