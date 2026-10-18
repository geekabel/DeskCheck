//! Places, like the office or home: how DeskCheck recognizes them, and
//! the choices of sound, microphone and screens kept for each.
//!
//! A place is recognized when one of its conditions holds (they are joined
//! by "or", as in the design): a screen is plugged in, the Wi-Fi has a
//! given name, or no external screen is plugged in. A screen is the most
//! telling clue, so a place recognized by a screen wins over one
//! recognized by its Wi-Fi, which wins over "no external screen".
//!
//! Everything here is pure: what is plugged in comes in an [`Observed`].
//!
//! On arrival (DC-28), [`Place::sound_choice`] and [`SavedLayout::is_in_use`]
//! tell what would change, a [`Snapshot`] keeps what was in use for Undo,
//! and [`Arrivals`] tells when the place changes, so DeskCheck acts once
//! per arrival and not at each update.

use std::fmt;

use crate::audio::{AudioState, DeviceId};
use crate::display::{DisplayState, ScreenId};

/// A clue that tells DeskCheck where the computer is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    /// This screen is plugged in (on or off).
    Screen(ScreenId),
    /// The Wi-Fi in use has this name.
    Wifi(String),
    /// No screen other than the built-in one is plugged in.
    NoExternalScreen,
}

impl Condition {
    /// How telling the clue is: a lower rank wins.
    fn rank(&self) -> u8 {
        match self {
            Self::Screen(_) => 0,
            Self::Wifi(_) => 1,
            Self::NoExternalScreen => 2,
        }
    }
}

/// What DeskCheck does when it recognizes a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnArrival {
    /// Apply its choices, and say so (with Undo).
    #[default]
    Apply,
    /// Ask before applying them.
    Ask,
    /// Only show the place.
    Nothing,
}

impl OnArrival {
    /// Its name in the settings file.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Ask => "ask",
            Self::Nothing => "nothing",
        }
    }

    /// Reads a name of the settings file.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "apply" => Some(Self::Apply),
            "ask" => Some(Self::Ask),
            "nothing" => Some(Self::Nothing),
            _ => None,
        }
    }
}

/// A screen of a saved layout.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedScreen {
    /// The screen.
    pub id: ScreenId,
    /// Left edge on the desktop.
    pub x: i32,
    /// Top edge on the desktop.
    pub y: i32,
    /// Scale.
    pub scale: f64,
    /// The primary one.
    pub primary: bool,
    /// Width of its mode, in pixels.
    pub width: u32,
    /// Height of its mode, in pixels.
    pub height: u32,
    /// Refresh rate of its mode, in hertz.
    pub refresh_rate: f64,
}

/// The layout of the screens kept for a place.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SavedLayout {
    /// The screens that are on, with their place.
    pub screens: Vec<SavedScreen>,
}

impl SavedLayout {
    /// The layout of the screens that are on now.
    #[must_use]
    pub fn from_state(state: &DisplayState) -> Self {
        Self {
            screens: state
                .enabled()
                .into_iter()
                .filter_map(|screen| {
                    let placement = screen.placement?;
                    let mode = screen.current_mode?;
                    Some(SavedScreen {
                        id: screen.id.clone(),
                        x: placement.x,
                        y: placement.y,
                        scale: placement.scale,
                        primary: placement.primary,
                        width: mode.width,
                        height: mode.height,
                        refresh_rate: mode.refresh_rate,
                    })
                })
                .collect(),
        }
    }

    /// "Extended · Primary: 2"-like words, with the numbers of the saved
    /// order (left to right).
    #[must_use]
    pub fn summary(&self) -> String {
        let mut places: Vec<(i32, i32)> = self.screens.iter().map(|s| (s.x, s.y)).collect();
        places.sort_unstable();
        places.dedup();
        let layout = match (self.screens.len(), places.len()) {
            (0, _) => return "No layout kept".to_owned(),
            (1, _) => "One screen",
            (_, 1) => "Mirrored",
            _ => "Extended",
        };
        let mut ordered: Vec<&SavedScreen> = self.screens.iter().collect();
        ordered.sort_by_key(|s| (s.x, s.y));
        match ordered.iter().position(|s| s.primary) {
            Some(at) if self.screens.len() > 1 => format!("{layout} · Primary: {}", at + 1),
            _ => layout.to_owned(),
        }
    }
}

/// A place, with its clues and its choices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Place {
    /// Its name, like "Office".
    pub name: String,
    /// The clues; any one of them is enough.
    pub conditions: Vec<Condition>,
    /// Outputs, best first: the first one plugged in is used.
    pub outputs: Vec<DeviceId>,
    /// Outputs never chosen here.
    pub never_outputs: Vec<DeviceId>,
    /// Microphones, best first.
    pub inputs: Vec<DeviceId>,
    /// The layout of the screens, if one was kept.
    pub layout: Option<SavedLayout>,
    /// What to do when DeskCheck recognizes it.
    pub on_arrival: OnArrival,
}

/// `true` when two identities are the same screen. A screen without a
/// useful serial number matches any screen of its model.
#[must_use]
pub fn same_screen(a: &ScreenId, b: &ScreenId) -> bool {
    a.vendor == b.vendor
        && a.product == b.product
        && (!a.has_serial() || !b.has_serial() || a.serial == b.serial)
}

impl Place {
    /// The first output of the list that is plugged in, never one of the
    /// outputs to avoid.
    #[must_use]
    pub fn pick_output<'a>(&'a self, plugged: &[DeviceId]) -> Option<&'a DeviceId> {
        self.outputs
            .iter()
            .find(|id| plugged.contains(id) && !self.never_outputs.contains(id))
    }

    /// The first microphone of the list that is plugged in. `plugged`
    /// holds microphones only: a monitor is never chosen.
    #[must_use]
    pub fn pick_input<'a>(&'a self, plugged: &[DeviceId]) -> Option<&'a DeviceId> {
        self.inputs.iter().find(|id| plugged.contains(id))
    }

    /// The condition of this place that holds, the most telling first.
    #[must_use]
    pub fn matching(&self, observed: &Observed) -> Option<&Condition> {
        self.conditions
            .iter()
            .filter(|condition| observed.holds(condition))
            .min_by_key(|condition| condition.rank())
    }
}

/// What is plugged in, to recognize a place.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Observed {
    /// Every screen plugged in, on or off.
    pub screens: Vec<ScreenId>,
    /// How many of them are not built in.
    pub external_screens: usize,
    /// The name of the Wi-Fi in use.
    pub wifi: Option<String>,
}

impl Observed {
    /// What the screens and the Wi-Fi say.
    #[must_use]
    pub fn new(state: Option<&DisplayState>, wifi: Option<String>) -> Self {
        let screens = state.map(|s| s.screens.as_slice()).unwrap_or_default();
        Self {
            screens: screens.iter().map(|s| s.id.clone()).collect(),
            external_screens: screens.iter().filter(|s| !s.builtin).count(),
            wifi,
        }
    }

    /// `true` when the condition holds now.
    #[must_use]
    pub fn holds(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Screen(id) => self.screens.iter().any(|seen| same_screen(seen, id)),
            Condition::Wifi(name) => self.wifi.as_deref() == Some(name.as_str()),
            Condition::NoExternalScreen => {
                // Only when the screens could be read.
                !self.screens.is_empty() && self.external_screens == 0
            }
        }
    }
}

/// The place DeskCheck recognized, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    /// Its position in the list of places.
    pub index: usize,
    /// The condition that holds.
    pub because: Condition,
}

/// The place where the computer is, if one matches: the one with the most
/// telling condition, the first of the list when two are as telling.
#[must_use]
pub fn detect(places: &[Place], observed: &Observed) -> Option<Detection> {
    places
        .iter()
        .enumerate()
        .filter_map(|(index, place)| {
            place.matching(observed).map(|condition| Detection {
                index,
                because: condition.clone(),
            })
        })
        .min_by_key(|detection| (detection.because.rank(), detection.index))
}

/// The default output and microphone a place asks for, when they are not
/// the ones in use already.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SoundChoice {
    /// The output to make the default one.
    pub output: Option<DeviceId>,
    /// The microphone to make the default one.
    pub input: Option<DeviceId>,
}

impl SoundChoice {
    /// `true` when nothing has to change.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.output.is_none() && self.input.is_none()
    }
}

impl Place {
    /// What the place changes in the sound now: the first output and the
    /// first microphone of its lists that are plugged in, unless they are
    /// the defaults already.
    #[must_use]
    pub fn sound_choice(&self, state: &AudioState) -> SoundChoice {
        let outputs: Vec<DeviceId> = state.outputs.iter().map(|o| o.id.clone()).collect();
        let microphones: Vec<DeviceId> = state
            .inputs
            .iter()
            .filter(|input| !input.is_monitor())
            .map(|input| input.id.clone())
            .collect();
        SoundChoice {
            output: self
                .pick_output(&outputs)
                .filter(|id| state.default_output.as_ref() != Some(*id))
                .cloned(),
            input: self
                .pick_input(&microphones)
                .filter(|id| state.default_input.as_ref() != Some(*id))
                .cloned(),
        }
    }
}

impl SavedLayout {
    /// `true` when the screens are already laid out this way: the same
    /// screens on, at the same places, with the same mode, scale and
    /// primary screen.
    #[must_use]
    pub fn is_in_use(&self, state: &DisplayState) -> bool {
        let now = Self::from_state(state);
        now.screens.len() == self.screens.len()
            && self.screens.iter().all(|saved| {
                now.screens.iter().any(|seen| {
                    same_screen(&seen.id, &saved.id)
                        && seen.x == saved.x
                        && seen.y == saved.y
                        && seen.primary == saved.primary
                        && seen.width == saved.width
                        && seen.height == saved.height
                        && (seen.scale - saved.scale).abs() < 1e-4
                        && (seen.refresh_rate - saved.refresh_rate).abs() < 0.5
                })
            })
    }

    /// `true` when every screen of the layout is plugged in, so it can be
    /// applied.
    #[must_use]
    pub fn fits(&self, state: &DisplayState) -> bool {
        !self.screens.is_empty()
            && self.screens.iter().all(|saved| {
                state
                    .screens
                    .iter()
                    .any(|screen| same_screen(&screen.id, &saved.id))
            })
    }
}

/// What was in use before a place was applied, to put it back.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    /// The default output before, if the place changed it.
    pub output: Option<DeviceId>,
    /// The default microphone before, if the place changed it.
    pub input: Option<DeviceId>,
    /// The layout of the screens before, if the place changed it.
    pub layout: Option<SavedLayout>,
}

impl Snapshot {
    /// `true` when the place changed nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.output.is_none() && self.input.is_none() && self.layout.is_none()
    }
}

/// Tells when the recognized place changes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arrivals {
    last: Option<String>,
    started: bool,
}

impl Arrivals {
    /// Takes the place recognized now, by name. Returns `true` on arrival:
    /// a place is recognized and it was not the one before. The first
    /// place seen after DeskCheck starts is an arrival too.
    pub fn see(&mut self, place: Option<&str>) -> bool {
        let arrived = place.is_some() && (!self.started || self.last.as_deref() != place);
        self.started = true;
        self.last = place.map(str::to_owned);
        arrived
    }

    /// A place was renamed: when it is the one recognized, it is still
    /// the same place, not a new arrival.
    pub fn rename(&mut self, from: &str, to: &str) {
        if self.last.as_deref() == Some(from) {
            self.last = Some(to.to_owned());
        }
    }

    /// The place recognized last.
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.last.as_deref()
    }
}

/// A condition in words, with the name the user gave a screen.
pub struct Described<'a, F: Fn(&ScreenId) -> String> {
    condition: &'a Condition,
    screen_name: F,
}

impl<'a, F: Fn(&ScreenId) -> String> Described<'a, F> {
    /// Describes `condition`, naming screens with `screen_name`.
    pub fn new(condition: &'a Condition, screen_name: F) -> Self {
        Self {
            condition,
            screen_name,
        }
    }
}

impl<F: Fn(&ScreenId) -> String> fmt::Display for Described<'_, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.condition {
            Condition::Screen(id) => {
                write!(f, "the screen {} is plugged in", (self.screen_name)(id))
            }
            Condition::Wifi(name) => write!(f, "the Wi-Fi is {name}"),
            Condition::NoExternalScreen => f.write_str("no external screen is plugged in"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{Mode, Placement, Screen};

    fn dell() -> ScreenId {
        ScreenId::new("DEL", "DELL U2419H", "ABC1234")
    }

    fn asus() -> ScreenId {
        ScreenId::new("AUS", "XG27ACS", "T2LMTF031523")
    }

    fn screen(id: ScreenId, builtin: bool, x: i32, primary: bool) -> Screen {
        Screen {
            id,
            connector: String::new(),
            display_name: None,
            builtin,
            current_mode: Some(Mode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
            }),
            placement: Some(Placement {
                x,
                y: 0,
                scale: 1.0,
                primary,
            }),
        }
    }

    fn laptop() -> ScreenId {
        ScreenId::new("BOE", "0x0a1c", "0x00000000")
    }

    fn places() -> Vec<Place> {
        vec![
            Place {
                name: "Office".to_owned(),
                conditions: vec![
                    Condition::Wifi("UTBM".to_owned()),
                    Condition::Screen(dell()),
                ],
                ..Place::default()
            },
            Place {
                name: "Home".to_owned(),
                conditions: vec![Condition::Screen(asus()), Condition::Wifi("Box".to_owned())],
                ..Place::default()
            },
            Place {
                name: "On the move".to_owned(),
                conditions: vec![Condition::NoExternalScreen],
                ..Place::default()
            },
        ]
    }

    fn at(screens: &[Screen], wifi: Option<&str>) -> Observed {
        Observed::new(
            Some(&DisplayState {
                screens: screens.to_vec(),
            }),
            wifi.map(str::to_owned),
        )
    }

    #[test]
    fn the_office_is_recognized_by_its_screen() {
        let observed = at(
            &[
                screen(laptop(), true, 0, false),
                screen(dell(), false, 1536, true),
            ],
            Some("UTBM"),
        );
        let found = detect(&places(), &observed).unwrap();
        assert_eq!(found.index, 0);
        // The screen says more than the Wi-Fi.
        assert_eq!(found.because, Condition::Screen(dell()));
    }

    #[test]
    fn a_screen_wins_over_the_wifi_of_another_place() {
        // At home with the ASUS, on the Wi-Fi of the office (a phone
        // sharing its connection with the same name, say).
        let observed = at(
            &[
                screen(laptop(), true, 0, true),
                screen(asus(), false, 1920, false),
            ],
            Some("UTBM"),
        );
        assert_eq!(detect(&places(), &observed).unwrap().index, 1);
    }

    #[test]
    fn the_wifi_alone_is_enough() {
        let observed = at(&[screen(laptop(), true, 0, true)], Some("Box"));
        let found = detect(&places(), &observed).unwrap();
        assert_eq!(
            (found.index, found.because),
            (1, Condition::Wifi("Box".to_owned()))
        );
    }

    #[test]
    fn without_screen_nor_known_wifi_the_laptop_is_on_the_move() {
        let observed = at(&[screen(laptop(), true, 0, true)], Some("Café"));
        assert_eq!(detect(&places(), &observed).unwrap().index, 2);
        // Screens not read: no place by "no external screen".
        assert_eq!(detect(&places(), &Observed::new(None, None)), None);
    }

    #[test]
    fn a_screen_without_serial_matches_its_model() {
        let no_serial = ScreenId::new("DEL", "DELL U2419H", "");
        assert!(same_screen(&no_serial, &dell()));
        assert!(!same_screen(
            &ScreenId::new("DEL", "DELL U2419H", "OTHER"),
            &dell()
        ));
        assert!(!same_screen(&asus(), &dell()));
    }

    #[test]
    fn the_first_device_plugged_in_is_picked() {
        let place = Place {
            outputs: vec![DeviceId::new("headset"), DeviceId::new("laptop")],
            never_outputs: vec![DeviceId::new("dell")],
            inputs: vec![DeviceId::new("headset.mic"), DeviceId::new("webcam.mic")],
            ..Place::default()
        };
        let plugged = [DeviceId::new("laptop"), DeviceId::new("dell")];
        assert_eq!(place.pick_output(&plugged), Some(&DeviceId::new("laptop")));
        assert_eq!(place.pick_output(&[DeviceId::new("dell")]), None);
        assert_eq!(
            place.pick_input(&[DeviceId::new("webcam.mic"), DeviceId::new("headset.mic")]),
            Some(&DeviceId::new("headset.mic"))
        );
    }

    #[test]
    fn a_layout_is_kept_and_described() {
        let state = DisplayState {
            screens: vec![
                screen(laptop(), true, 0, false),
                screen(dell(), false, 1920, true),
            ],
        };
        let layout = SavedLayout::from_state(&state);
        assert_eq!(layout.screens.len(), 2);
        assert_eq!(layout.summary(), "Extended · Primary: 2");
        assert_eq!(SavedLayout::default().summary(), "No layout kept");
    }

    #[test]
    fn conditions_are_said_in_words() {
        let name = |_: &ScreenId| "Dell U2419H".to_owned();
        assert_eq!(
            Described::new(&Condition::Screen(dell()), name).to_string(),
            "the screen Dell U2419H is plugged in"
        );
        assert_eq!(
            Described::new(&Condition::Wifi("UTBM".to_owned()), name).to_string(),
            "the Wi-Fi is UTBM"
        );
        assert_eq!(OnArrival::from_key("ask"), Some(OnArrival::Ask));
        assert_eq!(OnArrival::Nothing.key(), "nothing");
    }

    fn sound() -> AudioState {
        use crate::audio::{Hardware, Input, InputKind, Output, Volume};
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
            outputs: vec![output("laptop"), output("usb")],
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
            default_output: Some(DeviceId::new("laptop")),
            default_input: Some(DeviceId::new("usb-mic")),
            ..AudioState::default()
        }
    }

    #[test]
    fn only_what_differs_is_changed() {
        let place = Place {
            outputs: vec![DeviceId::new("usb"), DeviceId::new("laptop")],
            inputs: vec![
                DeviceId::new("usb.monitor"),
                DeviceId::new("usb-mic"),
                DeviceId::new("laptop-mic"),
            ],
            ..Place::default()
        };
        let choice = place.sound_choice(&sound());
        // The output changes; the microphone is the right one already, and
        // a monitor is never taken as a microphone.
        assert_eq!(choice.output, Some(DeviceId::new("usb")));
        assert_eq!(choice.input, None);
        assert!(!choice.is_empty());
        assert!(Place::default().sound_choice(&sound()).is_empty());
    }

    #[test]
    fn a_layout_in_use_is_not_applied_again() {
        let state = DisplayState {
            screens: vec![
                screen(laptop(), true, 0, false),
                screen(dell(), false, 1920, true),
            ],
        };
        let saved = SavedLayout::from_state(&state);
        assert!(saved.is_in_use(&state));
        assert!(saved.fits(&state));
        let mut moved = state.clone();
        moved.screens[1].placement.as_mut().unwrap().primary = false;
        moved.screens[0].placement.as_mut().unwrap().primary = true;
        assert!(!saved.is_in_use(&moved));
        // Without the Dell, the layout cannot be applied.
        let alone = DisplayState {
            screens: vec![screen(laptop(), true, 0, true)],
        };
        assert!(!saved.fits(&alone));
        assert!(!SavedLayout::default().fits(&alone));
    }

    #[test]
    fn a_place_is_acted_on_once_per_arrival() {
        let mut arrivals = Arrivals::default();
        assert!(!arrivals.see(None));
        assert!(arrivals.see(Some("Office")));
        assert!(!arrivals.see(Some("Office")));
        assert!(arrivals.see(Some("Home")));
        assert!(!arrivals.see(None));
        assert_eq!(arrivals.current(), None);
        assert!(arrivals.see(Some("Home")));
        assert_eq!(arrivals.current(), Some("Home"));
        arrivals.rename("Home", "House");
        assert!(!arrivals.see(Some("House")));
        assert!(Snapshot::default().is_empty());
    }
}
