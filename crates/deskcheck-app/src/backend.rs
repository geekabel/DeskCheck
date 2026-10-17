//! The system work of the app, off the interface thread.
//!
//! [`start`] runs a thread that reads the sound and the screens, watches
//! them (DC-16), and sends an [`Update`] to the interface after each
//! change, on an `async_channel` the interface awaits with
//! `glib::spawn_future_local`. The interface asks for work with a
//! [`Request`]. Nothing here blocks the interface: a slow or missing sound
//! server only delays the updates.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use deskcheck_core::audio::AudioState;
use deskcheck_core::display::DisplayState;
use deskcheck_core::events::{Change, coalesce};
use deskcheck_pulse::ServerKind;

/// How long to gather a burst of changes before reading again.
const BURST: Duration = Duration::from_millis(150);

/// What the backend sends to the interface.
#[derive(Debug, Clone)]
pub enum Update {
    /// Which sound server answers, or why none does.
    SoundServer(Result<ServerKind, String>),
    /// The sound, read again.
    Sound(Result<AudioState, String>),
    /// The screens, read again.
    Screens(Result<DisplayState, String>),
}

/// What the interface asks the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Read everything again.
    Refresh,
}

/// What the backend thread waits for: a change or a request.
enum Input {
    Change(Change),
    Request(Request),
}

/// The handle the interface keeps.
#[derive(Debug, Clone)]
pub struct Backend {
    requests: Sender<Request>,
}

impl Backend {
    /// Asks the backend for some work. Does nothing once it stopped.
    pub fn request(&self, request: Request) {
        let _ = self.requests.send(request);
    }
}

fn read_sound() -> Update {
    Update::Sound(deskcheck_pulse::read_state().map_err(|error| error.to_string()))
}

fn read_screens() -> Update {
    Update::Screens(deskcheck_mutter::read_state().map_err(|error| error.to_string()))
}

/// Starts the backend thread. It stops when the interface is gone.
pub fn start(updates: async_channel::Sender<Update>) -> Backend {
    let (inputs, received) = mpsc::channel();
    let (requests, from_interface) = mpsc::channel();
    forward(from_interface, inputs.clone(), Input::Request);
    let _ = thread::Builder::new()
        .name("deskcheck-backend".to_owned())
        .spawn(move || run(&inputs, &received, &updates));
    Backend { requests }
}

/// Moves each message of `from` into `to`, so the backend waits on one
/// channel for both changes and requests.
fn forward<T: Send + 'static>(from: Receiver<T>, to: Sender<Input>, wrap: fn(T) -> Input) {
    let _ = thread::Builder::new()
        .name("deskcheck-forward".to_owned())
        .spawn(move || {
            for message in from {
                if to.send(wrap(message)).is_err() {
                    return;
                }
            }
        });
}

/// The body of the backend thread.
fn run(
    inputs: &Sender<Input>,
    received: &Receiver<Input>,
    updates: &async_channel::Sender<Update>,
) {
    let send = |update: Update| updates.send_blocking(update).is_ok();
    let server = deskcheck_pulse::server_kind().map_err(|error| error.to_string());
    if !(send(Update::SoundServer(server)) && send(read_sound()) && send(read_screens())) {
        return;
    }

    // One channel for the changes of both sources (DC-16).
    let (changes, from_watchers) = mpsc::channel();
    forward(from_watchers, inputs.clone(), Input::Change);
    // Kept alive as long as the thread runs; a missing source is already
    // shown by the first reads above.
    let _sound = deskcheck_pulse::SoundWatcher::start(changes.clone());
    let _screens = deskcheck_mutter::ScreenWatcher::start(changes);

    while let Ok(first) = received.recv() {
        // Gathers the burst that usually follows the first input.
        let mut burst = vec![first];
        let end = Instant::now() + BURST;
        while let Some(wait) = end.checked_duration_since(Instant::now()) {
            match received.recv_timeout(wait) {
                Ok(input) => burst.push(input),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        let refresh = burst
            .iter()
            .any(|input| matches!(input, Input::Request(Request::Refresh)));
        let changes = coalesce(burst.into_iter().filter_map(|input| match input {
            Input::Change(change) => Some(change),
            Input::Request(_) => None,
        }));
        if (refresh || changes.iter().any(Change::is_sound)) && !send(read_sound()) {
            return;
        }
        if (refresh || changes.contains(&Change::Screens)) && !send(read_screens()) {
            return;
        }
    }
}
