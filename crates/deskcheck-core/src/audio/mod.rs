//! Sound devices and the application streams that use them.
//!
//! The words follow the sound server (PulseAudio, or PipeWire through
//! `pipewire-pulse`):
//!
//! - an [`Output`] plays sound (a sink: speakers, headset, HDMI screen);
//! - an [`Input`] gives sound (a source). It is either a real capture device,
//!   like a microphone, or a *monitor*: a copy of what an output plays.
//!   A monitor is never a microphone, even if apps list it as one;
//! - a [`Playback`] is an application stream going to an output;
//! - a [`Recording`] is an application stream coming from an input.
//!
//! Each device also has a [`Hardware`]: what it is (headset, webcam,
//! screen...) and how it is connected, found by [`classify`].
//!
//! [`diagnose`] finds the common problems, like an app that records a
//! monitor, and gives a [`Fix`] for each.
//!
//! These types only hold data. The `deskcheck-pulse` crate reads them from
//! the sound server.

use std::fmt;

mod diagnose;
mod hardware;
pub mod level;
pub mod tone;

pub use diagnose::{Finding, Fix, Problem, Target, best_microphone, diagnose};
pub use hardware::{Clues, Connection, Direction, Form, Hardware, classify};

/// Stable identifier of a sound device: the device name given by the sound
/// server, for example `alsa_output.usb-Headset-00.analog-stereo`.
///
/// The name stays the same when the device is unplugged and plugged again,
/// so it is the key used to remember a device. The numeric index of the
/// sound server is not stable and is not used here.
///
/// ```
/// use deskcheck_core::audio::DeviceId;
///
/// let id = DeviceId::new("alsa_input.usb-Headset-00.mono-fallback");
/// assert_eq!(id.as_str(), "alsa_input.usb-Headset-00.mono-fallback");
/// assert_eq!(id.to_string(), "alsa_input.usb-Headset-00.mono-fallback");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    /// Creates an identifier from the device name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the device name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for DeviceId {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for DeviceId {
    fn from(name: String) -> Self {
        Self(name)
    }
}

/// A volume, in percent of the normal level.
///
/// `100` is the normal level. Values above 100 mean the sound is amplified,
/// as GNOME allows with "over-amplification".
///
/// ```
/// use deskcheck_core::audio::Volume;
///
/// let volume = Volume::from_percent(62);
/// assert_eq!(volume.percent(), 62);
/// assert_eq!(volume.to_string(), "62%");
/// assert!(Volume::from_percent(0).is_silent());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Volume(u16);

impl Volume {
    /// The normal level, 100 %.
    pub const NORMAL: Self = Self(100);

    /// Creates a volume from a percentage of the normal level.
    #[must_use]
    pub const fn from_percent(percent: u16) -> Self {
        Self(percent)
    }

    /// Returns the volume as a percentage of the normal level.
    #[must_use]
    pub const fn percent(self) -> u16 {
        self.0
    }

    /// Returns `true` when the volume is 0, so nothing can be heard.
    #[must_use]
    pub const fn is_silent(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Volume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

/// A device that plays sound (a sink of the sound server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Stable identifier.
    pub id: DeviceId,
    /// Label given by the sound server, for example `USB Headset Analog Stereo`.
    pub description: String,
    /// What the device is and how it is connected.
    pub hardware: Hardware,
    /// Current volume.
    pub volume: Volume,
    /// `true` when the output is muted.
    pub muted: bool,
}

/// Where an [`Input`] takes its sound from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputKind {
    /// A real capture device: a microphone, a headset microphone, a line in.
    Capture,
    /// A copy of what an output plays (a "monitor" source).
    ///
    /// In a call, an app that listens to a monitor sends the sound of your
    /// own speakers, not your voice.
    Monitor {
        /// The output this monitor copies.
        of: DeviceId,
    },
}

/// A device that gives sound (a source of the sound server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// Stable identifier.
    pub id: DeviceId,
    /// Label given by the sound server, for example `Monitor of HDMI / DisplayPort 2`.
    pub description: String,
    /// What the device is and how it is connected. For a monitor, this is
    /// the hardware of the output it copies.
    pub hardware: Hardware,
    /// Real capture device or monitor.
    pub kind: InputKind,
    /// Current volume (sensitivity).
    pub volume: Volume,
    /// `true` when the input is muted.
    pub muted: bool,
}

impl Input {
    /// Returns `true` when this input is a monitor, not a real capture device.
    #[must_use]
    pub fn is_monitor(&self) -> bool {
        matches!(self.kind, InputKind::Monitor { .. })
    }
}

/// Identifier of an application stream, given by the sound server.
///
/// It only lives as long as the stream: a new call gets a new stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamId(u32);

impl StreamId {
    /// Creates an identifier from the index of the sound server.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the index of the sound server.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// The application behind a stream, from the stream properties.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct App {
    /// Human readable name (`application.name`), for example `Firefox`.
    pub name: String,
    /// Process name (`application.process.binary`), for example `firefox`.
    pub binary: Option<String>,
    /// XDG icon name (`application.icon_name`).
    pub icon_name: Option<String>,
}

/// An application stream that plays sound on an output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playback {
    /// Stream identifier.
    pub id: StreamId,
    /// The application that plays.
    pub app: App,
    /// What is played (`media.name`), for example the title of a tab.
    pub media: Option<String>,
    /// The output that receives the sound.
    pub output: DeviceId,
    /// Volume of this stream.
    pub volume: Volume,
    /// `true` when this stream is muted.
    pub muted: bool,
}

/// An application stream that records sound from an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    /// Stream identifier.
    pub id: StreamId,
    /// The application that records, for example the browser in a call.
    pub app: App,
    /// What is recorded (`media.name`), when the app gives it.
    pub media: Option<String>,
    /// The input the sound comes from.
    pub input: DeviceId,
    /// Volume of this stream.
    pub volume: Volume,
    /// `true` when this stream is muted.
    pub muted: bool,
    /// `true` for a level meter, like the level bar of GNOME Settings: it
    /// only measures the sound, it is not a call or a recording. A level
    /// meter on a monitor is normal and is not a problem.
    pub level_meter: bool,
}

/// Everything known about sound at one moment.
///
/// ```
/// use deskcheck_core::audio::{AudioState, DeviceId, Hardware, Input, InputKind, Volume};
///
/// let state = AudioState {
///     inputs: vec![
///         Input {
///             id: DeviceId::new("alsa_input.usb-Headset-00.mono-fallback"),
///             description: "USB Headset Mono".into(),
///             hardware: Hardware::default(),
///             kind: InputKind::Capture,
///             volume: Volume::NORMAL,
///             muted: false,
///         },
///         Input {
///             id: DeviceId::new("alsa_output.pci-0000_00_1f.3.hdmi-stereo.monitor"),
///             description: "Monitor of HDMI".into(),
///             hardware: Hardware::default(),
///             kind: InputKind::Monitor {
///                 of: DeviceId::new("alsa_output.pci-0000_00_1f.3.hdmi-stereo"),
///             },
///             volume: Volume::NORMAL,
///             muted: false,
///         },
///     ],
///     ..AudioState::default()
/// };
///
/// assert_eq!(state.microphones().count(), 1);
/// assert_eq!(state.monitors().count(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioState {
    /// All outputs.
    pub outputs: Vec<Output>,
    /// All inputs, real capture devices and monitors.
    pub inputs: Vec<Input>,
    /// The output used by new playback streams.
    pub default_output: Option<DeviceId>,
    /// The input used by new recording streams.
    pub default_input: Option<DeviceId>,
    /// Streams that play sound.
    pub playbacks: Vec<Playback>,
    /// Streams that record sound.
    pub recordings: Vec<Recording>,
}

impl AudioState {
    /// Finds an output by its identifier.
    #[must_use]
    pub fn output(&self, id: &DeviceId) -> Option<&Output> {
        self.outputs.iter().find(|output| &output.id == id)
    }

    /// Finds an input by its identifier.
    #[must_use]
    pub fn input(&self, id: &DeviceId) -> Option<&Input> {
        self.inputs.iter().find(|input| &input.id == id)
    }

    /// Returns the default output, if it is known and present.
    #[must_use]
    pub fn default_output_device(&self) -> Option<&Output> {
        self.default_output.as_ref().and_then(|id| self.output(id))
    }

    /// Returns the default input, if it is known and present.
    #[must_use]
    pub fn default_input_device(&self) -> Option<&Input> {
        self.default_input.as_ref().and_then(|id| self.input(id))
    }

    /// Real capture devices only, without the monitors.
    pub fn microphones(&self) -> impl Iterator<Item = &Input> {
        self.inputs.iter().filter(|input| !input.is_monitor())
    }

    /// Monitors only.
    pub fn monitors(&self) -> impl Iterator<Item = &Input> {
        self.inputs.iter().filter(|input| input.is_monitor())
    }

    /// Recording streams that take their sound from the given input.
    pub fn recordings_from<'a>(&'a self, id: &'a DeviceId) -> impl Iterator<Item = &'a Recording> {
        self.recordings
            .iter()
            .filter(move |recording| &recording.input == id)
    }

    /// Playback streams that send their sound to the given output.
    pub fn playbacks_on<'a>(&'a self, id: &'a DeviceId) -> impl Iterator<Item = &'a Playback> {
        self.playbacks
            .iter()
            .filter(move |playback| &playback.output == id)
    }

    /// Recording streams that take their sound from a monitor, with that
    /// monitor. In a call, such an app sends a copy of an output, not the
    /// voice of the user. Level meters are left out: measuring a monitor is
    /// what they are for.
    pub fn recordings_from_monitors(&self) -> impl Iterator<Item = (&Recording, &Input)> {
        self.recordings
            .iter()
            .filter(|recording| !recording.level_meter)
            .filter_map(|recording| {
                self.input(&recording.input)
                    .filter(|input| input.is_monitor())
                    .map(|input| (recording, input))
            })
    }

    /// Returns the default input when it is a monitor, not a microphone.
    ///
    /// The sound server picks a monitor as the default input when it finds
    /// no microphone. New recordings then hear the speakers, not the user.
    #[must_use]
    pub fn default_input_monitor(&self) -> Option<&Input> {
        self.default_input_device()
            .filter(|input| input.is_monitor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADSET_OUT: &str = "alsa_output.usb-Headset-00.analog-stereo";
    const HEADSET_MIC: &str = "alsa_input.usb-Headset-00.mono-fallback";
    const HDMI_OUT: &str = "alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1";
    const HDMI_MONITOR: &str = "alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1.monitor";

    fn output(name: &str) -> Output {
        Output {
            id: DeviceId::new(name),
            description: name.to_owned(),
            hardware: Hardware::default(),
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    fn input(name: &str, kind: InputKind) -> Input {
        Input {
            id: DeviceId::new(name),
            description: name.to_owned(),
            hardware: Hardware::default(),
            kind,
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    fn firefox() -> App {
        App {
            name: "Firefox".to_owned(),
            binary: Some("firefox".to_owned()),
            icon_name: None,
        }
    }

    /// The office case: Firefox records from the monitor of the HDMI screen.
    fn office() -> AudioState {
        AudioState {
            outputs: vec![output(HEADSET_OUT), output(HDMI_OUT)],
            inputs: vec![
                input(HEADSET_MIC, InputKind::Capture),
                input(
                    HDMI_MONITOR,
                    InputKind::Monitor {
                        of: DeviceId::new(HDMI_OUT),
                    },
                ),
            ],
            default_output: Some(DeviceId::new(HEADSET_OUT)),
            default_input: Some(DeviceId::new(HEADSET_MIC)),
            playbacks: vec![Playback {
                id: StreamId::new(12),
                app: firefox(),
                media: Some("Teams".to_owned()),
                output: DeviceId::new(HEADSET_OUT),
                volume: Volume::NORMAL,
                muted: false,
            }],
            recordings: vec![Recording {
                id: StreamId::new(42),
                app: firefox(),
                media: None,
                input: DeviceId::new(HDMI_MONITOR),
                volume: Volume::NORMAL,
                muted: false,
                level_meter: false,
            }],
        }
    }

    #[test]
    fn device_id_converts_from_str_and_string() {
        assert_eq!(DeviceId::from(HEADSET_MIC), DeviceId::new(HEADSET_MIC));
        assert_eq!(
            DeviceId::from(HEADSET_MIC.to_owned()),
            DeviceId::new(HEADSET_MIC)
        );
    }

    #[test]
    fn volume_basics() {
        assert_eq!(Volume::default().percent(), 0);
        assert!(Volume::default().is_silent());
        assert!(!Volume::NORMAL.is_silent());
        assert!(Volume::from_percent(150) > Volume::NORMAL);
        assert_eq!(Volume::from_percent(100).to_string(), "100%");
    }

    #[test]
    fn stream_id_display() {
        assert_eq!(StreamId::new(42).to_string(), "#42");
        assert_eq!(StreamId::new(42).index(), 42);
    }

    #[test]
    fn monitor_is_not_a_microphone() {
        let state = office();
        let mics: Vec<&str> = state.microphones().map(|i| i.id.as_str()).collect();
        let monitors: Vec<&str> = state.monitors().map(|i| i.id.as_str()).collect();
        assert_eq!(mics, [HEADSET_MIC]);
        assert_eq!(monitors, [HDMI_MONITOR]);
        assert!(
            state
                .input(&DeviceId::new(HDMI_MONITOR))
                .unwrap()
                .is_monitor()
        );
    }

    #[test]
    fn monitor_knows_its_output() {
        let state = office();
        let monitor = state.input(&DeviceId::new(HDMI_MONITOR)).unwrap();
        let InputKind::Monitor { of } = &monitor.kind else {
            panic!("expected a monitor, got {:?}", monitor.kind);
        };
        assert_eq!(state.output(of).unwrap().id.as_str(), HDMI_OUT);
    }

    #[test]
    fn default_devices_are_found() {
        let state = office();
        assert_eq!(
            state.default_output_device().unwrap().id.as_str(),
            HEADSET_OUT
        );
        assert_eq!(
            state.default_input_device().unwrap().id.as_str(),
            HEADSET_MIC
        );
    }

    #[test]
    fn missing_default_device_gives_none() {
        let state = AudioState {
            default_input: Some(DeviceId::new("unplugged")),
            ..office()
        };
        assert!(state.default_input_device().is_none());
        assert!(AudioState::default().default_output_device().is_none());
    }

    #[test]
    fn recordings_from_monitors_are_found() {
        let state = office();
        let found: Vec<(StreamId, &str)> = state
            .recordings_from_monitors()
            .map(|(recording, input)| (recording.id, input.id.as_str()))
            .collect();
        assert_eq!(found, [(StreamId::new(42), HDMI_MONITOR)]);

        let mut fixed = office();
        fixed.recordings[0].input = DeviceId::new(HEADSET_MIC);
        assert_eq!(fixed.recordings_from_monitors().count(), 0);
    }

    #[test]
    fn default_input_monitor_is_reported() {
        assert!(office().default_input_monitor().is_none());
        let state = AudioState {
            default_input: Some(DeviceId::new(HDMI_MONITOR)),
            ..office()
        };
        assert_eq!(
            state.default_input_monitor().unwrap().id.as_str(),
            HDMI_MONITOR
        );
    }

    #[test]
    fn streams_are_found_by_device() {
        let state = office();
        let from_monitor: Vec<StreamId> = state
            .recordings_from(&DeviceId::new(HDMI_MONITOR))
            .map(|r| r.id)
            .collect();
        assert_eq!(from_monitor, [StreamId::new(42)]);
        assert_eq!(
            state.recordings_from(&DeviceId::new(HEADSET_MIC)).count(),
            0
        );
        assert_eq!(state.playbacks_on(&DeviceId::new(HEADSET_OUT)).count(), 1);
        assert_eq!(state.playbacks_on(&DeviceId::new(HDMI_OUT)).count(), 0);
    }
}
