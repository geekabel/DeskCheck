//! Where the mouse is on the desktop, to tell which screen it is on.
//!
//! - On X11, any app may ask the X server: `QueryPointer` on the root
//!   window gives the place of the pointer on the whole desktop.
//! - On Wayland, an app only sees the pointer over its own windows. GNOME
//!   Shell knows it, so DeskCheck comes with a small extension of GNOME
//!   Shell (`gnome-shell-extension/` in the repository) that answers on
//!   the session bus. Without it, [`PointerError::NeedsExtension`].
//!
//! The place is in the coordinates of the desktop of GNOME, the same as
//! the places of the screens: see
//! [`deskcheck_core::display::DisplayState::screen_at`].

use std::fmt;

use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use zbus::proxy;

use crate::CALL_TIMEOUT;

/// The kind of graphical session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    /// The X server: screens sized with their mode.
    X11,
    /// Wayland: screens sized with their mode divided by their scale.
    Wayland,
}

impl Session {
    /// The session, from `XDG_SESSION_TYPE`, or `None` outside one.
    #[must_use]
    pub fn from_type(session_type: Option<&str>) -> Option<Self> {
        match session_type?.to_ascii_lowercase().as_str() {
            "x11" => Some(Self::X11),
            "wayland" => Some(Self::Wayland),
            _ => None,
        }
    }

    /// The session DeskCheck runs in.
    #[must_use]
    pub fn current() -> Option<Self> {
        Self::from_type(std::env::var("XDG_SESSION_TYPE").ok().as_deref())
    }
}

/// Why the place of the mouse is not known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerError {
    /// Not in an X11 or Wayland session.
    NoSession,
    /// On Wayland, the extension of DeskCheck for GNOME Shell is not on.
    NeedsExtension,
    /// The X server or GNOME did not answer.
    Failed(String),
}

impl fmt::Display for PointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSession => f.write_str("not in an X11 or Wayland session"),
            Self::NeedsExtension => f.write_str(
                "on Wayland, the place of the mouse needs the DeskCheck extension for GNOME Shell",
            ),
            Self::Failed(reason) => write!(f, "cannot read the place of the mouse: {reason}"),
        }
    }
}

impl std::error::Error for PointerError {}

/// Well-known name, object path and interface of the extension.
pub const EXTENSION_SERVICE: &str = "io.github.geekabel.DeskCheck.Pointer";

#[proxy(
    interface = "io.github.geekabel.DeskCheck.Pointer",
    default_service = "io.github.geekabel.DeskCheck.Pointer",
    default_path = "/io/github/geekabel/DeskCheck/Pointer",
    gen_async = false
)]
trait Pointer {
    /// The place of the pointer on the desktop.
    fn get_position(&self) -> zbus::Result<(i32, i32)>;
}

/// How DeskCheck asks for the place of the mouse.
enum Source {
    X11 {
        // Boxed: the connection to the X server is much bigger than the
        // one to the bus.
        connection: Box<RustConnection>,
        root: u32,
    },
    Wayland(zbus::blocking::Connection),
}

/// Reads the place of the mouse. Kept open: each read is one short call.
pub struct PointerReader {
    source: Source,
}

impl PointerReader {
    /// Opens the way to read the mouse in this session.
    ///
    /// # Errors
    ///
    /// Returns [`PointerError::NoSession`] outside a graphical session, or
    /// [`PointerError::Failed`] when the X server or the bus cannot be
    /// reached.
    pub fn new() -> Result<Self, PointerError> {
        let source = match Session::current().ok_or(PointerError::NoSession)? {
            Session::X11 => {
                let (connection, screen) = x11rb::connect(None)
                    .map_err(|error| PointerError::Failed(error.to_string()))?;
                let root = connection
                    .setup()
                    .roots
                    .get(screen)
                    .map(|screen| screen.root)
                    .ok_or_else(|| PointerError::Failed("no root window".to_owned()))?;
                Source::X11 {
                    connection: Box::new(connection),
                    root,
                }
            }
            Session::Wayland => Source::Wayland(
                zbus::blocking::connection::Builder::session()
                    .and_then(|builder| builder.method_timeout(CALL_TIMEOUT).build())
                    .map_err(|error| PointerError::Failed(error.to_string()))?,
            ),
        };
        Ok(Self { source })
    }

    /// The session this reader works in.
    #[must_use]
    pub fn session(&self) -> Session {
        match self.source {
            Source::X11 { .. } => Session::X11,
            Source::Wayland(_) => Session::Wayland,
        }
    }

    /// The place of the mouse on the desktop.
    ///
    /// # Errors
    ///
    /// Returns [`PointerError::NeedsExtension`] on Wayland without the
    /// extension, or [`PointerError::Failed`].
    pub fn position(&self) -> Result<(i32, i32), PointerError> {
        match &self.source {
            Source::X11 { connection, root } => {
                let reply = connection
                    .query_pointer(*root)
                    .map_err(|error| PointerError::Failed(error.to_string()))?
                    .reply()
                    .map_err(|error| PointerError::Failed(error.to_string()))?;
                Ok((i32::from(reply.root_x), i32::from(reply.root_y)))
            }
            Source::Wayland(connection) => {
                let proxy = PointerProxy::new(connection).map_err(|e| extension_error(&e))?;
                proxy.get_position().map_err(|e| extension_error(&e))
            }
        }
    }
}

/// A missing extension is told apart from other failures.
fn extension_error(error: &zbus::Error) -> PointerError {
    let missing = match error {
        zbus::Error::MethodError(name, _, _) => matches!(
            name.as_str(),
            "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner"
                | "org.freedesktop.DBus.Error.UnknownObject"
        ),
        zbus::Error::FDO(fdo) => matches!(
            **fdo,
            zbus::fdo::Error::ServiceUnknown(_)
                | zbus::fdo::Error::NameHasNoOwner(_)
                | zbus::fdo::Error::UnknownObject(_)
        ),
        _ => false,
    };
    if missing {
        PointerError::NeedsExtension
    } else {
        PointerError::Failed(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_comes_from_its_type() {
        assert_eq!(Session::from_type(Some("x11")), Some(Session::X11));
        assert_eq!(Session::from_type(Some("Wayland")), Some(Session::Wayland));
        assert_eq!(Session::from_type(Some("tty")), None);
        assert_eq!(Session::from_type(None), None);
    }

    #[test]
    fn a_missing_extension_is_told() {
        let missing = zbus::Error::FDO(Box::new(zbus::fdo::Error::ServiceUnknown(
            "no owner".to_owned(),
        )));
        assert_eq!(extension_error(&missing), PointerError::NeedsExtension);
        let other = zbus::Error::Failure("broken".to_owned());
        assert!(matches!(extension_error(&other), PointerError::Failed(_)));
        assert!(
            PointerError::NeedsExtension
                .to_string()
                .contains("extension for GNOME Shell")
        );
    }
}
