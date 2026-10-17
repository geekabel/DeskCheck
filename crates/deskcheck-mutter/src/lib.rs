//! Screen backend of DeskCheck.
//!
//! It reads the screens from GNOME, through the D-Bus interface
//! `org.gnome.Mutter.DisplayConfig` on the session bus. Mutter is the
//! window manager of GNOME, so this only works in a GNOME session.
//!
//! ```no_run
//! let state = deskcheck_mutter::read_state()?;
//! for screen in state.enabled() {
//!     println!("{} on {}", screen.id, screen.connector);
//! }
//! # Ok::<(), deskcheck_mutter::Error>(())
//! ```

use std::fmt;
use std::io::ErrorKind;
use std::time::Duration;

use deskcheck_core::display::DisplayState;

pub use watch::ScreenWatcher;

mod convert;
pub mod dbus;
pub mod pointer;
mod raw;
pub mod setup;
mod watch;

/// How long to wait for GNOME to answer a call.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// What can go wrong while reading the screens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// There is no session bus to talk to.
    NoSessionBus(String),
    /// The session has no `org.gnome.Mutter.DisplayConfig` service, so it
    /// is not a GNOME session.
    NotGnome,
    /// GNOME did not answer in time ([`CALL_TIMEOUT`]).
    NoAnswer,
    /// Any other D-Bus error.
    DBus(String),
    /// GNOME or DeskCheck refused a change of the screens.
    Refused(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSessionBus(reason) => {
                write!(f, "cannot reach the session bus: {reason}")
            }
            Self::NotGnome => f.write_str(
                "this session is not a GNOME session: the screen service of GNOME \
                 (org.gnome.Mutter.DisplayConfig) is not running",
            ),
            Self::NoAnswer => write!(
                f,
                "GNOME did not answer within {} seconds",
                CALL_TIMEOUT.as_secs()
            ),
            Self::DBus(reason) => write!(f, "cannot read the screens from GNOME: {reason}"),
            Self::Refused(reason) => write!(f, "the screens cannot be changed: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

/// D-Bus error names that mean "nobody owns the service name".
fn means_no_service(error_name: &str) -> bool {
    matches!(
        error_name,
        "org.freedesktop.DBus.Error.ServiceUnknown" | "org.freedesktop.DBus.Error.NameHasNoOwner"
    )
}

fn call_error(error: &zbus::Error) -> Error {
    match error {
        zbus::Error::MethodError(name, _, _) if means_no_service(name.as_str()) => Error::NotGnome,
        zbus::Error::FDO(fdo) if matches!(**fdo, zbus::fdo::Error::ServiceUnknown(_)) => {
            Error::NotGnome
        }
        zbus::Error::InputOutput(io) if io.kind() == ErrorKind::TimedOut => Error::NoAnswer,
        _ => Error::DBus(error.to_string()),
    }
}

/// Reads the screens on an existing D-Bus connection.
///
/// [`read_state`] uses the session bus. Tests use this function with a
/// fake service.
///
/// # Errors
///
/// Returns [`Error::NotGnome`] when no screen service is running,
/// [`Error::NoAnswer`] when the call times out (set the timeout on the
/// connection), or [`Error::DBus`] for any other D-Bus failure.
pub fn read_state_with(connection: &zbus::blocking::Connection) -> Result<DisplayState, Error> {
    let wire = dbus::current_state(connection).map_err(|error| call_error(&error))?;
    Ok(convert::display_state(&dbus::to_raw(wire)))
}

/// Connects to the session bus and reads the screens from GNOME.
///
/// # Errors
///
/// Returns [`Error::NoSessionBus`] without a session bus,
/// [`Error::NotGnome`] outside a GNOME session, [`Error::NoAnswer`] when
/// GNOME does not answer within [`CALL_TIMEOUT`], or [`Error::DBus`] for
/// any other D-Bus failure.
pub fn read_state() -> Result<DisplayState, Error> {
    read_state_with(&session()?)
}

/// Reads what is needed to change the screens, on an existing connection.
///
/// # Errors
///
/// The same as [`read_state_with`].
pub fn read_setup_with(connection: &zbus::blocking::Connection) -> Result<setup::Setup, Error> {
    let wire = dbus::current_state(connection).map_err(|error| call_error(&error))?;
    Ok(setup::Setup::from_wire(wire))
}

/// Reads what is needed to change the screens, on the session bus.
///
/// # Errors
///
/// The same as [`read_state`].
pub fn read_setup() -> Result<setup::Setup, Error> {
    read_setup_with(&session()?)
}

/// `ApplyMonitorsConfig` methods.
const VERIFY: u32 = 0;
const PERSISTENT: u32 = 2;

/// Applies a change of the screens on an existing connection.
///
/// The layout is read again, changed, checked by DeskCheck with the rules
/// of GNOME, then checked by GNOME itself (method "verify"), and only then
/// applied for good (method "persistent"). GNOME then shows its own
/// dialog to keep the change, and goes back to the old layout after 20
/// seconds without an answer.
///
/// # Errors
///
/// Returns [`Error::Refused`] when the change breaks a rule or GNOME
/// refuses it, or the errors of [`read_state_with`].
pub fn apply_with(
    connection: &zbus::blocking::Connection,
    change: &setup::Change,
) -> Result<(), Error> {
    let setup = read_setup_with(connection)?;
    let plan = setup::Plan::current(&setup)
        .change(&setup, change)
        .map_err(|error| Error::Refused(error.to_string()))?;
    let logical: Vec<dbus::WireLogicalConfig> = plan
        .logical
        .iter()
        .map(|l| {
            (
                l.x,
                l.y,
                l.scale,
                l.transform,
                l.primary,
                l.monitors
                    .iter()
                    .map(|(connector, mode)| {
                        (connector.clone(), mode.clone(), dbus::Properties::new())
                    })
                    .collect(),
            )
        })
        .collect();
    // The layout mode may be sent only when GNOME says it can change.
    let mut properties = dbus::Properties::new();
    if setup.supports_changing_layout_mode {
        let mode: u32 = match setup.layout_mode {
            setup::LayoutMode::Logical => 1,
            setup::LayoutMode::Physical => 2,
        };
        properties.insert("layout-mode".to_owned(), mode.into());
    }
    let refused = |error: zbus::Error| match error {
        zbus::Error::MethodError(_, Some(reason), _) => Error::Refused(reason),
        other => call_error(&other),
    };
    dbus::apply_config(connection, setup.serial, VERIFY, &logical, &properties).map_err(refused)?;
    dbus::apply_config(connection, setup.serial, PERSISTENT, &logical, &properties).map_err(refused)
}

/// Applies a change of the screens on the session bus (see [`apply_with`]).
///
/// # Errors
///
/// The same as [`apply_with`].
pub fn apply(change: &setup::Change) -> Result<(), Error> {
    apply_with(&session()?, change)
}

fn session() -> Result<zbus::blocking::Connection, Error> {
    zbus::blocking::connection::Builder::session()
        .and_then(|builder| builder.method_timeout(CALL_TIMEOUT).build())
        .map_err(|error| Error::NoSessionBus(error.to_string()))
}

/// Name of the D-Bus service read by this crate.
#[must_use]
pub fn service_name() -> &'static str {
    dbus::SERVICE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_owner_errors_mean_not_gnome() {
        assert!(means_no_service(
            "org.freedesktop.DBus.Error.ServiceUnknown"
        ));
        assert!(means_no_service(
            "org.freedesktop.DBus.Error.NameHasNoOwner"
        ));
        assert!(!means_no_service("org.freedesktop.DBus.Error.AccessDenied"));
    }

    #[test]
    fn errors_have_clear_messages() {
        assert!(Error::NotGnome.to_string().contains("not a GNOME session"));
        assert!(
            Error::NoSessionBus("no address".to_owned())
                .to_string()
                .contains("session bus")
        );
    }
}
