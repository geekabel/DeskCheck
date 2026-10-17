//! What the Now page shows, computed from the state. Pure, so it is tested
//! without a display.

use deskcheck_config::{Color, Config};
use deskcheck_core::audio::{AudioState, Hardware};
use deskcheck_core::display::DisplayState;

/// Colors given to screens without a color of their own, by number: the
/// blue and orange of the design, then other colors of the GNOME palette.
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
        red: 0x26,
        green: 0xa2,
        blue: 0x69,
    },
    Color {
        red: 0x91,
        green: 0x41,
        blue: 0xac,
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

/// The symbolic icon of a device, from its hardware.
pub fn device_icon(hardware: Hardware) -> String {
    format!("{}-symbolic", hardware.icon_name())
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
        microphones: Vec<String>,
        outputs: Vec<String>,
        records_monitor: bool,
    }
    let mut apps: Vec<Usage> = Vec::new();
    let mut usage = |name: &str| -> usize {
        if let Some(at) = apps.iter().position(|app| app.name == name) {
            return at;
        }
        apps.push(Usage {
            name: name.to_owned(),
            microphones: Vec::new(),
            outputs: Vec::new(),
            records_monitor: false,
        });
        apps.len() - 1
    };
    let mut rows_of_recordings = Vec::new();
    for recording in state.recordings.iter().filter(|r| !r.level_meter) {
        let at = usage(&recording.app.name);
        rows_of_recordings.push((at, recording));
    }
    let mut rows_of_playbacks = Vec::new();
    for playback in &state.playbacks {
        let at = usage(&playback.app.name);
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

/// A screen of the map: its place in the map, in pixels of the map.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenTile {
    /// Number, as in `deskcheck displays`.
    pub number: usize,
    /// Name to show.
    pub name: String,
    /// Color, as `#rrggbb`.
    pub color: String,
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: i32,
    /// Height.
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

/// Rounds a float position or size to whole pixels.
#[allow(
    clippy::cast_possible_truncation,
    reason = "positions in a map of a few hundred pixels"
)]
fn px(value: f64) -> i32 {
    value.round() as i32
}

/// Places the screens that are on in a map of `width` × `height` pixels,
/// with a margin, keeping their real layout and proportions.
pub fn screen_map(
    config: &Config,
    state: &DisplayState,
    width: i32,
    height: i32,
) -> Vec<ScreenTile> {
    const MARGIN: f64 = 8.0;
    let numbered = state.numbered();
    // The size of each screen on the desktop, in logical pixels.
    let boxes: Vec<(usize, &deskcheck_core::display::Screen, f64, f64, f64, f64)> = numbered
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
                f64::from(placement.x),
                f64::from(placement.y),
                f64::from(mode.width) / scale,
                f64::from(mode.height) / scale,
            ))
        })
        .collect();
    let Some(left) = boxes.iter().map(|b| b.2).reduce(f64::min) else {
        return Vec::new();
    };
    let top = boxes.iter().map(|b| b.3).fold(f64::INFINITY, f64::min);
    let right = boxes
        .iter()
        .map(|b| b.2 + b.4)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = boxes
        .iter()
        .map(|b| b.3 + b.5)
        .fold(f64::NEG_INFINITY, f64::max);
    let room_w = f64::from(width) - 2.0 * MARGIN;
    let room_h = f64::from(height) - 2.0 * MARGIN;
    let factor = (room_w / (right - left)).min(room_h / (bottom - top));
    // Centered in the map.
    let offset_x = MARGIN + (room_w - (right - left) * factor) / 2.0;
    let offset_y = MARGIN + (room_h - (bottom - top) * factor) / 2.0;
    boxes
        .into_iter()
        .map(|(number, screen, x, y, w, h)| ScreenTile {
            number,
            name: config.screen_label(screen).to_owned(),
            color: screen_color(config, &screen.id, number).to_string(),
            x: px(offset_x + (x - left) * factor),
            y: px(offset_y + (y - top) * factor),
            width: px(w * factor),
            height: px(h * factor),
        })
        .collect()
}

/// One line about the layout: `Extended · Primary: 2`.
pub fn screens_line(state: &DisplayState) -> String {
    let numbered = state.numbered();
    let primary = numbered
        .iter()
        .find(|(_, screen)| screen.is_primary())
        .map(|(number, _)| *number);
    let mut places: Vec<_> = numbered
        .iter()
        .filter_map(|(_, screen)| screen.placement.map(|p| (p.x, p.y)))
        .collect();
    let count = places.len();
    places.sort_unstable();
    places.dedup();
    let layout = match (count, places.len()) {
        (0, _) => return "No screen is on.".to_owned(),
        (1, _) => "One screen",
        (_, 1) => "Mirrored",
        _ => "Extended",
    };
    match primary {
        Some(number) if count > 1 => format!("{layout} · Primary: {number}"),
        _ => layout.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{
        App, Connection, DeviceId, Form, Input, InputKind, Output, Playback, Recording, StreamId,
        Volume,
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
    fn the_apps_card_matches_the_design() {
        assert_eq!(
            app_rows(&Config::default(), &office_sound()),
            [
                AppRow {
                    name: "Firefox".to_owned(),
                    detail: "Microphone: Monitor of Dell · Sound: USB Headset".to_owned(),
                    tag: AppTag::NotAMicrophone,
                },
                AppRow {
                    name: "Chrome".to_owned(),
                    detail: "Sound: USB Headset".to_owned(),
                    tag: AppTag::Ok,
                },
            ]
        );
    }

    #[test]
    fn device_icons_are_symbolic() {
        let headset = Hardware {
            form: Form::Headset,
            connection: Connection::Usb,
        };
        assert_eq!(device_icon(headset), "audio-headset-symbolic");
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
    fn the_map_keeps_the_layout_and_the_proportions() {
        let tiles = screen_map(&Config::default(), &home(), 360, 100);
        assert_eq!(tiles.len(), 2);
        let (laptop, asus) = (&tiles[0], &tiles[1]);
        assert_eq!((laptop.number, asus.number), (1, 2));
        // Side by side, the laptop on the left, both at the top.
        assert_eq!(laptop.y, asus.y);
        assert!(laptop.x + laptop.width <= asus.x + 1);
        // The ASUS is bigger, in proportion: 2560 / 1920.
        let ratio = f64::from(asus.width) / f64::from(laptop.width);
        assert!((ratio - 2560.0 / 1920.0).abs() < 0.05, "{ratio}");
        // Inside the map.
        assert!(asus.x + asus.width <= 360 - 7);
        assert!(asus.y + asus.height <= 100 - 7);
        assert_eq!(
            (laptop.color.as_str(), asus.color.as_str()),
            ("#1a5fb4", "#b54708")
        );
    }

    #[test]
    fn a_screen_keeps_its_own_color() {
        let mut config = Config::default();
        let asus = ScreenId::new("AUS", "XG27ACS", "T2LMTF031523");
        config.set_screen_color(&asus, Some("#26a269".parse().unwrap()));
        let tiles = screen_map(&config, &home(), 360, 100);
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
            screen_map(&Config::default(), &DisplayState::default(), 360, 100),
            [] as [ScreenTile; 0]
        );
    }
}
