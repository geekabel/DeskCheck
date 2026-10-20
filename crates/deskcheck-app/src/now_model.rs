//! What the Now page shows, computed from the state. Pure, so it is tested
//! without a display.

use deskcheck_config::{Color, Config, DeviceIcon};
use deskcheck_core::audio::{
    App, AudioState, Connection, DeviceId, Fix, Form, Hardware, Problem, StreamId, best_microphone,
    diagnose,
};
use deskcheck_core::display::DisplayState;

use crate::screens_model::Layout;

/// Colors given to screens without a color of their own, by number: the
/// four colors of the design, then two more.
const SCREEN_COLORS: [Color; 6] = [
    Color {
        red: 0x1a,
        green: 0x5f,
        blue: 0xb4,
    },
    Color {
        red: 0xb5,
        green: 0x47,
        blue: 0x08,
    },
    Color {
        red: 0x0f,
        green: 0x76,
        blue: 0x6e,
    },
    Color {
        red: 0x6d,
        green: 0x28,
        blue: 0xd9,
    },
    Color {
        red: 0xc6,
        green: 0x46,
        blue: 0x00,
    },
    Color {
        red: 0xa5,
        green: 0x1d,
        blue: 0x2d,
    },
];

/// The icon of a device, from its hardware: one of the icons of the design.
/// `input` picks the icon when the hardware says nothing.
pub fn device_icon(hardware: Hardware, input: bool) -> &'static str {
    match (hardware.form, hardware.connection) {
        (Form::Headset | Form::Headphones, _) => "dc-headphones-symbolic",
        (Form::Webcam, _) => "dc-webcam-symbolic",
        (Form::Screen, _) | (_, Connection::Hdmi) => "dc-screen-symbolic",
        (_, Connection::BuiltIn) => "dc-laptop-symbolic",
        (Form::Microphone, _) => "dc-microphone-symbolic",
        (Form::Unknown, _) if input => "dc-microphone-symbolic",
        (Form::Speakers | Form::Unknown, _) => "dc-speaker-symbolic",
    }
}

/// The icon of a [`DeviceIcon`] the user chose.
pub fn chosen_icon(icon: DeviceIcon) -> &'static str {
    match icon {
        DeviceIcon::Headset => "dc-headphones-symbolic",
        DeviceIcon::Speakers => "dc-speaker-symbolic",
        DeviceIcon::Screen => "dc-screen-symbolic",
        DeviceIcon::Laptop => "dc-laptop-symbolic",
        DeviceIcon::Microphone => "dc-microphone-symbolic",
        DeviceIcon::Webcam => "dc-webcam-symbolic",
    }
}

/// The icon of a device: the one the user chose, else the one of its
/// hardware.
pub fn icon_of(config: &Config, id: &DeviceId, hardware: Hardware, input: bool) -> &'static str {
    config
        .audio_icon(id)
        .map_or_else(|| device_icon(hardware, input), chosen_icon)
}

/// The icon of an app: a globe for a web browser, as in the design.
pub fn app_icon(app: &App) -> &'static str {
    let names = [Some(app.name.as_str()), app.binary.as_deref()];
    let browser = names.into_iter().flatten().any(|name| {
        let name = name.to_ascii_lowercase();
        ["firefox", "epiphany", "web"]
            .iter()
            .any(|browser| name.contains(browser))
    });
    if browser {
        "dc-browser-symbolic"
    } else {
        "dc-app-symbolic"
    }
}

/// How an app row looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTag {
    /// Nothing wrong.
    Ok,
    /// The app records a monitor, not a microphone.
    NotAMicrophone,
}

/// One row of the apps card: an app with where its sound goes and comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRow {
    /// The app.
    pub name: String,
    /// Its icon.
    pub icon: &'static str,
    /// `Microphone: … · Sound: …`.
    pub detail: String,
    /// Its tag.
    pub tag: AppTag,
}

fn add_once(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

/// The rows of the apps card, one per app, in the order they appear.
/// Level meters are left out: they only measure.
pub fn app_rows(config: &Config, state: &AudioState) -> Vec<AppRow> {
    struct Usage {
        name: String,
        icon: &'static str,
        microphones: Vec<String>,
        outputs: Vec<String>,
        records_monitor: bool,
    }
    let mut apps: Vec<Usage> = Vec::new();
    let mut usage = |app: &App| -> usize {
        if let Some(at) = apps.iter().position(|known| known.name == app.name) {
            return at;
        }
        apps.push(Usage {
            name: app.name.clone(),
            icon: app_icon(app),
            microphones: Vec::new(),
            outputs: Vec::new(),
            records_monitor: false,
        });
        apps.len() - 1
    };
    let mut rows_of_recordings = Vec::new();
    for recording in state.recordings.iter().filter(|r| !r.level_meter) {
        let at = usage(&recording.app);
        rows_of_recordings.push((at, recording));
    }
    let mut rows_of_playbacks = Vec::new();
    for playback in &state.playbacks {
        let at = usage(&playback.app);
        rows_of_playbacks.push((at, playback));
    }
    for (at, recording) in rows_of_recordings {
        let (label, monitor) = state.input(&recording.input).map_or_else(
            || (recording.input.to_string(), false),
            |input| (config.input_label(input).into_owned(), input.is_monitor()),
        );
        add_once(&mut apps[at].microphones, label);
        apps[at].records_monitor |= monitor;
    }
    for (at, playback) in rows_of_playbacks {
        let label = state.output(&playback.output).map_or_else(
            || playback.output.to_string(),
            |output| config.output_label(output).to_owned(),
        );
        add_once(&mut apps[at].outputs, label);
    }
    apps.into_iter()
        .map(|app| {
            let mut parts = Vec::new();
            if !app.microphones.is_empty() {
                parts.push(format!("Microphone: {}", app.microphones.join(", ")));
            }
            if !app.outputs.is_empty() {
                parts.push(format!("Sound: {}", app.outputs.join(", ")));
            }
            AppRow {
                name: app.name,
                icon: app.icon,
                detail: parts.join(" · "),
                tag: if app.records_monitor {
                    AppTag::NotAMicrophone
                } else {
                    AppTag::Ok
                },
            }
        })
        .collect()
}

/// The alert at the top of the Now page: an app listens to a monitor, so
/// in a call the others hear a copy of an output, not the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// `In Firefox, they probably can't hear you`.
    pub title: String,
    /// What is wrong, and the microphone to use instead.
    pub text: String,
    /// The text of the button that switches the apps, and its fixes.
    pub switch: Option<(String, Vec<Fix>)>,
    /// The streams the alert is about, to ignore them.
    pub streams: Vec<StreamId>,
}

/// Joins names in a sentence: `A`, `A and B`, `A, B and C`.
fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The alert, when an app listens to a monitor, leaving out the streams
/// the user chose to ignore. `voice_heard` tells whether the microphone
/// to switch to heard a voice a moment ago.
pub fn alert(
    config: &Config,
    state: &AudioState,
    ignored: &[StreamId],
    voice_heard: bool,
) -> Option<Alert> {
    let mut apps: Vec<String> = Vec::new();
    let mut listens: Vec<String> = Vec::new();
    let mut fixes = Vec::new();
    let mut streams = Vec::new();
    for finding in diagnose(state) {
        let Problem::RecordsMonitor {
            stream,
            app,
            monitor,
        } = &finding.problem
        else {
            continue;
        };
        if ignored.contains(stream) {
            continue;
        }
        let monitor = state.input(monitor).map_or_else(
            || monitor.to_string(),
            |m| config.input_label(m).into_owned(),
        );
        let sentence = format!("{app} listens to \"{monitor}\"");
        if !listens.contains(&sentence) {
            listens.push(sentence);
        }
        if !apps.contains(app) {
            apps.push(app.clone());
        }
        streams.push(*stream);
        fixes.extend(finding.fix);
    }
    if streams.is_empty() {
        return None;
    }
    let not_mic = if listens.len() == 1 {
        "It is not a microphone."
    } else {
        "These are not microphones."
    };
    let mic = best_microphone(state).map(|mic| config.input_label(mic).into_owned());
    let mic_part = match (&mic, voice_heard) {
        (Some(mic), true) => format!(" Your microphone, \"{mic}\", hears your voice well."),
        (Some(mic), false) => format!(" Your microphone is \"{mic}\"."),
        (None, _) => " Plug in a headset or a microphone.".to_owned(),
    };
    let who = and_list(&apps);
    let switch = mic.filter(|_| !fixes.is_empty()).map(|mic| {
        let whom = if apps.len() == 1 {
            who.clone()
        } else {
            "them".to_owned()
        };
        (format!("Switch {whom} to {mic}"), fixes)
    });
    Some(Alert {
        title: format!("In {who}, they probably can't hear you"),
        text: format!("{}. {not_mic}{mic_part}", and_list(&listens)),
        switch,
        streams,
    })
}

/// A screen of the map, as in the design: the screens stand side by side
/// on a desk, from left to right, each as big as its share of the desktop.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenTile {
    /// Its identity, to show the mouse on it.
    pub id: deskcheck_core::display::ScreenId,
    /// Number, as in `deskcheck displays`.
    pub number: usize,
    /// Name to show.
    pub name: String,
    /// Color, as `#rrggbb`.
    pub color: String,
    /// `true` for the primary screen, drawn with a thicker border.
    pub primary: bool,
    /// Width, in pixels of the map.
    pub width: i32,
    /// Height, in pixels of the map.
    pub height: i32,
}

/// The color of a screen: its own, or one of the palette by number.
pub fn screen_color(
    config: &Config,
    id: &deskcheck_core::display::ScreenId,
    number: usize,
) -> Color {
    config
        .screen_color(id)
        .unwrap_or_else(|| SCREEN_COLORS[number.saturating_sub(1) % SCREEN_COLORS.len()])
}

/// Rounds a float size to whole pixels.
#[allow(
    clippy::cast_possible_truncation,
    reason = "sizes in a map of a few hundred pixels"
)]
fn px(value: f64) -> i32 {
    value.round() as i32
}

/// A screen on the desktop: number, screen, place, and size in logical
/// pixels.
type Desktop<'a> = (
    usize,
    &'a deskcheck_core::display::Screen,
    (i32, i32),
    f64,
    f64,
);

/// The screens that are on, from left to right, sized to fit in `width` ×
/// `height` pixels with `gap` pixels between them, keeping proportions.
pub fn screen_map(
    config: &Config,
    state: &DisplayState,
    width: i32,
    height: i32,
    gap: i32,
) -> Vec<ScreenTile> {
    let numbered = state.numbered();
    let mut boxes: Vec<Desktop> = numbered
        .iter()
        .filter_map(|&(number, screen)| {
            let placement = screen.placement?;
            let mode = screen.current_mode?;
            let scale = if placement.scale > 0.0 {
                placement.scale
            } else {
                1.0
            };
            Some((
                number,
                screen,
                (placement.x, placement.y),
                f64::from(mode.width) / scale,
                f64::from(mode.height) / scale,
            ))
        })
        .collect();
    if boxes.is_empty() {
        return Vec::new();
    }
    boxes.sort_by_key(|b| b.2);
    let gaps = f64::from(gap) * f64::from(i32::try_from(boxes.len() - 1).unwrap_or(0));
    let total_w: f64 = boxes.iter().map(|b| b.3).sum();
    let tallest = boxes.iter().map(|b| b.4).fold(0.0, f64::max);
    let factor = ((f64::from(width) - gaps) / total_w).min(f64::from(height) / tallest);
    boxes
        .into_iter()
        .map(|(number, screen, _, w, h)| ScreenTile {
            id: screen.id.clone(),
            number,
            name: config.screen_label(screen).to_owned(),
            color: screen_color(config, &screen.id, number).to_string(),
            primary: screen.is_primary(),
            width: px(w * factor),
            height: px(h * factor),
        })
        .collect()
}

/// One line about the layout: `Extended · Primary: 2`.
pub fn screens_line(state: &DisplayState) -> String {
    let Some(layout) = crate::screens_model::layout(state) else {
        return "No screen is on.".to_owned();
    };
    let primary = state
        .numbered()
        .iter()
        .find(|(_, screen)| screen.is_primary())
        .map(|(number, _)| *number);
    match (layout, primary) {
        (Layout::Single, _) | (_, None) => layout.word().to_owned(),
        (_, Some(number)) => format!("{} · Primary: {number}", layout.word()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{
        DeviceId, Input, InputKind, Output, Playback, Recording, StreamId, Volume,
    };
    use deskcheck_core::display::{Mode, Placement, Screen, ScreenId};

    fn app(name: &str) -> App {
        App {
            name: name.to_owned(),
            ..App::default()
        }
    }

    fn office_sound() -> AudioState {
        let headset = DeviceId::new("headset");
        let dell_monitor = DeviceId::new("dell.monitor");
        AudioState {
            outputs: vec![Output {
                id: headset.clone(),
                description: "USB Headset".to_owned(),
                hardware: Hardware::default(),
                volume: Volume::NORMAL,
                muted: false,
            }],
            inputs: vec![Input {
                id: dell_monitor.clone(),
                description: "Monitor of Dell".to_owned(),
                hardware: Hardware::default(),
                kind: InputKind::Monitor {
                    of: DeviceId::new("dell"),
                },
                volume: Volume::NORMAL,
                muted: false,
            }],
            playbacks: vec![
                Playback {
                    id: StreamId::new(1),
                    app: app("Firefox"),
                    media: None,
                    output: headset.clone(),
                    volume: Volume::NORMAL,
                    muted: false,
                },
                Playback {
                    id: StreamId::new(2),
                    app: app("Chrome"),
                    media: None,
                    output: headset.clone(),
                    volume: Volume::NORMAL,
                    muted: false,
                },
                Playback {
                    id: StreamId::new(3),
                    app: app("Firefox"),
                    media: None,
                    output: headset,
                    volume: Volume::NORMAL,
                    muted: false,
                },
            ],
            recordings: vec![
                Recording {
                    id: StreamId::new(4),
                    app: app("Firefox"),
                    media: None,
                    input: dell_monitor.clone(),
                    volume: Volume::NORMAL,
                    muted: false,
                    level_meter: false,
                },
                Recording {
                    id: StreamId::new(5),
                    app: app("GNOME Settings"),
                    media: None,
                    input: dell_monitor,
                    volume: Volume::NORMAL,
                    muted: false,
                    level_meter: true,
                },
            ],
            ..AudioState::default()
        }
    }

    #[test]
    fn the_alert_matches_the_design() {
        let mut state = office_sound();
        state.inputs.push(Input {
            id: DeviceId::new("headset.mic"),
            description: "Headset mic".to_owned(),
            hardware: Hardware::default(),
            kind: InputKind::Capture,
            volume: Volume::NORMAL,
            muted: false,
        });
        let alert = alert(&Config::default(), &state, &[], true).unwrap();
        assert_eq!(alert.title, "In Firefox, they probably can't hear you");
        assert_eq!(
            alert.text,
            "Firefox listens to \"Monitor of Dell\". It is not a microphone. \
             Your microphone, \"Headset mic\", hears your voice well."
        );
        let (label, fixes) = alert.switch.unwrap();
        assert_eq!(label, "Switch Firefox to Headset mic");
        assert_eq!(fixes.len(), 1);
        assert_eq!(alert.streams, [StreamId::new(4)]);
        // Ignored, it goes away; the level meter of GNOME Settings never counts.
        assert_eq!(
            super::alert(&Config::default(), &state, &[StreamId::new(4)], true),
            None
        );
        let quiet = super::alert(&Config::default(), &state, &[], false).unwrap();
        assert!(quiet.text.ends_with("Your microphone is \"Headset mic\"."));
    }

    #[test]
    fn without_a_microphone_the_alert_has_no_switch() {
        let alert = alert(&Config::default(), &office_sound(), &[], false).unwrap();
        assert_eq!(alert.switch, None);
        assert!(alert.text.ends_with("Plug in a headset or a microphone."));
        assert_eq!(
            and_list(&["A".to_owned(), "B".to_owned(), "C".to_owned()]),
            "A, B and C"
        );
    }

    #[test]
    fn the_apps_card_matches_the_design() {
        assert_eq!(
            app_rows(&Config::default(), &office_sound()),
            [
                AppRow {
                    name: "Firefox".to_owned(),
                    icon: "dc-browser-symbolic",
                    detail: "Microphone: Monitor of Dell · Sound: USB Headset".to_owned(),
                    tag: AppTag::NotAMicrophone,
                },
                AppRow {
                    name: "Chrome".to_owned(),
                    icon: "dc-app-symbolic",
                    detail: "Sound: USB Headset".to_owned(),
                    tag: AppTag::Ok,
                },
            ]
        );
    }

    #[test]
    fn devices_get_the_icons_of_the_design() {
        let headset = Hardware {
            form: Form::Headset,
            connection: Connection::Usb,
        };
        assert_eq!(device_icon(headset, false), "dc-headphones-symbolic");
        assert_eq!(device_icon(headset, true), "dc-headphones-symbolic");
        let built_in = Hardware {
            form: Form::Microphone,
            connection: Connection::BuiltIn,
        };
        assert_eq!(device_icon(built_in, true), "dc-laptop-symbolic");
        assert_eq!(
            device_icon(Hardware::default(), true),
            "dc-microphone-symbolic"
        );
        assert_eq!(
            device_icon(Hardware::default(), false),
            "dc-speaker-symbolic"
        );
    }

    #[test]
    fn browsers_get_a_globe() {
        assert_eq!(app_icon(&app("Firefox")), "dc-browser-symbolic");
        let by_binary = App {
            name: "Teams".to_owned(),
            binary: Some("firefox-bin".to_owned()),
            icon_name: None,
        };
        assert_eq!(app_icon(&by_binary), "dc-browser-symbolic");
        assert_eq!(app_icon(&app("MicTone")), "dc-app-symbolic");
    }

    fn screen(id: ScreenId, name: &str, x: i32, size: (u32, u32), primary: bool) -> Screen {
        Screen {
            id,
            connector: String::new(),
            display_name: Some(name.to_owned()),
            builtin: false,
            current_mode: Some(Mode {
                width: size.0,
                height: size.1,
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

    /// Godwin's home screens.
    fn home() -> DisplayState {
        DisplayState {
            screens: vec![
                screen(
                    ScreenId::new("LGD", "0x0764", ""),
                    "Built-in display",
                    0,
                    (1920, 1080),
                    true,
                ),
                screen(
                    ScreenId::new("AUS", "XG27ACS", "T2LMTF031523"),
                    "ASUS",
                    1920,
                    (2560, 1440),
                    false,
                ),
            ],
        }
    }

    #[test]
    fn the_screens_stand_side_by_side_in_proportion() {
        let tiles = screen_map(&Config::default(), &home(), 336, 72, 10);
        assert_eq!(tiles.len(), 2);
        let (laptop, asus) = (&tiles[0], &tiles[1]);
        assert_eq!((laptop.number, asus.number), (1, 2));
        assert!(laptop.primary && !asus.primary);
        // The tallest screen fills the height; the other keeps its share.
        assert_eq!(asus.height, 72);
        assert_eq!((asus.width, laptop.width, laptop.height), (128, 96, 54));
        assert!(laptop.width + 10 + asus.width <= 336);
        assert_eq!(
            (laptop.color.as_str(), asus.color.as_str()),
            ("#1a5fb4", "#b54708")
        );
    }

    #[test]
    fn many_screens_shrink_to_fit_the_width() {
        let mut wide = home();
        wide.screens[0].placement.as_mut().unwrap().x = 5120;
        let tiles = screen_map(&Config::default(), &wide, 200, 72, 10);
        // Ordered from left to right on the desktop.
        assert_eq!(
            (tiles[0].name.as_str(), tiles[1].name.as_str()),
            ("ASUS", "Built-in display")
        );
        assert!(tiles[0].width + 10 + tiles[1].width <= 200);
    }

    #[test]
    fn a_screen_keeps_its_own_color() {
        let mut config = Config::default();
        let asus = ScreenId::new("AUS", "XG27ACS", "T2LMTF031523");
        config.set_screen_color(&asus, Some("#26a269".parse().unwrap()));
        let tiles = screen_map(&config, &home(), 336, 72, 10);
        assert_eq!(tiles[1].color, "#26a269");
    }

    #[test]
    fn the_layout_line() {
        assert_eq!(screens_line(&home()), "Extended · Primary: 1");
        let mut mirrored = home();
        mirrored.screens[1].placement.as_mut().unwrap().x = 0;
        assert_eq!(screens_line(&mirrored), "Mirrored · Primary: 1");
        let mut one = home();
        one.screens.truncate(1);
        assert_eq!(screens_line(&one), "One screen");
        assert_eq!(screens_line(&DisplayState::default()), "No screen is on.");
        assert_eq!(
            screen_map(&Config::default(), &DisplayState::default(), 336, 72, 10),
            [] as [ScreenTile; 0]
        );
    }
}
