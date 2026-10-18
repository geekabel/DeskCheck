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
//!
//! It also reads the name of the Wi-Fi every few seconds (DC-27), and
//! applies the choices of a place, keeping what was in use so the user
//! can undo it (DC-28).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use deskcheck_core::audio::{AudioState, DeviceId, Fix, InputKind, StreamId, Target, Volume, tone};
use deskcheck_core::display::DisplayState;
use deskcheck_core::display::ScreenId;
use deskcheck_core::events::{Change, coalesce};
use deskcheck_core::places::{Place, SavedLayout, Snapshot, SoundChoice};
use deskcheck_mutter::pointer::{PointerError, PointerReader, Session};
use deskcheck_mutter::setup::{Change as ScreenChange, LayoutMode, Setup};
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
    /// What is needed to change the screens: every mode, every scale.
    Setup(Option<Setup>),
    /// The screen the mouse is on, when it changes.
    Mouse(Mouse),
    /// A new peak of a level meter, from 0.0 to 1.0.
    Peak {
        /// The source measured: a microphone, or the monitor of an output.
        source: DeviceId,
        /// The loudest sample since the last peak.
        peak: f32,
    },
    /// The name of the Wi-Fi in use, when it changes.
    Wifi(Option<String>),
    /// The choices of a place were applied.
    PlaceApplied(PlaceResult),
    /// A sentence for the user, about a request that ended.
    Message(String),
}

/// What applying a place did.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceResult {
    /// The name of the place.
    pub name: String,
    /// The default devices set.
    pub sound: SoundChoice,
    /// `true` when the screens were laid out again.
    pub layout_changed: bool,
    /// What was in use before, to undo it.
    pub undo: Snapshot,
    /// What failed, in sentences.
    pub problems: Vec<String>,
}

/// Which sources the level meters measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeterScope {
    /// What the Now page shows: the default microphone, and the monitor of
    /// the default output.
    #[default]
    Now,
    /// Every microphone and every monitor, for the Audio page.
    Everything,
}

/// What the interface asks the backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Read everything again.
    Refresh,
    /// Measure other sources.
    Meter(MeterScope),
    /// Apply a fix of the diagnosis.
    Fix(Fix),
    /// Change the screens: layout, primary, mode, scale, place.
    Screens(ScreenChange),
    /// Move an app that plays to another output.
    MovePlayback {
        /// The stream of the app.
        stream: StreamId,
        /// The output.
        to: DeviceId,
    },
    /// Move an app that records to another input.
    MoveRecording {
        /// The stream of the app.
        stream: StreamId,
        /// The input.
        to: DeviceId,
    },
    /// Make an output or an input the default one.
    SetDefault(Target),
    /// Set the volume of a device.
    SetVolume(Target, Volume),
    /// Play the test chime on an output.
    TestOutput(DeviceId),
    /// Apply the choices of a place: default output and microphone, and
    /// the layout of the screens.
    ApplyPlace(Box<Place>),
    /// Put back what was in use before a place was applied.
    Undo(Box<Snapshot>),
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

/// Reads the screens, and what is needed to change them, and keeps them
/// for the mouse. Returns `false` when the interface is gone.
fn send_screens(send: &impl Fn(Update) -> bool, mouse: &mut MouseWatch) -> bool {
    let screens = deskcheck_mutter::read_state().map_err(|error| error.to_string());
    let setup = deskcheck_mutter::read_setup().ok();
    mouse.screens = screens.as_ref().ok().cloned();
    if let Some(setup) = &setup {
        mouse.physical = setup.layout_mode == LayoutMode::Physical;
    }
    send(Update::Screens(screens)) && send(Update::Setup(setup))
}

/// Where the mouse is, for the interface (DC-26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mouse {
    /// On this screen.
    On(ScreenId),
    /// Known, but on no screen that is on.
    Elsewhere,
    /// On Wayland, the extension for GNOME Shell is needed.
    NeedsExtension,
    /// Not known, for this reason.
    Unknown(String),
}

/// How often the mouse is read.
const MOUSE_TICK: Duration = Duration::from_millis(300);

/// How often a failed way to read the mouse is tried again: the user may
/// have turned the extension on.
const MOUSE_RETRY: Duration = Duration::from_secs(10);

/// Reads the mouse now and then, and tells the interface when it moves to
/// another screen.
struct MouseWatch {
    reader: Option<PointerReader>,
    screens: Option<DisplayState>,
    physical: bool,
    last_read: Option<Instant>,
    last_try: Option<Instant>,
    sent: Option<Mouse>,
}

impl MouseWatch {
    fn new() -> Self {
        Self {
            reader: None,
            screens: None,
            physical: Session::current() == Some(Session::X11),
            last_read: None,
            last_try: None,
            sent: None,
        }
    }

    /// What the mouse is on now, or `None` when it is not time to look.
    fn look(&mut self) -> Option<Mouse> {
        let now = Instant::now();
        if self.last_read.is_some_and(|at| now - at < MOUSE_TICK) {
            return None;
        }
        self.last_read = Some(now);
        if self.reader.is_none() {
            if self.last_try.is_some_and(|at| now - at < MOUSE_RETRY) {
                return None;
            }
            self.last_try = Some(now);
            match PointerReader::new() {
                Ok(reader) => self.reader = Some(reader),
                Err(error) => return Some(Mouse::Unknown(error.to_string())),
            }
        }
        let reader = self.reader.as_ref()?;
        Some(match reader.position() {
            Ok((x, y)) => self
                .screens
                .as_ref()
                .and_then(|state| state.screen_at(x, y, self.physical))
                .map_or(Mouse::Elsewhere, |screen| Mouse::On(screen.id.clone())),
            Err(PointerError::NeedsExtension) => {
                // Asked again later, when the user may have turned it on.
                self.reader = None;
                Mouse::NeedsExtension
            }
            Err(error) => {
                self.reader = None;
                Mouse::Unknown(error.to_string())
            }
        })
    }

    /// Sends where the mouse is, when it changed. Returns `false` when the
    /// interface is gone.
    fn send(&mut self, send: &impl Fn(Update) -> bool) -> bool {
        let Some(mouse) = self.look() else {
            return true;
        };
        if self.sent.as_ref() == Some(&mouse) {
            return true;
        }
        self.sent = Some(mouse.clone());
        send(Update::Mouse(mouse))
    }
}

/// How often the name of the Wi-Fi is read.
const WIFI_TICK: Duration = Duration::from_secs(5);

/// Reads the Wi-Fi now and then, and tells the interface when it changes.
#[derive(Default)]
struct WifiWatch {
    reader: deskcheck_network::WifiReader,
    last_read: Option<Instant>,
    /// `true` once a name, or no name, was sent.
    told: bool,
    sent: Option<String>,
}

impl WifiWatch {
    /// Sends the name of the Wi-Fi, when it changed. Without
    /// `NetworkManager`, there is no Wi-Fi. Returns `false` when the
    /// interface is gone.
    fn send(&mut self, send: &impl Fn(Update) -> bool) -> bool {
        let now = Instant::now();
        if self.last_read.is_some_and(|at| now - at < WIFI_TICK) {
            return true;
        }
        self.last_read = Some(now);
        let name = self.reader.read().ok().flatten();
        if self.told && self.sent == name {
            return true;
        }
        self.told = true;
        self.sent.clone_from(&name);
        send(Update::Wifi(name))
    }
}

/// Applies the choices of a place, with the sound and the screens read
/// last. Only what differs is changed, and kept for Undo.
fn apply_place(
    place: &Place,
    sound: Option<&AudioState>,
    screens: Option<&DisplayState>,
) -> PlaceResult {
    let mut result = PlaceResult {
        name: place.name.clone(),
        sound: SoundChoice::default(),
        layout_changed: false,
        undo: Snapshot::default(),
        problems: Vec::new(),
    };
    if let Some(state) = sound {
        let choice = place.sound_choice(state);
        if let Some(output) = &choice.output {
            match deskcheck_pulse::apply(&Action::SetDefaultOutput(output.clone())) {
                Ok(()) => {
                    result.undo.output.clone_from(&state.default_output);
                    result.sound.output = Some(output.clone());
                }
                Err(error) => result.problems.push(error_text(&error)),
            }
        }
        if let Some(input) = &choice.input {
            match deskcheck_pulse::apply(&Action::SetDefaultInput(input.clone())) {
                Ok(()) => {
                    result.undo.input.clone_from(&state.default_input);
                    result.sound.input = Some(input.clone());
                }
                Err(error) => result.problems.push(error_text(&error)),
            }
        }
    }
    if let (Some(layout), Some(state)) = (&place.layout, screens) {
        // A layout whose screens are not all plugged in is left aside.
        if layout.fits(state) && !layout.is_in_use(state) {
            match deskcheck_mutter::apply_layout(layout) {
                Ok(()) => {
                    result.undo.layout = Some(SavedLayout::from_state(state));
                    result.layout_changed = true;
                }
                Err(error) => result.problems.push(error_text(&error)),
            }
        }
    }
    result
}

/// Puts back what was in use before a place. Returns what failed, if
/// anything: the Now page already says it is undone.
fn undo_place(snapshot: &Snapshot) -> Option<String> {
    let mut problems = Vec::new();
    if let Some(output) = &snapshot.output {
        if let Err(error) = deskcheck_pulse::apply(&Action::SetDefaultOutput(output.clone())) {
            problems.push(error_text(&error));
        }
    }
    if let Some(input) = &snapshot.input {
        if let Err(error) = deskcheck_pulse::apply(&Action::SetDefaultInput(input.clone())) {
            problems.push(error_text(&error));
        }
    }
    if let Some(layout) = &snapshot.layout {
        if let Err(error) = deskcheck_mutter::apply_layout(layout) {
            problems.push(error_text(&error));
        }
    }
    (!problems.is_empty()).then(|| problems.join(" "))
}

/// Changes the screens. GNOME then asks the user to keep the change.
fn change_screens(change: &ScreenChange) -> String {
    match deskcheck_mutter::apply(change) {
        Ok(()) => "Keep the change in the dialog of GNOME: without an answer, the screens go \
                   back in 20 s."
            .to_owned(),
        Err(error) => error_text(&error),
    }
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

/// The sources measured for a scope.
pub fn sources_of(scope: MeterScope, state: &AudioState) -> Vec<DeviceId> {
    let mut sources = metered_sources(state);
    if scope == MeterScope::Everything {
        for input in &state.inputs {
            if !sources.contains(&input.id) {
                sources.push(input.id.clone());
            }
        }
    }
    sources
}

/// The level meters, started again when the measured sources change.
#[derive(Default)]
struct MeterSet {
    scope: MeterScope,
    sources: Vec<DeviceId>,
    meters: Option<Meters>,
}

impl MeterSet {
    fn follow(&mut self, state: &AudioState) {
        let wanted = sources_of(self.scope, state);
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

fn error_text(error: &impl std::fmt::Display) -> String {
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
        Request::MovePlayback { stream, to } => Action::MovePlayback {
            stream: *stream,
            to: to.clone(),
        },
        Request::MoveRecording { stream, to } => Action::MoveRecording {
            stream: *stream,
            to: to.clone(),
        },
        Request::Fix(fix) => Action::from_fix(fix)?,
        Request::Refresh
        | Request::Screens(_)
        | Request::ApplyPlace(_)
        | Request::Undo(_)
        | Request::Meter(_)
        | Request::TestOutput(_)
        | Request::TestInput { .. } => return None,
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
    // The last state read, to start other meters without reading again.
    let mut last: Option<AudioState> = None;
    let send_sound = |meters: &mut MeterSet, last: &mut Option<AudioState>| {
        let sound = read_sound();
        if let Ok(state) = &sound {
            meters.follow(state);
        }
        *last = sound.as_ref().ok().cloned();
        send(Update::Sound(sound))
    };

    let mut mouse = MouseWatch::new();
    let mut wifi = WifiWatch::default();
    let server = deskcheck_pulse::server_kind().map_err(|error| error.to_string());
    if !(send(Update::SoundServer(server))
        && send_sound(&mut meters, &mut last)
        && send_screens(&send, &mut mouse)
        && wifi.send(&send))
    {
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
            Ok(Input::Request(Request::Meter(scope))) => {
                meters.scope = scope;
                if let Some(state) = &last {
                    meters.follow(state);
                }
            }
            Ok(Input::Request(request @ (Request::TestOutput(_) | Request::TestInput { .. }))) => {
                run_test(request, updates.clone());
            }
            Ok(Input::Request(Request::Screens(change))) => {
                // GNOME sends MonitorsChanged after the change: the screens
                // are read again then.
                if !send(Update::Message(change_screens(&change))) {
                    return;
                }
            }
            Ok(Input::Request(Request::ApplyPlace(place))) => {
                // The watchers see the changes and the state is read again.
                let result = apply_place(&place, last.as_ref(), mouse.screens.as_ref());
                if !send(Update::PlaceApplied(result)) {
                    return;
                }
            }
            Ok(Input::Request(Request::Undo(snapshot))) => {
                if let Some(problems) = undo_place(&snapshot) {
                    if !send(Update::Message(problems)) {
                        return;
                    }
                }
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
        if !meters.send_peaks(&send) || !mouse.send(&send) || !wifi.send(&send) {
            return;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let changes = coalesce(std::mem::take(&mut pending));
            if (refresh || changes.iter().any(Change::is_sound))
                && !send_sound(&mut meters, &mut last)
            {
                return;
            }
            if (refresh || changes.contains(&Change::Screens)) && !send_screens(&send, &mut mouse) {
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
