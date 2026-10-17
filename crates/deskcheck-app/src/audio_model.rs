//! What the Audio page shows, computed from the state. Pure, so it is
//! tested without a display.

use std::time::Instant;

use deskcheck_config::Config;
use deskcheck_core::audio::{AudioState, DeviceId, InputKind, StreamId, best_microphone};

use crate::now_model::{app_icon, device_icon};

/// The three tabs of the Audio page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTab {
    /// Where the sound can go.
    Outputs,
    /// What can hear you. Monitors are listed apart.
    Microphones,
    /// Each app, with its device.
    Apps,
}

impl AudioTab {
    /// All tabs, in the order of the design.
    pub const ALL: [Self; 3] = [Self::Outputs, Self::Microphones, Self::Apps];

    /// The name of its page in the stack.
    pub fn id(self) -> &'static str {
        match self {
            Self::Outputs => "outputs",
            Self::Microphones => "microphones",
            Self::Apps => "apps",
        }
    }

    /// The title of the tab and of its page.
    pub fn title(self) -> &'static str {
        match self {
            Self::Outputs => "Outputs",
            Self::Microphones => "Microphones",
            Self::Apps => "Apps",
        }
    }

    /// The line under the title.
    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Outputs => "The checked output is the one new apps play on.",
            Self::Microphones => "The checked microphone is the one new apps use.",
            Self::Apps => "Each app, where its sound goes and what it listens to.",
        }
    }

    /// The tip at the top of the page: a bold start, then the rest.
    pub fn tip(self) -> (&'static str, &'static str) {
        match self {
            Self::Outputs => ("Press Test.", "A short sound plays on that output only."),
            Self::Microphones => (
                "Speak out loud.",
                "The bar that moves shows the microphone that really hears you.",
            ),
            Self::Apps => (
                "Pick another device.",
                "The app moves to it at once, without stopping.",
            ),
        }
    }

    /// The icon of the tip.
    pub fn tip_icon(self) -> &'static str {
        match self {
            Self::Outputs => "dc-speaker-symbolic",
            Self::Microphones => "dc-microphone-symbolic",
            Self::Apps => "dc-switch-symbolic",
        }
    }
}

/// The text of a tab: its title and how many things it lists.
pub fn tab_label(tab: AudioTab, state: Option<&AudioState>) -> String {
    let Some(state) = state else {
        return tab.title().to_owned();
    };
    let count = match tab {
        AudioTab::Outputs => state.outputs.len(),
        AudioTab::Microphones => state.microphones().count(),
        AudioTab::Apps => stream_rows(state).len(),
    };
    format!("{} ({count})", tab.title())
}

/// One output or microphone of the lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRow {
    /// The device.
    pub id: DeviceId,
    /// Its name: the one given, or the one of the system.
    pub name: String,
    /// The name of the system and the technical name.
    pub technical: String,
    /// Its icon.
    pub icon: &'static str,
    /// `true` for the default device.
    pub default: bool,
    /// `true` when its sound is off.
    pub muted: bool,
}

fn technical(description: &str, id: &DeviceId) -> String {
    format!("{description} · {id}")
}

/// The rows of the Outputs tab, in the order of the system.
pub fn output_rows(config: &Config, state: &AudioState) -> Vec<DeviceRow> {
    state
        .outputs
        .iter()
        .map(|output| DeviceRow {
            id: output.id.clone(),
            name: config.output_label(output).to_owned(),
            technical: technical(&output.description, &output.id),
            icon: device_icon(output.hardware, false),
            default: state.default_output.as_ref() == Some(&output.id),
            muted: output.muted,
        })
        .collect()
}

/// The monitor of an output: the source that copies what it plays.
pub fn monitor_of<'a>(state: &'a AudioState, output: &DeviceId) -> Option<&'a DeviceId> {
    state.inputs.iter().find_map(|input| match &input.kind {
        InputKind::Monitor { of } if of == output => Some(&input.id),
        _ => None,
    })
}

/// The rows of the Microphones tab: microphones only, monitors apart.
pub fn microphone_rows(config: &Config, state: &AudioState) -> Vec<DeviceRow> {
    state
        .microphones()
        .map(|input| DeviceRow {
            id: input.id.clone(),
            name: config.input_label(input).into_owned(),
            technical: technical(&input.description, &input.id),
            icon: device_icon(input.hardware, true),
            default: state.default_input.as_ref() == Some(&input.id),
            muted: input.muted,
        })
        .collect()
}

/// A monitor that an app records: a problem in a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorRow {
    /// The monitor.
    pub id: DeviceId,
    /// `Monitor of …`, with the name given to the output.
    pub name: String,
    /// The name of the system and the technical name.
    pub technical: String,
    /// The icon of the output it copies.
    pub icon: &'static str,
    /// The apps that record it, by name, once each.
    pub apps: Vec<String>,
    /// Their streams, to move them.
    pub streams: Vec<StreamId>,
}

/// The monitors: those an app records (with a fix), and the names of the
/// others.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Monitors {
    /// How many monitors there are.
    pub count: usize,
    /// The monitors an app records.
    pub recorded: Vec<MonitorRow>,
    /// The names of the other monitors.
    pub others: Vec<String>,
    /// The microphone to switch to, with its name.
    pub switch_to: Option<(DeviceId, String)>,
}

/// The monitors of the Microphones tab.
pub fn monitors(config: &Config, state: &AudioState) -> Monitors {
    let mut monitors = Monitors {
        switch_to: best_microphone(state)
            .map(|mic| (mic.id.clone(), config.input_label(mic).into_owned())),
        ..Monitors::default()
    };
    for input in state.monitors() {
        monitors.count += 1;
        let name = config.input_label(input).into_owned();
        let mut apps = Vec::new();
        let mut streams = Vec::new();
        for (recording, _) in state
            .recordings_from_monitors()
            .filter(|(_, monitor)| monitor.id == input.id)
        {
            if !apps.contains(&recording.app.name) {
                apps.push(recording.app.name.clone());
            }
            streams.push(recording.id);
        }
        if apps.is_empty() {
            monitors.others.push(name);
            continue;
        }
        let icon = match &input.kind {
            InputKind::Monitor { of } => state.output(of).map_or("dc-speaker-symbolic", |output| {
                device_icon(output.hardware, false)
            }),
            InputKind::Capture => device_icon(input.hardware, false),
        };
        monitors.recorded.push(MonitorRow {
            id: input.id.clone(),
            name,
            technical: technical(&input.description, &input.id),
            icon,
            apps,
            streams,
        });
    }
    monitors
}

/// The title of the monitors card.
pub fn monitors_title(count: usize) -> String {
    format!("Output monitors ({count}): these are not microphones")
}

/// The line under the monitor rows.
pub fn monitors_note(monitors: &Monitors) -> String {
    let never = "Never offered as the default microphone.";
    match (monitors.recorded.is_empty(), monitors.others.is_empty()) {
        (_, true) => never.to_owned(),
        (true, false) => format!("{}. {never}", monitors.others.join(", ")),
        (false, false) => format!("Also: {}. {never}", monitors.others.join(", ")),
    }
}

/// The tag of a monitor an app records: `Firefox listens`.
pub fn listeners_tag(apps: &[String]) -> String {
    match apps {
        [one] => format!("{one} listens"),
        many => format!("{} listen", many.join(", ")),
    }
}

/// Whether an app plays or records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    /// The app plays sound on an output.
    Playback,
    /// The app records from an input.
    Recording,
}

/// One row of the Apps tab: a stream of an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRow {
    /// Plays or records.
    pub kind: StreamKind,
    /// The stream, to move it.
    pub stream: StreamId,
    /// The app.
    pub app: String,
    /// Its icon.
    pub icon: &'static str,
    /// The device it uses now.
    pub device: DeviceId,
    /// `true` when it records a monitor.
    pub records_monitor: bool,
}

/// The rows of the Apps tab: first the apps that record, then those that
/// play. Level meters are left out: they only measure.
pub fn stream_rows(state: &AudioState) -> Vec<StreamRow> {
    let recordings = state
        .recordings
        .iter()
        .filter(|recording| !recording.level_meter)
        .map(|recording| StreamRow {
            kind: StreamKind::Recording,
            stream: recording.id,
            app: recording.app.name.clone(),
            icon: app_icon(&recording.app),
            device: recording.input.clone(),
            records_monitor: state
                .input(&recording.input)
                .is_some_and(deskcheck_core::audio::Input::is_monitor),
        });
    let playbacks = state.playbacks.iter().map(|playback| StreamRow {
        kind: StreamKind::Playback,
        stream: playback.id,
        app: playback.app.name.clone(),
        icon: app_icon(&playback.app),
        device: playback.output.clone(),
        records_monitor: false,
    });
    recordings.chain(playbacks).collect()
}

/// The devices an app row can pick, with their names: the outputs for an
/// app that plays, the microphones for an app that records, and the
/// monitor it records now, if any, so the list shows where it is.
pub fn stream_choices(
    config: &Config,
    state: &AudioState,
    row: &StreamRow,
) -> Vec<(DeviceId, String)> {
    match row.kind {
        StreamKind::Playback => state
            .outputs
            .iter()
            .map(|output| (output.id.clone(), config.output_label(output).to_owned()))
            .collect(),
        StreamKind::Recording => state
            .inputs
            .iter()
            .filter(|input| !input.is_monitor() || input.id == row.device)
            .map(|input| (input.id.clone(), config.input_label(input).into_owned()))
            .collect(),
    }
}

/// What a microphone hears, from its recent level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hearing {
    /// A loud sound: it hears your voice.
    Voice,
    /// A faint sound: it is far from you.
    Faint,
    /// Nothing at all.
    Silent,
}

impl Hearing {
    /// The text of its tag.
    pub fn label(self) -> &'static str {
        match self {
            Self::Voice => "Hears your voice",
            Self::Faint => "Faint, far away",
            Self::Silent => "Silent",
        }
    }
}

/// What an output plays, from the level of its monitor.
pub fn playing_label(hearing: Hearing) -> &'static str {
    match hearing {
        Hearing::Voice | Hearing::Faint => "Playing",
        Hearing::Silent => "Quiet",
    }
}

/// Above this level, a microphone hears your voice.
pub const VOICE_DB: f32 = -35.0;
/// Above this level, it hears something.
const FAINT_DB: f32 = -58.0;
/// How fast the remembered level falls, in dB per second, so a tag does
/// not change between two words.
const FALL_DB_PER_SECOND: f32 = 12.0;

/// The loudest recent level of a source, falling slowly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hold {
    db: f32,
    at: Option<Instant>,
}

impl Default for Hold {
    fn default() -> Self {
        Self {
            db: deskcheck_core::audio::level::FLOOR_DB,
            at: None,
        }
    }
}

impl Hold {
    /// The remembered level at `now`.
    pub fn db(&self, now: Instant) -> f32 {
        let Some(at) = self.at else {
            return self.db;
        };
        let fallen = now.saturating_duration_since(at).as_secs_f32() * FALL_DB_PER_SECOND;
        (self.db - fallen).max(deskcheck_core::audio::level::FLOOR_DB)
    }

    /// Takes a new level, in dB, heard at `now`.
    pub fn update(&mut self, db: f32, now: Instant) {
        if db >= self.db(now) {
            self.db = db;
            self.at = Some(now);
        }
    }

    /// What the source hears at `now`.
    pub fn hearing(&self, now: Instant) -> Hearing {
        let db = self.db(now);
        if db > VOICE_DB {
            Hearing::Voice
        } else if db > FAINT_DB {
            Hearing::Faint
        } else {
            Hearing::Silent
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use deskcheck_core::audio::{
        App, Connection, Form, Hardware, Input, Output, Playback, Recording, Volume,
    };

    fn app(name: &str) -> App {
        App {
            name: name.to_owned(),
            ..App::default()
        }
    }

    fn input(id: &str, description: &str, hardware: Hardware, kind: InputKind) -> Input {
        Input {
            id: DeviceId::new(id),
            description: description.to_owned(),
            hardware,
            kind,
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    fn output(id: &str, description: &str, hardware: Hardware) -> Output {
        Output {
            id: DeviceId::new(id),
            description: description.to_owned(),
            hardware,
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    /// The office of the design: a USB headset, the laptop, a webcam, the
    /// Dell screen, and Firefox recording the monitor of the Dell screen.
    fn office() -> AudioState {
        let headset = Hardware {
            form: Form::Headset,
            connection: Connection::Usb,
        };
        let laptop = Hardware {
            form: Form::Speakers,
            connection: Connection::BuiltIn,
        };
        let webcam = Hardware {
            form: Form::Webcam,
            connection: Connection::Usb,
        };
        let dell = Hardware {
            form: Form::Screen,
            connection: Connection::Hdmi,
        };
        let monitor = |of: &str| InputKind::Monitor {
            of: DeviceId::new(of),
        };
        AudioState {
            outputs: vec![
                output("headset", "USB Headset", headset),
                output("laptop", "Built-in Audio", laptop),
                output("dell", "HDMI / DisplayPort 2", dell),
            ],
            inputs: vec![
                input("headset.mic", "USB Headset", headset, InputKind::Capture),
                input("laptop.mic", "Built-in Audio", laptop, InputKind::Capture),
                input("webcam.mic", "USB Webcam", webcam, InputKind::Capture),
                input(
                    "headset.monitor",
                    "Monitor of USB Headset",
                    headset,
                    monitor("headset"),
                ),
                input(
                    "laptop.monitor",
                    "Monitor of Built-in Audio",
                    laptop,
                    monitor("laptop"),
                ),
                input(
                    "dell.monitor",
                    "Monitor of HDMI / DisplayPort 2",
                    dell,
                    monitor("dell"),
                ),
            ],
            playbacks: vec![Playback {
                id: StreamId::new(7),
                app: app("Firefox"),
                media: None,
                output: DeviceId::new("headset"),
                volume: Volume::NORMAL,
                muted: false,
            }],
            recordings: vec![
                Recording {
                    id: StreamId::new(8),
                    app: app("Firefox"),
                    media: None,
                    input: DeviceId::new("dell.monitor"),
                    volume: Volume::NORMAL,
                    muted: false,
                    level_meter: false,
                },
                Recording {
                    id: StreamId::new(9),
                    app: app("GNOME Settings"),
                    media: None,
                    input: DeviceId::new("headset.mic"),
                    volume: Volume::NORMAL,
                    muted: false,
                    level_meter: true,
                },
            ],
            default_output: Some(DeviceId::new("headset")),
            default_input: Some(DeviceId::new("headset.mic")),
        }
    }

    #[test]
    fn the_tabs_count_what_they_list() {
        let state = office();
        let labels: Vec<String> = AudioTab::ALL
            .iter()
            .map(|tab| tab_label(*tab, Some(&state)))
            .collect();
        // Monitors are not microphones; the level meter is not an app.
        assert_eq!(labels, ["Outputs (3)", "Microphones (3)", "Apps (2)"]);
        assert_eq!(tab_label(AudioTab::Apps, None), "Apps");
    }

    #[test]
    fn microphones_are_listed_with_their_icon_and_default() {
        let mut config = Config::default();
        config.set_audio_name(
            &DeviceId::new("headset.mic"),
            Some("Headset mic".to_owned()),
        );
        let rows = microphone_rows(&config, &office());
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["Headset mic", "Built-in Audio", "USB Webcam"]);
        assert!(rows[0].default && !rows[1].default);
        assert_eq!(rows[0].technical, "USB Headset · headset.mic");
        let icons: Vec<&str> = rows.iter().map(|row| row.icon).collect();
        assert_eq!(
            icons,
            [
                "dc-headphones-symbolic",
                "dc-laptop-symbolic",
                "dc-webcam-symbolic"
            ]
        );
    }

    #[test]
    fn outputs_are_listed_in_order() {
        let state = office();
        let rows = output_rows(&Config::default(), &state);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].default);
        assert_eq!(rows[2].icon, "dc-screen-symbolic");
        assert_eq!(
            monitor_of(&state, &DeviceId::new("dell")),
            Some(&DeviceId::new("dell.monitor"))
        );
        assert_eq!(monitor_of(&state, &DeviceId::new("nothing")), None);
    }

    #[test]
    fn a_recorded_monitor_comes_with_a_switch() {
        let mut config = Config::default();
        config.set_audio_name(&DeviceId::new("dell"), Some("Dell screen".to_owned()));
        config.set_audio_name(
            &DeviceId::new("headset.mic"),
            Some("Headset mic".to_owned()),
        );
        let monitors = monitors(&config, &office());
        assert_eq!(monitors.count, 3);
        assert_eq!(monitors.recorded.len(), 1);
        let dell = &monitors.recorded[0];
        assert_eq!(dell.name, "Monitor of Dell screen");
        assert_eq!(dell.icon, "dc-screen-symbolic");
        assert_eq!(dell.apps, ["Firefox"]);
        assert_eq!(dell.streams, [StreamId::new(8)]);
        assert_eq!(
            monitors.switch_to,
            Some((DeviceId::new("headset.mic"), "Headset mic".to_owned()))
        );
        assert_eq!(
            monitors_note(&monitors),
            "Also: Monitor of USB Headset, Monitor of Built-in Audio. \
             Never offered as the default microphone."
        );
        assert_eq!(listeners_tag(&dell.apps), "Firefox listens");
        assert_eq!(
            listeners_tag(&["Firefox".to_owned(), "Zoom".to_owned()]),
            "Firefox, Zoom listen"
        );
        assert_eq!(
            monitors_title(3),
            "Output monitors (3): these are not microphones"
        );
    }

    #[test]
    fn without_recorded_monitors_the_note_lists_them_all() {
        let mut state = office();
        state.recordings.clear();
        let monitors = monitors(&Config::default(), &state);
        assert_eq!(monitors.recorded, []);
        assert!(monitors_note(&monitors).starts_with("Monitor of USB Headset, "));
    }

    #[test]
    fn apps_rows_skip_level_meters_and_choose_among_the_right_devices() {
        let state = office();
        let rows = stream_rows(&state);
        assert_eq!(rows.len(), 2);
        let (recording, playback) = (&rows[0], &rows[1]);
        assert_eq!(recording.kind, StreamKind::Recording);
        assert!(recording.records_monitor);
        assert_eq!(recording.icon, "dc-browser-symbolic");
        assert_eq!(playback.kind, StreamKind::Playback);
        // The microphones, and the monitor it records now.
        let choices: Vec<String> = stream_choices(&Config::default(), &state, recording)
            .into_iter()
            .map(|(id, _)| id.to_string())
            .collect();
        assert_eq!(
            choices,
            ["headset.mic", "laptop.mic", "webcam.mic", "dell.monitor"]
        );
        assert_eq!(
            stream_choices(&Config::default(), &state, playback).len(),
            3
        );
    }

    #[test]
    fn the_hold_keeps_a_loud_sound_for_a_moment() {
        let start = Instant::now();
        let mut hold = Hold::default();
        assert_eq!(hold.hearing(start), Hearing::Silent);
        hold.update(-20.0, start);
        assert_eq!(hold.hearing(start), Hearing::Voice);
        // A pause between two words: still the voice.
        hold.update(-60.0, start + Duration::from_millis(500));
        assert_eq!(
            hold.hearing(start + Duration::from_millis(500)),
            Hearing::Voice
        );
        // Two seconds later, it fell to faint; later still, to silent.
        assert_eq!(hold.hearing(start + Duration::from_secs(2)), Hearing::Faint);
        assert_eq!(
            hold.hearing(start + Duration::from_secs(4)),
            Hearing::Silent
        );
        assert_eq!(playing_label(Hearing::Faint), "Playing");
        assert_eq!(playing_label(Hearing::Silent), "Quiet");
    }
}
