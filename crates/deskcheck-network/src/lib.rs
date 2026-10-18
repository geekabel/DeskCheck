//! Network backend of DeskCheck: the name of the Wi-Fi in use, one of the
//! clues that tell a place.
//!
//! It asks `NetworkManager` on the system bus, through its D-Bus interface:
//! the active connections (`ActiveConnections`), the one of type
//! `802-11-wireless`, its device (`Devices`), the access point that device
//! uses (`ActiveAccessPoint`), and the name of that access point (`Ssid`,
//! an array of bytes).
//!
//! ```no_run
//! match deskcheck_network::wifi_name()? {
//!     Some(name) => println!("On the Wi-Fi {name}"),
//!     None => println!("No Wi-Fi in use"),
//! }
//! # Ok::<(), deskcheck_network::Error>(())
//! ```

use std::fmt;
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::proxy;
use zbus::zvariant::OwnedObjectPath;

/// How long to wait for `NetworkManager` to answer.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// The well-known name of `NetworkManager`.
const SERVICE: &str = "org.freedesktop.NetworkManager";

/// The type of a Wi-Fi connection.
const WIFI: &str = "802-11-wireless";

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager",
    gen_async = false
)]
trait NetworkManager {
    /// The active connections.
    #[zbus(property)]
    fn active_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager",
    gen_async = false
)]
trait Active {
    /// The type of the connection, like `802-11-wireless`.
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<String>;

    /// The devices of the connection.
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager",
    gen_async = false
)]
trait Wireless {
    /// The access point the device uses.
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager",
    gen_async = false
)]
trait AccessPoint {
    /// The name of the network, as bytes.
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;
}

/// What can go wrong while reading the Wi-Fi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// There is no system bus to talk to.
    NoSystemBus(String),
    /// `NetworkManager` is not running.
    NoNetworkManager,
    /// Any other D-Bus error.
    DBus(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSystemBus(reason) => write!(f, "cannot reach the system bus: {reason}"),
            Self::NoNetworkManager => f.write_str("NetworkManager is not running"),
            Self::DBus(reason) => write!(f, "cannot read the Wi-Fi: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

fn call_error(error: &zbus::Error) -> Error {
    let missing = match error {
        zbus::Error::MethodError(name, _, _) => matches!(
            name.as_str(),
            "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner"
        ),
        zbus::Error::FDO(fdo) => matches!(
            **fdo,
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_)
        ),
        _ => false,
    };
    if missing {
        Error::NoNetworkManager
    } else {
        Error::DBus(error.to_string())
    }
}

/// The name of a network, from its bytes. Names are almost always UTF-8;
/// other bytes are shown as replacement characters.
#[must_use]
pub fn ssid_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The name of the Wi-Fi in use, on an existing connection, or `None`
/// when no Wi-Fi connection is active.
///
/// # Errors
///
/// Returns [`Error::NoNetworkManager`] when the service is not running,
/// or [`Error::DBus`] for any other failure.
pub fn wifi_name_with(connection: &Connection) -> Result<Option<String>, Error> {
    let err = |error: zbus::Error| call_error(&error);
    // Properties are read when asked, not kept: always fresh.
    let manager = NetworkManagerProxy::builder(connection)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .map_err(err)?;
    for path in manager.active_connections().map_err(err)? {
        let active = ActiveProxy::builder(connection)
            .path(path)
            .map_err(err)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .map_err(err)?;
        if active.kind().map_err(err)? != WIFI {
            continue;
        }
        for device in active.devices().map_err(err)? {
            let wireless = WirelessProxy::builder(connection)
                .path(device)
                .map_err(err)?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .map_err(err)?;
            let point = wireless.active_access_point().map_err(err)?;
            // "/" means no access point.
            if point.as_str() == "/" {
                continue;
            }
            let access_point = AccessPointProxy::builder(connection)
                .path(point)
                .map_err(err)?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .map_err(err)?;
            let ssid = access_point.ssid().map_err(err)?;
            if !ssid.is_empty() {
                return Ok(Some(ssid_text(&ssid)));
            }
        }
    }
    Ok(None)
}

/// Opens a connection to the system bus, where `NetworkManager` is.
///
/// # Errors
///
/// Returns [`Error::NoSystemBus`] without a system bus.
pub fn system_bus() -> Result<Connection, Error> {
    zbus::blocking::connection::Builder::system()
        .and_then(|builder| builder.method_timeout(CALL_TIMEOUT).build())
        .map_err(|error| Error::NoSystemBus(error.to_string()))
}

/// The name of the Wi-Fi in use, from `NetworkManager` on the system bus.
///
/// # Errors
///
/// Returns [`Error::NoSystemBus`] without a system bus, or the errors of
/// [`wifi_name_with`].
pub fn wifi_name() -> Result<Option<String>, Error> {
    wifi_name_with(&system_bus()?)
}

/// Reads the Wi-Fi again and again on one connection, opened when first
/// needed and opened again after a failure.
#[derive(Default)]
pub struct WifiReader {
    connection: Option<Connection>,
}

impl WifiReader {
    /// A reader, not connected yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The name of the Wi-Fi in use, as [`wifi_name`].
    ///
    /// # Errors
    ///
    /// The same as [`wifi_name`].
    pub fn read(&mut self) -> Result<Option<String>, Error> {
        if self.connection.is_none() {
            self.connection = Some(system_bus()?);
        }
        let connection = self.connection.as_ref().ok_or(Error::NoNetworkManager)?;
        let name = wifi_name_with(connection);
        if name.is_err() {
            self.connection = None;
        }
        name
    }
}

/// Name of the D-Bus service read by this crate.
#[must_use]
pub fn service_name() -> &'static str {
    SERVICE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_read_from_bytes() {
        assert_eq!(ssid_text(b"UTBM"), "UTBM");
        assert_eq!(ssid_text("Café".as_bytes()), "Café");
        assert_eq!(ssid_text(&[0x55, 0xff]), "U\u{fffd}");
    }

    #[test]
    fn a_missing_service_is_told() {
        let error = zbus::Error::FDO(Box::new(zbus::fdo::Error::ServiceUnknown("x".to_owned())));
        assert_eq!(call_error(&error), Error::NoNetworkManager);
        assert_eq!(service_name(), "org.freedesktop.NetworkManager");
    }
}
