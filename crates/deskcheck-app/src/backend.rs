//! The system work of the app, off the interface thread.
//!
//! [`start`] runs a thread that reads the sound and the screens, watches
//! them (DC-16), and sends an [`Update`] to the interface after each
//! change, on an `async_channel` the interface awaits with
//! `glib::spawn_future_local`. It also runs the level meters of the Now
//! page and the changes the interface asks for with a [`Request`].
//!
//! Short changes (volume, default device) run in the backend thread. Sound
//! tests last seconds, so each runs in a thread of its own and reports with
//! [`Update::Message`]. Nothing here blocks the interface.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use deskcheck_core::audio::{AudioState, DeviceId, InputKind, Target, Volume, tone};
use deskcheck_core::display::DisplayState;
use deskcheck_core::events::{Change, coalesce};
use deskcheck_pulse::{Action, Meters, ServerKind};

/// How long to gather a burst of changes before reading again.
const BURST: Duration = Duration::from_millis(150);

/// How often the backend looks at the level meters.
const METER_TICK: Duration = Duration::from_millis(40);

/// How long a microphone test records.
const TEST_RECORDING: Duration = Duration::from_secs(3);

/// What the backend sends to the interface.
#[derive(Debug, Clone)]
pub enum Update {
    /// Which sound server answers, or why none does.
    SoundServer(Result<ServerKind, String>),
    /// The sound, read again.
    Sound(Result<AudioState, String>),
    /// The screens, read again.
    Screens(Result<DisplayState, String>),
    /// A new peak of a level meter, from 0.0 to 1.0.
    Peak {
        /// The source measured: a microphone, or the monitor of an output.
        source: DeviceId,
        /// The loudest sample since the last peak.
        peak: f32,
    },
    /// A sentence for the user, about a request that ended.
    Message(String),
}

/// What the interface asks the backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Read everything again.
    Refresh,
    /// Make an output or an input the default one.
    SetDefault(Target),
    /// Set the volume of a device.
    SetVolume(Target, Volume),
    /// Play the test chime on an output.
    TestOutput(DeviceId),
    /// Record a few seconds from a microphone, then play them on an output.
    TestInput {
        /// The microphone.
        input: DeviceId,
        /// Where to play the recording back.
        output: DeviceId,
    },
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

fn read_sound() -> Result<AudioState, String> {
    deskcheck_pulse::read_state().map_err(|error| error.to_string())
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

/// The sources the Now page measures: the default microphone, and the
/// monitor of the default output (to show what comes out).
pub fn metered_sources(state: &AudioState) -> Vec<DeviceId> {
    let mut sources = Vec::new();
    if let Some(mic) = state.default_input_device().filter(|i| !i.is_monitor()) {
        sources.push(mic.id.clone());
    }
    if let Some(output) = &state.default_output {
        let monitor = state
            .inputs
            .iter()
            .find(|input| matches!(&input.kind, InputKind::Monitor { of } if of == output));
        if let Some(monitor) = monitor {
            sources.push(monitor.id.clone());
        }
    }
    sources
}

/// The level meters, started again when the measured sources change.
#[derive(Default)]
struct MeterSet {
    sources: Vec<DeviceId>,
    meters: Option<Meters>,
}

impl MeterSet {
    fn follow(&mut self, state: &AudioState) {
        let wanted = metered_sources(state);
        if wanted == self.sources && (self.meters.is_some() || wanted.is_empty()) {
            return;
        }
        // Stops the old meters before starting new ones.
        self.meters = None;
        self.meters = if wanted.is_empty() {
            None
        } else {
            Meters::start(&wanted).ok()
        };
        self.sources = wanted;
    }

    /// Sends the peaks that came in. Returns `false` when the interface
    /// is gone.
    fn send_peaks(&self, send: &impl Fn(Update) -> bool) -> bool {
        let Some(meters) = &self.meters else {
            return true;
        };
        while let Ok(event) = meters.events().try_recv() {
            if let deskcheck_pulse::Event::Peak { input, peak } = event {
                if !send(Update::Peak {
                    source: input,
                    peak,
                }) {
                    return false;
                }
            }
        }
        true
    }
}

fn error_text(error: &deskcheck_pulse::Error) -> String {
    let mut text = error.to_string();
    if let Some(first) = text.get(..1) {
        let upper = first.to_uppercase();
        text.replace_range(..1, &upper);
    }
    text
}

/// Runs a sound test in a thread of its own, and reports with a message.
fn run_test(request: Request, updates: async_channel::Sender<Update>) {
    let _ = thread::Builder::new()
        .name("deskcheck-sound-test".to_owned())
        .spawn(move || {
            let message = match request {
                Request::TestOutput(output) => {
                    let chime = tone::chime(deskcheck_pulse::TEST_RATE);
                    match deskcheck_pulse::play(&output, &chime) {
                        Ok(()) => "Test sound played. Did you hear two notes?".to_owned(),
                        Err(error) => error_text(&error),
                    }
                }
                Request::TestInput { input, output } => {
                    match deskcheck_pulse::record(&input, TEST_RECORDING, |_| {}) {
                        Ok(samples) if tone::is_silence(&samples) => {
                            "The microphone heard almost nothing. Is it muted, or not the one near you?"
                                .to_owned()
                        }
                        Ok(samples) => match deskcheck_pulse::play(&output, &samples) {
                            Ok(()) => "Recording played back. Did you hear yourself?".to_owned(),
                            Err(error) => error_text(&error),
                        },
                        Err(error) => error_text(&error),
                    }
                }
                _ => return,
            };
            let _ = updates.send_blocking(Update::Message(message));
        });
}

/// Runs a short change. Returns a message when it failed.
fn apply(request: &Request) -> Option<String> {
    let action = match request {
        Request::SetDefault(Target::Output(id)) => Action::SetDefaultOutput(id.clone()),
        Request::SetDefault(Target::Input(id)) => Action::SetDefaultInput(id.clone()),
        Request::SetVolume(target, volume) => Action::SetVolume(target.clone(), *volume),
        Request::Refresh | Request::TestOutput(_) | Request::TestInput { .. } => return None,
    };
    deskcheck_pulse::apply(&action)
        .err()
        .map(|error| error_text(&error))
}

/// The body of the backend thread.
fn run(
    inputs: &Sender<Input>,
    received: &Receiver<Input>,
    updates: &async_channel::Sender<Update>,
) {
    let send = |update: Update| updates.send_blocking(update).is_ok();
    let mut meters = MeterSet::default();
    let send_sound = |meters: &mut MeterSet| {
        let sound = read_sound();
        if let Ok(state) = &sound {
            meters.follow(state);
        }
        send(Update::Sound(sound))
    };

    let server = deskcheck_pulse::server_kind().map_err(|error| error.to_string());
    if !(send(Update::SoundServer(server)) && send_sound(&mut meters) && send(read_screens())) {
        return;
    }

    // One channel for the changes of both sources (DC-16).
    let (changes, from_watchers) = mpsc::channel();
    forward(from_watchers, inputs.clone(), Input::Change);
    // Kept alive as long as the thread runs; a missing source is already
    // shown by the first reads above.
    let _sound = deskcheck_pulse::SoundWatcher::start(changes.clone());
    let _screens = deskcheck_mutter::ScreenWatcher::start(changes);

    let mut pending: Vec<Change> = Vec::new();
    let mut refresh = false;
    let mut deadline: Option<Instant> = None;
    loop {
        match received.recv_timeout(METER_TICK) {
            Ok(Input::Change(change)) => {
                pending.push(change);
                deadline.get_or_insert_with(|| Instant::now() + BURST);
            }
            Ok(Input::Request(Request::Refresh)) => {
                refresh = true;
                deadline.get_or_insert_with(Instant::now);
            }
            Ok(Input::Request(request @ (Request::TestOutput(_) | Request::TestInput { .. }))) => {
                run_test(request, updates.clone());
            }
            Ok(Input::Request(request)) => {
                // The watcher sees the change and the state is read again.
                if let Some(error) = apply(&request) {
                    if !send(Update::Message(error)) {
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if !meters.send_peaks(&send) {
            return;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let changes = coalesce(std::mem::take(&mut pending));
            if (refresh || changes.iter().any(Change::is_sound)) && !send_sound(&mut meters) {
                return;
            }
            if (refresh || changes.contains(&Change::Screens)) && !send(read_screens()) {
                return;
            }
            refresh = false;
            deadline = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{Hardware, Input, Output};

    fn input(id: &str, kind: InputKind) -> Input {
        Input {
            id: DeviceId::new(id),
            description: id.to_owned(),
            hardware: Hardware::default(),
            kind,
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    #[test]
    fn the_default_microphone_and_the_output_monitor_are_measured() {
        let state = AudioState {
            outputs: vec![Output {
                id: DeviceId::new("speakers"),
                description: "Speakers".to_owned(),
                hardware: Hardware::default(),
                volume: Volume::NORMAL,
                muted: false,
            }],
            inputs: vec![
                input(
                    "speakers.monitor",
                    InputKind::Monitor {
                        of: DeviceId::new("speakers"),
                    },
                ),
                input("mic", InputKind::Capture),
            ],
            default_output: Some(DeviceId::new("speakers")),
            default_input: Some(DeviceId::new("mic")),
            ..AudioState::default()
        };
        assert_eq!(
            metered_sources(&state),
            [DeviceId::new("mic"), DeviceId::new("speakers.monitor")]
        );

        // A monitor as default input is not measured as a microphone.
        let mut monitor_default = state;
        monitor_default.default_input = Some(DeviceId::new("speakers.monitor"));
        assert_eq!(
            metered_sources(&monitor_default),
            [DeviceId::new("speakers.monitor")]
        );
        assert_eq!(metered_sources(&AudioState::default()), []);
    }

    #[test]
    fn error_messages_start_with_a_capital() {
        let error = deskcheck_pulse::Error::NoDevice("x".to_owned());
        assert!(error_text(&error).starts_with("The sound device"));
    }
}
