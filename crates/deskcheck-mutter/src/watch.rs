//! Live changes of the screens.
//!
//! GNOME sends the D-Bus signal `MonitorsChanged` when screens are plugged,
//! unplugged, turned on or off, or moved. [`ScreenWatcher`] sends a
//! [`Change::Screens`] for each one, on the channel it is given, so changes
//! from GNOME and from the sound server can share one channel.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};

use deskcheck_core::events::Change;

use crate::{CALL_TIMEOUT, Error, call_error, dbus, read_state_with};

/// Watches the screens. Dropping it stops the watch.
#[derive(Debug)]
pub struct ScreenWatcher {
    connection: Option<zbus::blocking::Connection>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ScreenWatcher {
    /// Starts watching on the session bus, and sends each change on
    /// `changes`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoSessionBus`] without a session bus,
    /// [`Error::NotGnome`] outside a GNOME session, or another [`Error`]
    /// when the signal cannot be listened to.
    pub fn start(changes: Sender<Change>) -> Result<Self, Error> {
        let connection = zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.method_timeout(CALL_TIMEOUT).build())
            .map_err(|error| Error::NoSessionBus(error.to_string()))?;
        Self::start_with(&connection, changes)
    }

    /// Starts watching on an existing connection. Tests use it with a fake
    /// GNOME.
    ///
    /// # Errors
    ///
    /// Like [`ScreenWatcher::start`], except for the session bus.
    pub fn start_with(
        connection: &zbus::blocking::Connection,
        changes: Sender<Change>,
    ) -> Result<Self, Error> {
        // Fails at once outside GNOME, instead of waiting for nothing.
        read_state_with(connection)?;
        // Listening starts here, so no change is missed after `start`.
        let signals = dbus::monitors_changed(connection).map_err(|error| call_error(&error))?;
        let stopping = Arc::new(AtomicBool::new(false));
        let thread = {
            let stopping = Arc::clone(&stopping);
            thread::Builder::new()
                .name("deskcheck-screen-watch".to_owned())
                .spawn(move || {
                    for _signal in signals {
                        if changes.send(Change::Screens).is_err() {
                            return;
                        }
                    }
                    if !stopping.load(Ordering::Relaxed) {
                        let _ = changes.send(Change::Lost {
                            source: "screens",
                            reason: "the connection to GNOME was closed".to_owned(),
                        });
                    }
                })
                .map_err(|error| Error::DBus(error.to_string()))?
        };
        Ok(Self {
            connection: Some(connection.clone()),
            stopping,
            thread: Some(thread),
        })
    }
}

impl Drop for ScreenWatcher {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        // Closing the connection ends the signal iterator of the thread.
        if let Some(connection) = self.connection.take() {
            let _ = connection.close();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
