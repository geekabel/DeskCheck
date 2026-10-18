//! The words about places (DC-28): the badge of the header bar, the line
//! at the bottom of the Now page, and the notification. Pure, so they are
//! tested without a display.

use deskcheck_config::Config;
use deskcheck_core::audio::{AudioState, DeviceId};
use deskcheck_core::display::ScreenId;
use deskcheck_core::places::{Condition, Described, SoundChoice};

/// Where DeskCheck is with the place recognized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineState {
    /// Only shown: the place does nothing on arrival.
    Shown,
    /// Waiting for the user to say "Apply".
    Asking,
    /// Sent to the backend.
    Applying,
    /// Applied, at this time of day.
    Applied {
        /// Like "8:31".
        at: String,
    },
    /// Its choices were in use already.
    InUse,
    /// Applied, then undone.
    Undone,
}

/// The place recognized, as the Now page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceLine {
    /// The name of the place.
    pub name: String,
    /// Why it is recognized, like "the Wi-Fi is UTBM".
    pub because: String,
    /// What DeskCheck did.
    pub state: LineState,
}

/// The button at the end of the line: its text and its action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineButton {
    /// The text of the button.
    pub label: &'static str,
    /// The action of the app it runs.
    pub action: &'static str,
}

/// The action that puts back the settings from before a place.
pub const UNDO_ACTION: &str = "app.place-undo";

/// The action that applies the place DeskCheck asked about.
pub const APPLY_ACTION: &str = "app.place-apply";

impl PlaceLine {
    /// The sentence of the line, as in the design: "Place recognized:
    /// Office, because the screen DELL U2419H is plugged in. Place
    /// settings applied at 8:31."
    #[must_use]
    pub fn text(&self) -> String {
        let start = format!("Place recognized: {}, because {}.", self.name, self.because);
        let end = match &self.state {
            LineState::Shown => return start,
            LineState::Asking => "Apply its settings?".to_owned(),
            LineState::Applying => "Applying its settings…".to_owned(),
            LineState::Applied { at } => format!("Place settings applied at {at}."),
            LineState::InUse => "Its settings are already in use.".to_owned(),
            LineState::Undone => "Settings put back as before.".to_owned(),
        };
        format!("{start} {end}")
    }

    /// The button of the line, if any.
    #[must_use]
    pub fn button(&self) -> Option<LineButton> {
        match self.state {
            LineState::Asking => Some(LineButton {
                label: "Apply",
                action: APPLY_ACTION,
            }),
            LineState::Applied { .. } => Some(LineButton {
                label: "Undo",
                action: UNDO_ACTION,
            }),
            _ => None,
        }
    }
}

/// The text of the badge in the header bar.
#[must_use]
pub fn badge_text(place: Option<&str>, any_saved: bool) -> String {
    match place {
        Some(name) => format!("Place: {name}"),
        None if any_saved => "No place recognized".to_owned(),
        None => "No place yet".to_owned(),
    }
}

/// A screen by the name the user gave it, else its model.
#[must_use]
pub fn screen_name(config: &Config, id: &ScreenId) -> String {
    config
        .screen_name(id)
        .map_or_else(|| id.product.clone(), str::to_owned)
}

/// Why a place is recognized, in words.
#[must_use]
pub fn because(config: &Config, condition: &Condition) -> String {
    Described::new(condition, |id| screen_name(config, id)).to_string()
}

/// A sound device by the name the user gave it, else the name of the
/// sound server.
#[must_use]
pub fn device_name(config: &Config, sound: Option<&AudioState>, id: &DeviceId) -> String {
    if let Some(name) = config.audio_name(id) {
        return name.to_owned();
    }
    sound
        .and_then(|state| {
            state
                .output(id)
                .map(|o| o.description.clone())
                .or_else(|| state.input(id).map(|i| i.description.clone()))
        })
        .unwrap_or_else(|| id.as_str().to_owned())
}

/// The body of the notification once a place is applied: what changed.
#[must_use]
pub fn applied_body(
    config: &Config,
    sound: Option<&AudioState>,
    choice: &SoundChoice,
    layout_changed: bool,
) -> String {
    let mut parts = Vec::new();
    if let Some(output) = &choice.output {
        parts.push(format!("Sound: {}.", device_name(config, sound, output)));
    }
    if let Some(input) = &choice.input {
        parts.push(format!(
            "Microphone: {}.",
            device_name(config, sound, input)
        ));
    }
    if layout_changed {
        parts.push("Screens: laid out as saved.".to_owned());
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(state: LineState) -> PlaceLine {
        PlaceLine {
            name: "Office".to_owned(),
            because: "the Wi-Fi is UTBM".to_owned(),
            state,
        }
    }

    #[test]
    fn the_line_says_what_was_done() {
        assert_eq!(
            line(LineState::Applied {
                at: "8:31".to_owned()
            })
            .text(),
            "Place recognized: Office, because the Wi-Fi is UTBM. Place settings applied at 8:31."
        );
        assert_eq!(
            line(LineState::Shown).text(),
            "Place recognized: Office, because the Wi-Fi is UTBM."
        );
        assert!(
            line(LineState::Asking)
                .text()
                .ends_with("Apply its settings?")
        );
        assert!(line(LineState::InUse).text().ends_with("already in use."));
    }

    #[test]
    fn undo_and_apply_have_their_button() {
        let applied = line(LineState::Applied {
            at: "8:31".to_owned(),
        });
        assert_eq!(applied.button().map(|b| b.action), Some(UNDO_ACTION));
        assert_eq!(
            line(LineState::Asking).button().map(|b| b.label),
            Some("Apply")
        );
        assert_eq!(line(LineState::Undone).button(), None);
        assert_eq!(line(LineState::Shown).button(), None);
    }

    #[test]
    fn the_badge_names_the_place() {
        assert_eq!(badge_text(Some("Office"), true), "Place: Office");
        assert_eq!(badge_text(None, true), "No place recognized");
        assert_eq!(badge_text(None, false), "No place yet");
    }

    #[test]
    fn the_notification_lists_the_changes() {
        let mut config = Config::default();
        config.set_audio_name(&DeviceId::new("usb"), Some("USB headset".to_owned()));
        let choice = SoundChoice {
            output: Some(DeviceId::new("usb")),
            input: Some(DeviceId::new("mic")),
        };
        assert_eq!(
            applied_body(&config, None, &choice, true),
            "Sound: USB headset. Microphone: mic. Screens: laid out as saved."
        );
        assert_eq!(
            because(
                &config,
                &Condition::Screen(ScreenId::new("DEL", "DELL U2419H", "1"))
            ),
            "the screen DELL U2419H is plugged in"
        );
    }
}
