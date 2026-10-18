//! What the icon of the top bar and the quick panel show (DC-30). Pure,
//! so it is tested without a display or a top bar.

use deskcheck_config::Config;
use deskcheck_core::audio::{AudioState, DeviceId};
use deskcheck_core::display::{DisplayState, ScreenId};

use crate::now_model;

/// A device of a menu: its id and the name to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayDevice {
    /// The device.
    pub id: DeviceId,
    /// Its name, the one the user gave or the one of the sound server.
    pub name: String,
}

/// What the menu of the icon shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayState {
    /// The place recognized.
    pub place: Option<String>,
    /// `true` when no app listens to a monitor instead of a microphone.
    pub ready: bool,
    /// Every output.
    pub outputs: Vec<TrayDevice>,
    /// The position of the default output in `outputs`.
    pub output: Option<usize>,
    /// Every microphone (never a monitor).
    pub inputs: Vec<TrayDevice>,
    /// The position of the default microphone in `inputs`.
    pub input: Option<usize>,
    /// Whether the default microphone is muted, when there is one.
    pub mic_muted: Option<bool>,
}

impl TrayState {
    /// The menu for the sound read last and the place recognized.
    pub fn new(config: &Config, sound: Option<&AudioState>, place: Option<&str>) -> Self {
        let Some(state) = sound else {
            return Self {
                place: place.map(str::to_owned),
                ready: true,
                ..Self::default()
            };
        };
        let outputs: Vec<TrayDevice> = state
            .outputs
            .iter()
            .map(|output| TrayDevice {
                id: output.id.clone(),
                name: config.output_label(output).to_owned(),
            })
            .collect();
        let inputs: Vec<TrayDevice> = state
            .inputs
            .iter()
            .filter(|input| !input.is_monitor())
            .map(|input| TrayDevice {
                id: input.id.clone(),
                name: config.input_label(input).into_owned(),
            })
            .collect();
        let output = state
            .default_output
            .as_ref()
            .and_then(|id| outputs.iter().position(|o| &o.id == id));
        let input = state
            .default_input
            .as_ref()
            .and_then(|id| inputs.iter().position(|i| &i.id == id));
        let mic_muted = state
            .default_input_device()
            .filter(|input| !input.is_monitor())
            .map(|input| input.muted);
        Self {
            place: place.map(str::to_owned),
            ready: now_model::alert(config, state, &[], true).is_none(),
            outputs,
            output,
            inputs,
            input,
            mic_muted,
        }
    }

    /// The default output, by name.
    pub fn output_name(&self) -> Option<&str> {
        self.output
            .and_then(|at| self.outputs.get(at))
            .map(|device| device.name.as_str())
    }

    /// The default microphone, by name.
    pub fn input_name(&self) -> Option<&str> {
        self.input
            .and_then(|at| self.inputs.get(at))
            .map(|device| device.name.as_str())
    }

    /// The first line of the menu: the place and if all is ready.
    pub fn status(&self) -> String {
        format!(
            "{} · {}",
            self.place.as_deref().unwrap_or("No place recognized"),
            ready_text(self.ready)
        )
    }

    /// The submenu of the outputs.
    pub fn output_label(&self) -> String {
        format!("Sound: {}", self.output_name().unwrap_or("none"))
    }

    /// The submenu of the microphones.
    pub fn input_label(&self) -> String {
        format!("Microphone: {}", self.input_name().unwrap_or("none"))
    }

    /// The text shown when the pointer stays on the icon.
    pub fn tooltip(&self) -> String {
        format!("{}\n{}", self.output_label(), self.input_label())
    }
}

/// The tag of the quick panel, and the end of the first line of the menu.
pub fn ready_text(ready: bool) -> &'static str {
    if ready {
        "Ready for a call"
    } else {
        "Check the microphone"
    }
}

/// The text of the button that mutes the microphone, or turns it back on.
pub fn mute_text(muted: bool) -> &'static str {
    if muted {
        "Unmute the microphone"
    } else {
        "Mute the microphone"
    }
}

/// An app that records, for the "Who is listening?" part of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    /// "Firefox · Teams".
    pub name: String,
    /// Its icon.
    pub icon: &'static str,
    /// What it listens to.
    pub source: String,
    /// `false` when it listens to a monitor, not a microphone.
    pub ok: bool,
}

/// The apps that record, level meters left out.
pub fn listeners(config: &Config, state: &AudioState) -> Vec<Listener> {
    state
        .recordings
        .iter()
        .filter(|recording| !recording.level_meter)
        .map(|recording| {
            let (source, ok) = state.input(&recording.input).map_or_else(
                || (recording.input.to_string(), true),
                |input| (config.input_label(input).into_owned(), !input.is_monitor()),
            );
            let name = match &recording.media {
                Some(media) if !media.is_empty() && media != &recording.app.name => {
                    format!("{} · {media}", recording.app.name)
                }
                _ => recording.app.name.clone(),
            };
            Listener {
                name,
                icon: now_model::app_icon(&recording.app),
                source,
                ok,
            }
        })
        .collect()
}

/// A small numbered square of the panel, in the color of its screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenBadge {
    /// Its number.
    pub number: usize,
    /// Its color, as `#rrggbb`.
    pub color: String,
    /// `true` when the mouse is on it.
    pub mouse: bool,
}

/// The screens that are on, and the line beside them: where the mouse is.
pub fn screens_part(
    config: &Config,
    state: &DisplayState,
    mouse: Option<&ScreenId>,
) -> (Vec<ScreenBadge>, String) {
    let numbered = state.numbered();
    let badges = numbered
        .iter()
        .map(|(number, screen)| ScreenBadge {
            number: *number,
            color: now_model::screen_color(config, &screen.id, *number).to_string(),
            mouse: mouse == Some(&screen.id),
        })
        .collect();
    let line = numbered
        .iter()
        .find(|(_, screen)| mouse == Some(&screen.id))
        .map_or_else(
            || match numbered.len() {
                0 => "No screen is on".to_owned(),
                1 => "One screen".to_owned(),
                count => format!("{count} screens"),
            },
            |(number, screen)| format!("Mouse on screen {number}, {}", config.screen_label(screen)),
        );
    (badges, line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{
        App, Hardware, Input, InputKind, Output, Recording, StreamId, Volume,
    };

    fn sound() -> AudioState {
        let output = |id: &str, name: &str| Output {
            id: DeviceId::new(id),
            description: name.to_owned(),
            hardware: Hardware::default(),
            volume: Volume::NORMAL,
            muted: false,
        };
        let input = |id: &str, name: &str, kind: InputKind| Input {
            id: DeviceId::new(id),
            description: name.to_owned(),
            hardware: Hardware::default(),
            kind,
            volume: Volume::NORMAL,
            muted: true,
        };
        AudioState {
            outputs: vec![output("usb", "USB headset"), output("hdmi", "Dell")],
            inputs: vec![
                input(
                    "usb.monitor",
                    "Monitor of USB headset",
                    InputKind::Monitor {
                        of: DeviceId::new("usb"),
                    },
                ),
                input("mic", "Headset microphone", InputKind::Capture),
            ],
            default_output: Some(DeviceId::new("usb")),
            default_input: Some(DeviceId::new("mic")),
            recordings: vec![Recording {
                id: StreamId::new(7),
                app: App {
                    name: "Firefox".to_owned(),
                    binary: Some("firefox".to_owned()),
                    icon_name: None,
                },
                media: Some("Teams".to_owned()),
                input: DeviceId::new("mic"),
                volume: Volume::NORMAL,
                muted: false,
                level_meter: false,
            }],
            ..AudioState::default()
        }
    }

    #[test]
    fn the_menu_shows_the_devices_in_use() {
        let state = TrayState::new(&Config::default(), Some(&sound()), Some("Office"));
        assert_eq!(state.status(), "Office · Ready for a call");
        assert_eq!(state.output_label(), "Sound: USB headset");
        assert_eq!(state.input_label(), "Microphone: Headset microphone");
        // A monitor is never offered as a microphone.
        assert_eq!(state.inputs.len(), 1);
        assert_eq!(state.mic_muted, Some(true));
        assert_eq!(mute_text(true), "Unmute the microphone");
        assert_eq!(
            state.tooltip(),
            "Sound: USB headset\nMicrophone: Headset microphone"
        );
        let empty = TrayState::new(&Config::default(), None, None);
        assert_eq!(empty.status(), "No place recognized · Ready for a call");
        assert_eq!(empty.output_label(), "Sound: none");
    }

    #[test]
    fn listeners_say_what_they_hear() {
        let mut state = sound();
        let rows = listeners(&Config::default(), &state);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Firefox · Teams");
        assert_eq!(rows[0].source, "Headset microphone");
        assert!(rows[0].ok);
        state.recordings[0].input = DeviceId::new("usb.monitor");
        let rows = listeners(&Config::default(), &state);
        assert!(!rows[0].ok);
        let tray = TrayState::new(&Config::default(), Some(&state), None);
        assert!(!tray.ready);
        assert!(tray.status().ends_with("Check the microphone"));
    }
}
