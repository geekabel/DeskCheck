//! What the Screens page shows, computed from the state. Pure, so it is
//! tested without a display.

use deskcheck_config::{Color, Config};
use deskcheck_core::audio::{AudioState, Connection, Form, Output};
use deskcheck_core::display::{DisplayState, Mode, Screen, ScreenId};

use crate::now_model::screen_color;

/// The colors offered for a screen, as in the design: blue, orange, teal,
/// violet.
pub const SWATCHES: [(Color, &str); 4] = [
    (
        Color {
            red: 0x1a,
            green: 0x5f,
            blue: 0xb4,
        },
        "Blue",
    ),
    (
        Color {
            red: 0xb5,
            green: 0x47,
            blue: 0x08,
        },
        "Orange",
    ),
    (
        Color {
            red: 0x0f,
            green: 0x76,
            blue: 0x6e,
        },
        "Teal",
    ),
    (
        Color {
            red: 0x6d,
            green: 0x28,
            blue: 0xd9,
        },
        "Violet",
    ),
];

/// How the screens share the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Each screen shows its own part of the desktop.
    Extended,
    /// The screens show the same picture.
    Mirrored,
    /// Only one screen is on.
    Single,
}

impl Layout {
    /// The three layouts, in the order of the tabs of the design.
    pub const ALL: [Self; 3] = [Self::Extended, Self::Mirrored, Self::Single];

    /// The text of its tab.
    pub fn tab(self) -> &'static str {
        match self {
            Self::Extended => "Extend",
            Self::Mirrored => "Mirror",
            Self::Single => "One screen",
        }
    }

    /// The word used in a sentence.
    pub fn word(self) -> &'static str {
        match self {
            Self::Extended => "Extended",
            Self::Mirrored => "Mirrored",
            Self::Single => "One screen",
        }
    }
}

/// The layout of the screens that are on, or `None` when none is on.
pub fn layout(state: &DisplayState) -> Option<Layout> {
    let mut places: Vec<(i32, i32)> = state
        .screens
        .iter()
        .filter_map(|screen| screen.placement.map(|p| (p.x, p.y)))
        .collect();
    let count = places.len();
    places.sort_unstable();
    places.dedup();
    match (count, places.len()) {
        (0, _) => None,
        (1, _) => Some(Layout::Single),
        (_, 1) => Some(Layout::Mirrored),
        _ => Some(Layout::Extended),
    }
}

/// `1920 × 1080`.
pub fn mode_label(mode: Mode) -> String {
    format!("{} × {}", mode.width, mode.height)
}

/// `125 %`.
pub fn scale_label(scale: f64) -> String {
    format!("{:.0} %", scale * 100.0)
}

/// The technical line of a screen: `DP-1 · DELL U2419H · 1920 × 1080 · 100 %`.
/// A built-in screen has no product name worth showing.
pub fn details_line(screen: &Screen) -> String {
    let mut parts = vec![screen.connector.clone()];
    if !screen.builtin {
        parts.push(screen.id.product.clone());
    }
    if let Some(mode) = screen.current_mode {
        parts.push(mode_label(mode));
    }
    if let Some(placement) = screen.placement {
        parts.push(scale_label(placement.scale));
    }
    parts.retain(|part| !part.is_empty());
    parts.join(" · ")
}

/// How big a screen is drawn, which picks the size of its texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileSize {
    /// Room for everything.
    Large,
    /// Room for the number, the name, the line and the tags.
    Medium,
    /// Only the number and the name.
    Small,
}

impl TileSize {
    fn of(height: i32) -> Self {
        if height >= 150 {
            Self::Large
        } else if height >= 90 {
            Self::Medium
        } else {
            Self::Small
        }
    }
}

/// A screen of the map of the Screens page, at its real place.
#[derive(Debug, Clone, PartialEq)]
pub struct MapTile {
    /// Its identity, to select it.
    pub id: ScreenId,
    /// Number, as everywhere in DeskCheck.
    pub number: usize,
    /// Name to show.
    pub name: String,
    /// The technical line.
    pub details: String,
    /// Color, as `#rrggbb`.
    pub color: String,
    /// The primary screen, with the top bar.
    pub primary: bool,
    /// The screen of the laptop.
    pub builtin: bool,
    /// Left edge, in pixels of the map.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
    /// How much it can show.
    pub size: TileSize,
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
/// keeping their real layout and proportions, centered, with `gap` pixels
/// taken off each side of each screen so two neighbours do not touch.
pub fn layout_map(
    config: &Config,
    state: &DisplayState,
    width: i32,
    height: i32,
    gap: i32,
) -> Vec<MapTile> {
    let boxes: Vec<(usize, &Screen, f64, f64, f64, f64)> = state
        .numbered()
        .into_iter()
        .filter_map(|(number, screen)| {
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
    let (room_w, room_h) = (f64::from(width), f64::from(height));
    let factor = (room_w / (right - left)).min(room_h / (bottom - top));
    let offset_x = (room_w - (right - left) * factor) / 2.0;
    let offset_y = (room_h - (bottom - top) * factor) / 2.0;
    let half_gap = f64::from(gap) / 2.0;
    boxes
        .into_iter()
        .map(|(number, screen, x, y, w, h)| {
            let height = px(h * factor - 2.0 * half_gap);
            MapTile {
                id: screen.id.clone(),
                number,
                name: config.screen_label(screen).to_owned(),
                details: details_line(screen),
                color: screen_color(config, &screen.id, number).to_string(),
                primary: screen.is_primary(),
                builtin: screen.builtin,
                x: px(offset_x + (x - left) * factor + half_gap),
                y: px(offset_y + (y - top) * factor + half_gap),
                width: px(w * factor - 2.0 * half_gap),
                height,
                size: TileSize::of(height),
            }
        })
        .collect()
}

/// The screen to show in the settings: the one chosen, if still on, else
/// the primary one, else the first.
pub fn selected<'a>(
    state: &'a DisplayState,
    chosen: Option<&ScreenId>,
) -> Option<(usize, &'a Screen)> {
    let numbered = state.numbered();
    chosen
        .and_then(|id| numbered.iter().find(|(_, screen)| &screen.id == id))
        .or_else(|| numbered.iter().find(|(_, screen)| screen.is_primary()))
        .or_else(|| numbered.first())
        .copied()
}

/// The name of a maker, from its three letter code (the PNP id of the
/// screen), with the list of the system when it is there
/// (`/usr/share/hwdata/pnp.ids`, lines `DEL<tab>Dell Inc.`).
pub fn maker(code: &str, pnp_ids: Option<&str>) -> String {
    const KNOWN: [(&str, &str); 18] = [
        ("ACR", "Acer"),
        ("AOC", "AOC"),
        ("APP", "Apple"),
        ("AUO", "AU Optronics"),
        ("AUS", "ASUS"),
        ("BNQ", "BenQ"),
        ("BOE", "BOE"),
        ("CMN", "Chimei Innolux"),
        ("DEL", "Dell"),
        ("ENC", "EIZO"),
        ("GSM", "LG Electronics"),
        ("HWP", "HP"),
        ("IVO", "InfoVision"),
        ("LEN", "Lenovo"),
        ("LGD", "LG Display"),
        ("PHL", "Philips"),
        ("SAM", "Samsung"),
        ("SHP", "Sharp"),
    ];
    let from_list = pnp_ids.and_then(|list| {
        list.lines().find_map(|line| {
            let (id, name) = line.split_once('\t')?;
            (id == code).then(|| name.trim().to_owned())
        })
    });
    from_list
        .or_else(|| {
            KNOWN
                .iter()
                .find(|(id, _)| *id == code)
                .map(|(_, name)| (*name).to_owned())
        })
        .unwrap_or_else(|| code.to_owned())
}

/// The serial number to show.
pub fn serial_label(id: &ScreenId) -> String {
    if id.has_serial() {
        id.serial.clone()
    } else {
        "None (common for laptop panels)".to_owned()
    }
}

fn words(text: &str) -> String {
    text.to_lowercase()
}

/// The speakers of a screen: an output sent through HDMI or DisplayPort
/// whose name holds the model of the screen. When only one external screen
/// is on and only one such output exists, that output.
pub fn screen_speakers<'a>(
    sound: &'a AudioState,
    displays: &DisplayState,
    screen: &Screen,
) -> Option<&'a Output> {
    if screen.builtin {
        return None;
    }
    let through_screen: Vec<&Output> = sound
        .outputs
        .iter()
        .filter(|output| {
            output.hardware.form == Form::Screen || output.hardware.connection == Connection::Hdmi
        })
        .collect();
    let product = words(&screen.id.product);
    let named = through_screen.iter().find(|output| {
        let description = words(&output.description);
        (!product.is_empty() && description.contains(&product))
            || screen
                .display_name
                .as_deref()
                .is_some_and(|name| !name.is_empty() && description.contains(&words(name)))
    });
    if let Some(output) = named {
        return Some(output);
    }
    let external_on = displays
        .screens
        .iter()
        .filter(|s| s.is_enabled() && !s.builtin)
        .count();
    match (external_on, through_screen.as_slice()) {
        (1, [only]) => Some(only),
        _ => None,
    }
}

/// A monitor as GTK sees it, to find which screen it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenMonitor {
    /// Its connector, like `DP-1`, when GTK knows it.
    pub connector: Option<String>,
    /// Its left edge on the desktop.
    pub x: i32,
    /// Its top edge on the desktop.
    pub y: i32,
}

/// The number of the screen each monitor of GTK shows, or `None` when it
/// matches none. The connector decides first (GNOME and GTK use the same
/// names), then the place on the desktop.
pub fn match_monitors(state: &DisplayState, seen: &[SeenMonitor]) -> Vec<Option<usize>> {
    let numbered = state.numbered();
    let mut found: Vec<Option<usize>> = seen
        .iter()
        .map(|monitor| {
            let connector = monitor.connector.as_deref()?;
            numbered
                .iter()
                .find(|(_, screen)| screen.connector == connector)
                .map(|(number, _)| *number)
        })
        .collect();
    // Then by place, each screen once: a monitor GTK adds for the whole
    // desktop must not take the place of a real one.
    for (at, monitor) in seen.iter().enumerate() {
        if found[at].is_some() {
            continue;
        }
        found[at] = numbered
            .iter()
            .find(|(number, screen)| {
                !found.contains(&Some(*number))
                    && screen
                        .placement
                        .is_some_and(|p| (p.x, p.y) == (monitor.x, monitor.y))
            })
            .map(|(number, _)| *number);
    }
    found
}

/// The small line on a screen being identified: `eDP-1 · 1920 × 1080`.
pub fn identify_line(screen: &Screen) -> String {
    match screen.current_mode {
        Some(mode) => format!("{} · {}", screen.connector, mode_label(mode)),
        None => screen.connector.clone(),
    }
}

/// The line at the bottom of a screen being identified.
pub fn countdown(seconds: u32) -> String {
    format!("The numbers disappear in {seconds} s · Esc to close")
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{DeviceId, Hardware, Volume};
    use deskcheck_core::display::Placement;

    fn screen(id: ScreenId, connector: &str, x: i32, size: (u32, u32), scale: f64) -> Screen {
        Screen {
            id,
            connector: connector.to_owned(),
            display_name: None,
            builtin: connector.starts_with("eDP"),
            current_mode: Some(Mode {
                width: size.0,
                height: size.1,
                refresh_rate: 60.0,
            }),
            placement: Some(Placement {
                x,
                y: 0,
                scale,
                primary: !connector.starts_with("eDP"),
            }),
        }
    }

    /// The office of the design: the laptop at 125 % on the left, the Dell
    /// screen on the right, primary.
    fn office() -> DisplayState {
        DisplayState {
            screens: vec![
                screen(
                    ScreenId::new("BOE", "0x0a1c", "0x00000000"),
                    "eDP-1",
                    0,
                    (1920, 1080),
                    1.25,
                ),
                screen(
                    ScreenId::new("DEL", "DELL U2419H", "ABC1234"),
                    "DP-1",
                    1536,
                    (1920, 1080),
                    1.0,
                ),
            ],
        }
    }

    #[test]
    fn the_lines_of_a_screen_match_the_design() {
        let state = office();
        assert_eq!(
            details_line(&state.screens[0]),
            "eDP-1 · 1920 × 1080 · 125 %"
        );
        assert_eq!(
            details_line(&state.screens[1]),
            "DP-1 · DELL U2419H · 1920 × 1080 · 100 %"
        );
        assert_eq!(layout(&state), Some(Layout::Extended));
        assert_eq!(layout(&DisplayState::default()), None);
    }

    #[test]
    fn the_map_keeps_the_real_layout() {
        let tiles = layout_map(&Config::default(), &office(), 800, 200, 20);
        let (laptop, dell) = (&tiles[0], &tiles[1]);
        // The laptop at 125 % is smaller on the desktop than the Dell.
        assert!(laptop.width < dell.width && laptop.height < dell.height);
        // Side by side, with the gap between them.
        assert_eq!(laptop.x + laptop.width + 20, dell.x);
        assert_eq!(laptop.y, dell.y);
        assert!(dell.primary && !laptop.primary && laptop.builtin);
        assert_eq!(dell.height, 180);
        assert_eq!(dell.size, TileSize::Large);
        // Centered.
        assert_eq!(laptop.x - 10, 800 - (dell.x + dell.width + 10));
    }

    #[test]
    fn the_settings_show_the_chosen_screen_or_the_primary_one() {
        let state = office();
        let laptop = state.screens[0].id.clone();
        assert_eq!(selected(&state, Some(&laptop)).map(|(n, _)| n), Some(1));
        assert_eq!(selected(&state, None).map(|(n, _)| n), Some(2));
        let gone = ScreenId::new("XXX", "gone", "");
        assert_eq!(selected(&state, Some(&gone)).map(|(n, _)| n), Some(2));
        assert_eq!(selected(&DisplayState::default(), None), None);
    }

    #[test]
    fn monitors_are_matched_by_connector_then_place() {
        let state = office();
        let seen = [
            SeenMonitor {
                connector: Some("DP-1".to_owned()),
                x: 9999,
                y: 0,
            },
            SeenMonitor {
                connector: None,
                x: 0,
                y: 0,
            },
            SeenMonitor {
                connector: Some("HDMI-9".to_owned()),
                x: 50,
                y: 50,
            },
        ];
        assert_eq!(match_monitors(&state, &seen), [Some(2), Some(1), None]);
        // A screen found by its connector is not taken again by its place.
        let twice = [
            SeenMonitor {
                connector: None,
                x: 1536,
                y: 0,
            },
            SeenMonitor {
                connector: Some("DP-1".to_owned()),
                x: 1536,
                y: 0,
            },
        ];
        assert_eq!(match_monitors(&state, &twice), [None, Some(2)]);
        assert_eq!(identify_line(&state.screens[0]), "eDP-1 · 1920 × 1080");
        assert_eq!(countdown(5), "The numbers disappear in 5 s · Esc to close");
    }

    #[test]
    fn makers_are_named() {
        let list = "DEL\tDell Inc.\nAUS\tAsustek Computer Inc\n";
        assert_eq!(maker("DEL", Some(list)), "Dell Inc.");
        assert_eq!(maker("DEL", None), "Dell");
        assert_eq!(maker("ZZZ", Some(list)), "ZZZ");
        assert_eq!(
            serial_label(&ScreenId::new("DEL", "U2419H", "ABC1234")),
            "ABC1234"
        );
        assert!(serial_label(&ScreenId::new("BOE", "x", "0x00000000")).starts_with("None"));
    }

    fn output(id: &str, description: &str, connection: Connection) -> Output {
        Output {
            id: DeviceId::new(id),
            description: description.to_owned(),
            hardware: Hardware {
                form: Form::Unknown,
                connection,
            },
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    #[test]
    fn the_speakers_of_a_screen_are_found() {
        let displays = office();
        let dell = &displays.screens[1];
        let mut sound = AudioState {
            outputs: vec![
                output("laptop", "Built-in Audio", Connection::BuiltIn),
                output("hdmi", "HDMI / DisplayPort 2", Connection::Hdmi),
            ],
            ..AudioState::default()
        };
        // One external screen, one HDMI output: that one.
        assert_eq!(
            screen_speakers(&sound, &displays, dell).map(|o| o.id.as_str()),
            Some("hdmi")
        );
        // The laptop screen has no speakers of its own here.
        assert_eq!(
            screen_speakers(&sound, &displays, &displays.screens[0]),
            None
        );
        // Two HDMI outputs: only the one named after the screen.
        sound
            .outputs
            .push(output("hdmi3", "DELL U2419H", Connection::Hdmi));
        assert_eq!(
            screen_speakers(&sound, &displays, dell).map(|o| o.id.as_str()),
            Some("hdmi3")
        );
        sound.outputs.pop();
        sound
            .outputs
            .push(output("hdmi3", "HDMI 3", Connection::Hdmi));
        assert_eq!(screen_speakers(&sound, &displays, dell), None);
    }
}
