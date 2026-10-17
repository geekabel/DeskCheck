//! Live changes of the sound server.
//!
//! [`SoundWatcher::start`] subscribes to the events of the server: devices,
//! streams and the server itself (for the defaults). It turns each one into
//! a [`Change`] and sends it on the channel it is given, so changes from the
//! sound server and from GNOME can share one channel.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use deskcheck_core::events::Change;
use libpulse_binding as pulse;
use pulse::context::State as ContextState;
use pulse::context::subscribe::{Facility, InterestMaskSet};
use pulse::time::MicroSeconds;

use crate::client::{Error, connect, describe, run_until, turn};

/// Longest wait of the thread before it checks whether it must stop.
const WAKE_UP: MicroSeconds = MicroSeconds(100_000);

/// What DeskCheck listens to.
fn interest() -> InterestMaskSet {
    InterestMaskSet::SINK
        | InterestMaskSet::SOURCE
        | InterestMaskSet::CARD
        | InterestMaskSet::SINK_INPUT
        | InterestMaskSet::SOURCE_OUTPUT
        | InterestMaskSet::SERVER
}

/// The change an event of the server means, if DeskCheck cares about it.
pub(crate) fn change_for(facility: Facility) -> Option<Change> {
    match facility {
        Facility::Sink | Facility::Source | Facility::Card => Some(Change::SoundDevices),
        Facility::SinkInput | Facility::SourceOutput => Some(Change::SoundStreams),
        // The server sends this when a default device changes.
        Facility::Server => Some(Change::SoundDefaults),
        Facility::Module | Facility::Client | Facility::SampleCache => None,
    }
}

/// Watches the sound server. Dropping it stops the watch.
#[derive(Debug)]
pub struct SoundWatcher {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SoundWatcher {
    /// Starts watching, and sends each change on `changes`.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] when no sound server answers or it refuses the
    /// subscription. A failure later is sent as [`Change::Lost`].
    pub fn start(changes: Sender<Change>) -> Result<Self, Error> {
        let (ready_tx, ready) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("deskcheck-sound-watch".to_owned())
                .spawn(move || run(&stop, &changes, &ready_tx))
                .map_err(|_| Error::Setup("cannot start the sound watch thread"))?
        };
        match ready.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(Error::Mainloop("the sound watch thread stopped".to_owned()))
            }
        }
    }
}

impl Drop for SoundWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn lost(reason: String) -> Change {
    Change::Lost {
        source: "sound",
        reason,
    }
}

/// The body of the watch thread.
fn run(stop: &AtomicBool, changes: &Sender<Change>, ready: &Sender<Result<(), Error>>) {
    let mut connection = match connect() {
        Ok(connection) => connection,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    {
        let changes = changes.clone();
        connection.context.set_subscribe_callback(Some(Box::new(
            move |facility, _operation, _index| {
                if let Some(change) = facility.and_then(change_for) {
                    let _ = changes.send(change);
                }
            },
        )));
    }
    let accepted = Arc::new(AtomicBool::new(false));
    let subscribed = {
        let accepted = Arc::clone(&accepted);
        connection
            .context
            .subscribe(interest(), move |ok| accepted.store(ok, Ordering::Relaxed))
    };
    if let Err(error) = run_until(&mut connection.mainloop, &connection.context, || {
        subscribed.get_state() != pulse::operation::State::Running
    }) {
        let _ = ready.send(Err(error));
        return;
    }
    if !accepted.load(Ordering::Relaxed) {
        let _ = ready.send(Err(Error::Refused {
            what: "watch the sound server".to_owned(),
            reason: describe(connection.context.errno()),
        }));
        return;
    }
    let _ = ready.send(Ok(()));

    while !stop.load(Ordering::Relaxed) {
        if let Err(error) = turn(&mut connection, WAKE_UP) {
            let _ = changes.send(lost(error.to_string()));
            return;
        }
        if matches!(
            connection.context.get_state(),
            ContextState::Failed | ContextState::Terminated
        ) {
            let reason = Error::Disconnected(describe(connection.context.errno()));
            let _ = changes.send(lost(reason.to_string()));
            return;
        }
    }
    connection.context.set_subscribe_callback(None);
    connection.context.disconnect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_become_changes() {
        assert_eq!(change_for(Facility::Sink), Some(Change::SoundDevices));
        assert_eq!(change_for(Facility::Card), Some(Change::SoundDevices));
        assert_eq!(
            change_for(Facility::SourceOutput),
            Some(Change::SoundStreams)
        );
        assert_eq!(change_for(Facility::Server), Some(Change::SoundDefaults));
        assert_eq!(change_for(Facility::Client), None);
    }
}
