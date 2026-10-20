//! What the first-run assistant shows (DC-32), as in the design
//! "Nommage": four steps to name the outputs, the microphones (by
//! speaking into them), the screens, and the place where you are. Pure,
//! so it is tested without a display.

use deskcheck_config::{Config, DeviceIcon};
use deskcheck_core::audio::{AudioState, DeviceId, Form, Hardware};
use deskcheck_core::display::DisplayState;

/// A step of the assistant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Name each output, by listening to it.
    Outputs,
    /// Name each microphone, by speaking into it.
    Microphones,
    /// Name each screen, with its number shown on it.
    Screens,
    /// Keep the place where you are.
    Places,
}

impl Step {
    /// The steps, in order.
    pub const ALL: [Self; 4] = [
        Self::Outputs,
        Self::Microphones,
        Self::Screens,
        Self::Places,
    ];

    /// Its position, from 0.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|step| *step == self).unwrap_or(0)
    }

    /// The step after, if any.
    pub fn next(self) -> Option<Self> {
        Self::ALL.get(self.index() + 1).copied()
    }

    /// The step before, if any.
    pub fn previous(self) -> Option<Self> {
        self.index()
            .checked_sub(1)
            .and_then(|at| Self::ALL.get(at).copied())
    }

    /// Its name in the list of steps.
    pub fn title(self) -> &'static str {
        match self {
            Self::Outputs => "Sound outputs",
            Self::Microphones => "Microphones",
            Self::Screens => "Screens",
            Self::Places => "Places",
        }
    }

    /// The title of the page.
    pub fn heading(self) -> &'static str {
        match self {
            Self::Outputs => "Play a sound on the output to name",
            Self::Microphones => "Speak into the microphone to name",
            Self::Screens => "Look at the number on the screen to name",
            Self::Places => "Keep the place where you are",
        }
    }

    /// The line under the title.
    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Outputs => {
                "Press Play: the output you hear is the one to name. No need to know its technical name."
            }
            Self::Microphones => {
                "DeskCheck looks for the one that reacts to your voice. No need to know its technical name."
            }
            Self::Screens => {
                "Each screen shows its number for a few seconds. Give it a name you will recognize."
            }
            Self::Places => {
                "DeskCheck will recognize this place next time, and set the sound and the screens for you."
            }
        }
    }

    /// The text of the main button.
    pub fn next_label(self) -> &'static str {
        match self.next() {
            Some(Self::Microphones) => "Next: the microphones",
            Some(Self::Screens) => "Next: the screens",
            Some(Self::Places) => "Next: the places",
            _ => "Finish",
        }
    }

    /// "Step 2 of 4".
    pub fn counter(self) -> String {
        format!("Step {} of {}", self.index() + 1, Self::ALL.len())
    }

    /// The names offered for this step.
    pub fn suggestions(self) -> &'static [&'static str] {
        match self {
            Self::Outputs => &["Headset", "Laptop speakers", "Office screen", "Home screen"],
            Self::Microphones => &["Headset mic", "Laptop mic", "Webcam mic", "Office mic"],
            Self::Screens => &["Laptop", "Office screen", "Home screen", "Big screen"],
            Self::Places => &["Office", "Home", "On the move"],
        }
    }

    /// The icons offered for this step, with their words.
    pub fn icons(self) -> &'static [(DeviceIcon, &'static str)] {
        match self {
            Self::Outputs => &[
                (DeviceIcon::Headset, "Headset"),
                (DeviceIcon::Speakers, "Speakers"),
                (DeviceIcon::Screen, "Screen"),
                (DeviceIcon::Laptop, "Laptop"),
            ],
            Self::Microphones => &[
                (DeviceIcon::Headset, "Headset"),
                (DeviceIcon::Microphone, "Mic"),
                (DeviceIcon::Webcam, "Webcam"),
                (DeviceIcon::Laptop, "Laptop"),
            ],
            Self::Screens | Self::Places => &[],
        }
    }
}

/// The icon DeskCheck guesses for a device, the one picked at first.
pub fn guess_icon(hardware: Hardware, input: bool) -> DeviceIcon {
    use deskcheck_core::audio::Connection;
    match (hardware.form, hardware.connection) {
        (Form::Headset | Form::Headphones, _) => DeviceIcon::Headset,
        (Form::Webcam, _) => DeviceIcon::Webcam,
        (Form::Screen, _) | (_, Connection::Hdmi) => DeviceIcon::Screen,
        (_, Connection::BuiltIn) => DeviceIcon::Laptop,
        _ if input => DeviceIcon::Microphone,
        _ => DeviceIcon::Speakers,
    }
}

/// A device of the list of a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WizardDevice {
    /// The device.
    pub id: DeviceId,
    /// Its name, if the user gave one.
    pub name: Option<String>,
    /// "Headset · alsa_output.usb-…": what the sound server calls it.
    pub technical: String,
    /// Its icon.
    pub icon: &'static str,
    /// The icon chosen, else the one guessed.
    pub choice: DeviceIcon,
}

impl WizardDevice {
    /// The first line of its row.
    pub fn title(&self) -> &str {
        self.name.as_deref().unwrap_or("Not named yet")
    }
}

/// The outputs, for the first step.
pub fn outputs(config: &Config, state: &AudioState) -> Vec<WizardDevice> {
    state
        .outputs
        .iter()
        .map(|output| WizardDevice {
            id: output.id.clone(),
            name: config.audio_name(&output.id).map(str::to_owned),
            technical: format!("{} · {}", output.description, output.id),
            icon: crate::now_model::icon_of(config, &output.id, output.hardware, false),
            choice: config
                .audio_icon(&output.id)
                .unwrap_or_else(|| guess_icon(output.hardware, false)),
        })
        .collect()
}

/// The microphones (never the monitors), for the second step.
pub fn microphones(config: &Config, state: &AudioState) -> Vec<WizardDevice> {
    state
        .microphones()
        .map(|input| WizardDevice {
            id: input.id.clone(),
            name: config.audio_name(&input.id).map(str::to_owned),
            technical: format!("{} · {}", input.description, input.id),
            icon: crate::now_model::icon_of(config, &input.id, input.hardware, true),
            choice: config
                .audio_icon(&input.id)
                .unwrap_or_else(|| guess_icon(input.hardware, true)),
        })
        .collect()
}

/// The small line under a step in the list: "3 named".
pub fn step_note(
    step: Step,
    config: &Config,
    sound: Option<&AudioState>,
    screens: Option<&DisplayState>,
) -> Option<String> {
    let count = match step {
        Step::Outputs => sound.map_or(0, |state| {
            outputs(config, state)
                .iter()
                .filter(|d| d.name.is_some())
                .count()
        }),
        Step::Microphones => sound.map_or(0, |state| {
            microphones(config, state)
                .iter()
                .filter(|d| d.name.is_some())
                .count()
        }),
        Step::Screens => screens.map_or(0, |state| {
            state
                .screens
                .iter()
                .filter(|screen| config.screen_name(&screen.id).is_some())
                .count()
        }),
        Step::Places => {
            let count = config.places().len();
            return (count > 0).then(|| format!("{count} saved"));
        }
    };
    (count > 0).then(|| format!("{count} named"))
}

/// The microphone that hears the voice: of those that heard a voice a
/// moment ago, the loudest now. `levels` gives, for each microphone, its
/// level (0.0 to 1.0) and whether it heard a voice lately.
pub fn hearing(levels: &[(DeviceId, f32, bool)]) -> Option<DeviceId> {
    levels
        .iter()
        .filter(|(_, _, heard)| *heard)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _, _)| id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{Connection, Input, InputKind, Output, Volume};

    #[test]
    fn the_steps_follow_each_other() {
        assert_eq!(Step::Outputs.next(), Some(Step::Microphones));
        assert_eq!(Step::Places.next(), None);
        assert_eq!(Step::Outputs.previous(), None);
        assert_eq!(Step::Screens.previous(), Some(Step::Microphones));
        assert_eq!(Step::Microphones.counter(), "Step 2 of 4");
        assert_eq!(Step::Microphones.next_label(), "Next: the screens");
        assert_eq!(Step::Places.next_label(), "Finish");
        assert_eq!(Step::Microphones.icons().len(), 4);
        assert_eq!(Step::Screens.icons(), []);
    }

    fn state() -> AudioState {
        AudioState {
            outputs: vec![Output {
                id: DeviceId::new("usb"),
                description: "Jabra".to_owned(),
                hardware: Hardware {
                    form: Form::Headset,
                    connection: Connection::Usb,
                },
                volume: Volume::NORMAL,
                muted: false,
            }],
            inputs: vec![
                Input {
                    id: DeviceId::new("usb.monitor"),
                    description: "Monitor of Jabra".to_owned(),
                    hardware: Hardware::default(),
                    kind: InputKind::Monitor {
                        of: DeviceId::new("usb"),
                    },
                    volume: Volume::NORMAL,
                    muted: false,
                },
                Input {
                    id: DeviceId::new("laptop-mic"),
                    description: "Built-in".to_owned(),
                    hardware: Hardware {
                        form: Form::Microphone,
                        connection: Connection::BuiltIn,
                    },
                    kind: InputKind::Capture,
                    volume: Volume::NORMAL,
                    muted: false,
                },
            ],
            ..AudioState::default()
        }
    }

    #[test]
    fn devices_show_their_name_or_say_they_have_none() {
        let mut config = Config::default();
        config.set_audio_name(&DeviceId::new("usb"), Some("Headset".to_owned()));
        let outs = outputs(&config, &state());
        assert_eq!(outs[0].title(), "Headset");
        assert_eq!(outs[0].technical, "Jabra · usb");
        assert_eq!(outs[0].choice, DeviceIcon::Headset);
        let mics = microphones(&config, &state());
        assert_eq!(mics.len(), 1, "a monitor is not a microphone");
        assert_eq!(mics[0].title(), "Not named yet");
        assert_eq!(mics[0].choice, DeviceIcon::Laptop);
        assert_eq!(
            step_note(Step::Outputs, &config, Some(&state()), None).as_deref(),
            Some("1 named")
        );
        assert_eq!(
            step_note(Step::Microphones, &config, Some(&state()), None),
            None
        );
    }

    #[test]
    fn the_loudest_voice_tells_the_microphone() {
        let a = DeviceId::new("a");
        let b = DeviceId::new("b");
        assert_eq!(
            hearing(&[(a.clone(), 0.9, false), (b.clone(), 0.4, true)]),
            Some(b.clone())
        );
        assert_eq!(hearing(&[(a.clone(), 0.9, true), (b, 0.4, true)]), Some(a));
        assert_eq!(hearing(&[]), None);
    }
}
