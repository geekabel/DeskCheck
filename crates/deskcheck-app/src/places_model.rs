//! The words about places (DC-28): the badge of the header bar, the line
//! at the bottom of the Now page, and the notification. And what the
//! Places page shows and changes (DC-29). Pure, so they are tested
//! without a display.

use deskcheck_config::Config;
use deskcheck_core::audio::{AudioState, DeviceId};
use deskcheck_core::display::{DisplayState, ScreenId};
use deskcheck_core::places::{Condition, Described, Place, SavedLayout, SoundChoice};
use gtk::glib;

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

/// The short line under the name of a place in the list.
#[must_use]
pub fn place_summary(config: &Config, place: &Place) -> String {
    match place.conditions.first() {
        Some(Condition::Screen(id)) => format!("Screen {} plugged in", screen_name(config, id)),
        Some(Condition::Wifi(name)) => format!("Wi-Fi {name}"),
        Some(Condition::NoExternalScreen) => "No external screen".to_owned(),
        None => "Not recognized yet: add a condition".to_owned(),
    }
}

/// A condition of the "Recognized when" card, in Pango markup, the name in
/// bold as in the design.
#[must_use]
pub fn condition_markup(config: &Config, condition: &Condition) -> String {
    let bold = |text: &str| format!("<b>{}</b>", glib::markup_escape_text(text));
    match condition {
        Condition::Screen(id) if id.has_serial() => format!(
            "The screen {} n° {} is plugged in",
            bold(&screen_name(config, id)),
            glib::markup_escape_text(&id.serial)
        ),
        Condition::Screen(id) => {
            format!(
                "The screen {} is plugged in",
                bold(&screen_name(config, id))
            )
        }
        Condition::Wifi(name) => format!("The Wi-Fi is {}", bold(name)),
        Condition::NoExternalScreen => "No external screen is plugged in".to_owned(),
    }
}

/// The icon of a condition.
#[must_use]
pub fn condition_icon(condition: &Condition) -> &'static str {
    match condition {
        Condition::Screen(_) => "dc-screen-symbolic",
        Condition::Wifi(_) => "dc-wifi-symbolic",
        Condition::NoExternalScreen => "dc-laptop-symbolic",
    }
}

/// The conditions that can be added now: each external screen plugged
/// in, the Wi-Fi in use, and "no external screen". Those the place has
/// already are left out.
#[must_use]
pub fn condition_choices(
    place: &Place,
    screens: Option<&DisplayState>,
    wifi: Option<&str>,
) -> Vec<Condition> {
    let mut choices: Vec<Condition> = screens
        .map(|state| {
            state
                .screens
                .iter()
                .filter(|screen| !screen.builtin)
                .map(|screen| Condition::Screen(screen.id.clone()))
                .collect()
        })
        .unwrap_or_default();
    if let Some(name) = wifi {
        choices.push(Condition::Wifi(name.to_owned()));
    }
    choices.push(Condition::NoExternalScreen);
    choices.retain(|choice| !place.conditions.contains(choice));
    choices
}

/// A name no other place has: "New place", then "New place 2"...
#[must_use]
pub fn unique_name(places: &[Place], base: &str) -> String {
    let taken = |name: &str| places.iter().any(|place| place.name == name);
    if !taken(base) {
        return base.to_owned();
    }
    // One of the next numbers is free: there are only so many places.
    (2..=places.len() + 2)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| base.to_owned())
}

/// A new place, made from what is in use now: the default output and
/// microphone first in their lists, then the others plugged in, the
/// layout of the screens, and the most telling condition that holds (an
/// external screen, else the Wi-Fi, else "no external screen").
#[must_use]
pub fn new_place(
    name: String,
    sound: Option<&AudioState>,
    screens: Option<&DisplayState>,
    wifi: Option<&str>,
) -> Place {
    let mut place = Place {
        name,
        ..Place::default()
    };
    if let Some(state) = sound {
        let mut outputs: Vec<DeviceId> = state.outputs.iter().map(|o| o.id.clone()).collect();
        if let Some(default) = &state.default_output {
            outputs.sort_by_key(|id| id != default);
        }
        let mut inputs: Vec<DeviceId> = state
            .inputs
            .iter()
            .filter(|input| !input.is_monitor())
            .map(|input| input.id.clone())
            .collect();
        if let Some(default) = &state.default_input {
            inputs.sort_by_key(|id| id != default);
        }
        place.outputs = outputs;
        place.inputs = inputs;
    }
    if let Some(state) = screens {
        let layout = SavedLayout::from_state(state);
        place.layout = (!layout.screens.is_empty()).then_some(layout);
    }
    place.conditions = condition_choices(&place, screens, wifi)
        .into_iter()
        .take(1)
        .collect();
    place
}

/// Moves the item at `from` to the place of the item at `to`.
pub fn move_item<T>(list: &mut Vec<T>, from: usize, to: usize) {
    if from >= list.len() || to >= list.len() || from == to {
        return;
    }
    let item = list.remove(from);
    list.insert(to, item);
}

/// The devices plugged in that a list does not have yet.
#[must_use]
pub fn devices_to_add(
    plugged: &[DeviceId],
    list: &[DeviceId],
    never: &[DeviceId],
) -> Vec<DeviceId> {
    plugged
        .iter()
        .filter(|id| !list.contains(id) && !never.contains(id))
        .cloned()
        .collect()
}

/// The words beside the small map of the Screens card: "Extended",
/// "Primary: 2", one per line.
#[must_use]
pub fn layout_words(layout: Option<&SavedLayout>) -> String {
    layout.map_or_else(
        || "No layout kept".to_owned(),
        |layout| layout.summary().replace(" · ", "\n"),
    )
}

/// A screen of the small map: its number (left to right), its size in
/// pixels of the map, and if it is the primary one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniScreen {
    /// 1 for the leftmost.
    pub number: usize,
    /// The screen.
    pub id: ScreenId,
    /// Width in pixels.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
    /// The primary one.
    pub primary: bool,
}

/// The screens of a layout drawn side by side, the tallest `height`
/// pixels high, as in the design.
#[must_use]
pub fn mini_map(layout: &SavedLayout, height: i32) -> Vec<MiniScreen> {
    let mut screens: Vec<_> = layout.screens.iter().collect();
    screens.sort_by_key(|s| (s.x, s.y));
    // The size on the desktop: the mode divided by the scale.
    let size = |s: &deskcheck_core::places::SavedScreen| {
        let scale = if s.scale > 0.0 { s.scale } else { 1.0 };
        (f64::from(s.width) / scale, f64::from(s.height) / scale)
    };
    let tallest = screens.iter().map(|s| size(s).1).fold(1.0_f64, f64::max);
    #[allow(clippy::cast_possible_truncation, reason = "a few dozen pixels")]
    screens
        .iter()
        .enumerate()
        .map(|(at, s)| {
            let (w, h) = size(s);
            let factor = f64::from(height) / tallest;
            MiniScreen {
                number: at + 1,
                id: s.id.clone(),
                width: (w * factor).round() as i32,
                height: (h * factor).round() as i32,
                primary: s.primary,
            }
        })
        .collect()
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

    fn sound() -> AudioState {
        use deskcheck_core::audio::{Hardware, Input, InputKind, Output, Volume};
        let output = |id: &str| Output {
            id: DeviceId::new(id),
            description: id.to_owned(),
            hardware: Hardware::default(),
            volume: Volume::NORMAL,
            muted: false,
        };
        let input = |id: &str, kind: InputKind| Input {
            id: DeviceId::new(id),
            description: id.to_owned(),
            hardware: Hardware::default(),
            kind,
            volume: Volume::NORMAL,
            muted: false,
        };
        AudioState {
            outputs: vec![output("speakers"), output("usb")],
            inputs: vec![
                input(
                    "usb.monitor",
                    InputKind::Monitor {
                        of: DeviceId::new("usb"),
                    },
                ),
                input("laptop-mic", InputKind::Capture),
                input("usb-mic", InputKind::Capture),
            ],
            default_output: Some(DeviceId::new("usb")),
            default_input: Some(DeviceId::new("usb-mic")),
            ..AudioState::default()
        }
    }

    fn screens() -> DisplayState {
        use deskcheck_core::display::{Mode, Placement, Screen};
        let screen = |id: ScreenId, builtin: bool, x: i32, w: u32, h: u32| Screen {
            id,
            connector: String::new(),
            display_name: None,
            builtin,
            current_mode: Some(Mode {
                width: w,
                height: h,
                refresh_rate: 60.0,
            }),
            placement: Some(Placement {
                x,
                y: 0,
                scale: 1.0,
                primary: !builtin,
            }),
        };
        DisplayState {
            screens: vec![
                screen(ScreenId::new("BOE", "0x0a1c", ""), true, 0, 1920, 1080),
                screen(
                    ScreenId::new("DEL", "DELL U2419H", "AB1"),
                    false,
                    1920,
                    2560,
                    1440,
                ),
            ],
        }
    }

    #[test]
    fn a_new_place_takes_what_is_in_use() {
        let place = new_place(
            "Office".to_owned(),
            Some(&sound()),
            Some(&screens()),
            Some("UTBM"),
        );
        assert_eq!(
            place.outputs,
            [DeviceId::new("usb"), DeviceId::new("speakers")]
        );
        // The default microphone first, and never a monitor.
        assert_eq!(
            place.inputs,
            [DeviceId::new("usb-mic"), DeviceId::new("laptop-mic")]
        );
        assert_eq!(
            place.conditions,
            [Condition::Screen(ScreenId::new(
                "DEL",
                "DELL U2419H",
                "AB1"
            ))]
        );
        assert_eq!(place.layout.as_ref().map(|l| l.screens.len()), Some(2));
        // Without an external screen, the Wi-Fi tells the place.
        let alone = new_place("Home".to_owned(), None, None, Some("Box"));
        assert_eq!(alone.conditions, [Condition::Wifi("Box".to_owned())]);
        assert_eq!(alone.layout, None);
    }

    #[test]
    fn conditions_already_there_are_not_offered() {
        let place = new_place("Office".to_owned(), None, Some(&screens()), Some("UTBM"));
        assert_eq!(
            condition_choices(&place, Some(&screens()), Some("UTBM")),
            [
                Condition::Wifi("UTBM".to_owned()),
                Condition::NoExternalScreen
            ]
        );
        let config = Config::default();
        assert_eq!(
            condition_markup(&config, &place.conditions[0]),
            "The screen <b>DELL U2419H</b> n° AB1 is plugged in"
        );
        assert_eq!(
            condition_markup(&config, &Condition::Wifi("A&B".to_owned())),
            "The Wi-Fi is <b>A&amp;B</b>"
        );
        assert_eq!(
            place_summary(&config, &place),
            "Screen DELL U2419H plugged in"
        );
        assert_eq!(
            place_summary(&config, &Place::default()),
            "Not recognized yet: add a condition"
        );
    }

    #[test]
    fn names_and_lists_are_handled() {
        let places = vec![
            Place {
                name: "New place".to_owned(),
                ..Place::default()
            },
            Place {
                name: "New place 2".to_owned(),
                ..Place::default()
            },
        ];
        assert_eq!(unique_name(&places, "New place"), "New place 3");
        assert_eq!(unique_name(&places, "Home"), "Home");
        let mut list = vec![1, 2, 3];
        move_item(&mut list, 2, 0);
        assert_eq!(list, [3, 1, 2]);
        move_item(&mut list, 0, 9);
        assert_eq!(list, [3, 1, 2]);
        let plugged = [DeviceId::new("a"), DeviceId::new("b"), DeviceId::new("c")];
        assert_eq!(
            devices_to_add(&plugged, &[DeviceId::new("a")], &[DeviceId::new("c")]),
            [DeviceId::new("b")]
        );
    }

    #[test]
    fn the_small_map_keeps_the_proportions() {
        let layout = SavedLayout::from_state(&screens());
        let map = mini_map(&layout, 58);
        assert_eq!(
            map.iter()
                .map(|s| (s.number, s.width, s.height, s.primary))
                .collect::<Vec<_>>(),
            [(1, 77, 44, false), (2, 103, 58, true)]
        );
        assert_eq!(layout_words(Some(&layout)), "Extended\nPrimary: 2");
        assert_eq!(layout_words(None), "No layout kept");
    }
}
