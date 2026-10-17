//! Text report for `deskcheck displays`: which screen is which.
//!
//! [`render`] is a pure function of the screen state and the settings, so
//! the report is tested without a GNOME session. Screens show the name the
//! user gave them; `--verbose` also shows the name from GNOME.

use std::fmt::Write as _;

use deskcheck_config::Config;
use deskcheck_core::display::{DisplayState, Screen};

/// Options of the report.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Also show the identity, the connector and the position.
    pub verbose: bool,
}

/// One row of the table, before alignment.
struct Row {
    number: usize,
    name: String,
    mode: String,
    scale: String,
    tags: String,
}

fn row(config: &Config, number: usize, screen: &Screen) -> Row {
    let mut tags = Vec::new();
    if screen.is_primary() {
        tags.push("primary");
    }
    if screen.builtin {
        tags.push("built-in");
    }
    Row {
        number,
        name: config.screen_label(screen).to_owned(),
        mode: screen
            .current_mode
            .map_or_else(|| "unknown mode".to_owned(), |mode| mode.to_string()),
        scale: screen
            .placement
            .map(|placement| format!("{:.0}%", placement.scale * 100.0))
            .unwrap_or_default(),
        tags: tags.join(", "),
    }
}

/// Second line of a screen in verbose mode: the name from GNOME when the
/// user gave another one, then identity, connector and position.
fn details(config: &Config, screen: &Screen) -> String {
    let mut line = String::new();
    if config.screen_name(&screen.id).is_some() {
        let _ = write!(line, "{}, ", screen.name());
    }
    let _ = write!(line, "{} on {}", screen.id, screen.connector);
    if let Some(placement) = screen.placement {
        let _ = write!(line, ", at ({}, {})", placement.x, placement.y);
    }
    line
}

fn width(rows: &[Row], column: impl Fn(&Row) -> &str) -> usize {
    rows.iter()
        .map(|row| column(row).chars().count())
        .max()
        .unwrap_or(0)
}

fn write_screens_on(out: &mut String, state: &DisplayState, config: &Config, options: Options) {
    let numbered = state.numbered();
    if numbered.is_empty() {
        let _ = writeln!(out, "No screen is on.");
        return;
    }
    let rows: Vec<Row> = numbered
        .iter()
        .map(|&(number, screen)| row(config, number, screen))
        .collect();
    let name_width = width(&rows, |row| &row.name);
    let mode_width = width(&rows, |row| &row.mode);
    let scale_width = width(&rows, |row| &row.scale);

    let _ = writeln!(out, "Screens on, from left to right");
    for (row, (_, screen)) in rows.iter().zip(&numbered) {
        let line = format!(
            "  {:>2}  {:<name_width$}  {:<mode_width$}  {:>scale_width$}  {}",
            row.number, row.name, row.mode, row.scale, row.tags
        );
        let _ = writeln!(out, "{}", line.trim_end());
        if options.verbose {
            let _ = writeln!(out, "      {}", details(config, screen));
        }
    }
}

fn write_screens_off(out: &mut String, state: &DisplayState, config: &Config, options: Options) {
    let off: Vec<&Screen> = state.screens.iter().filter(|s| !s.is_enabled()).collect();
    if off.is_empty() {
        return;
    }
    let _ = writeln!(out, "\nConnected but off");
    for screen in off {
        if options.verbose {
            let _ = writeln!(out, "  {}", config.screen_label(screen));
            let _ = writeln!(out, "      {}", details(config, screen));
        } else {
            let _ = writeln!(
                out,
                "  {}  ({})",
                config.screen_label(screen),
                screen.connector
            );
        }
    }
}

/// Builds the whole report.
pub fn render(state: &DisplayState, config: &Config, options: Options) -> String {
    let mut out = String::new();
    write_screens_on(&mut out, state, config, options);
    write_screens_off(&mut out, state, config, options);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::display::{Mode, Placement, ScreenId};

    fn screen(
        id: ScreenId,
        connector: &str,
        display_name: &str,
        builtin: bool,
        mode: (u32, u32, f64),
        placement: (i32, i32, f64, bool),
    ) -> Screen {
        Screen {
            id,
            connector: connector.to_owned(),
            display_name: Some(display_name.to_owned()),
            builtin,
            current_mode: Some(Mode {
                width: mode.0,
                height: mode.1,
                refresh_rate: mode.2,
            }),
            placement: Some(Placement {
                x: placement.0,
                y: placement.1,
                scale: placement.2,
                primary: placement.3,
            }),
        }
    }

    /// Godwin's screens at home, as `display_dump` read them.
    fn home() -> DisplayState {
        DisplayState {
            screens: vec![
                screen(
                    ScreenId::new("LGD", "0x0764", "0x00000000"),
                    "eDP-1",
                    "Built-in display",
                    true,
                    (1920, 1080, 60.0),
                    (0, 0, 1.0, true),
                ),
                screen(
                    ScreenId::new("AUS", "XG27ACS", "T2LMTF031523"),
                    "HDMI-1",
                    "ASUSTek COMPUTER INC 27\"",
                    false,
                    (2560, 1440, 59.951),
                    (1920, 0, 1.0, false),
                ),
            ],
        }
    }

    /// An office setup: Dell screen on the right and primary, the laptop at
    /// 125 %, and a second Dell plugged in but switched off.
    fn office() -> DisplayState {
        let mut off = screen(
            ScreenId::new("DEL", "DELL P2422H", "XYZ987"),
            "HDMI-1",
            "Dell Inc. 24\"",
            false,
            (1920, 1080, 60.0),
            (0, 0, 1.0, false),
        );
        off.current_mode = None;
        off.placement = None;
        DisplayState {
            screens: vec![
                screen(
                    ScreenId::new("DEL", "DELL U2419H", "ABC1234"),
                    "DP-1",
                    "Dell Inc. 24\"",
                    false,
                    (1920, 1080, 60.0),
                    (1536, 0, 1.0, true),
                ),
                screen(
                    ScreenId::new("BOE", "0x0a1c", "0x00000000"),
                    "eDP-1",
                    "Built-in display",
                    true,
                    (1920, 1080, 60.01),
                    (0, 0, 1.25, false),
                ),
                off,
            ],
        }
    }

    #[test]
    fn home_screens() {
        let expected = "\
Screens on, from left to right
   1  Built-in display          1920 x 1080 @ 60 Hz  100%  primary, built-in
   2  ASUSTek COMPUTER INC 27\"  2560 x 1440 @ 60 Hz  100%
";
        assert_eq!(
            render(&home(), &Config::default(), Options::default()),
            expected
        );
    }

    #[test]
    fn home_screens_verbose() {
        let expected = "\
Screens on, from left to right
   1  Built-in display          1920 x 1080 @ 60 Hz  100%  primary, built-in
      0x0764 (LGD) on eDP-1, at (0, 0)
   2  ASUSTek COMPUTER INC 27\"  2560 x 1440 @ 60 Hz  100%
      XG27ACS (AUS, T2LMTF031523) on HDMI-1, at (1920, 0)
";
        assert_eq!(
            render(&home(), &Config::default(), Options { verbose: true }),
            expected
        );
    }

    #[test]
    fn office_screens_with_one_off() {
        let expected = "\
Screens on, from left to right
   1  Built-in display  1920 x 1080 @ 60 Hz  125%  built-in
   2  Dell Inc. 24\"     1920 x 1080 @ 60 Hz  100%  primary

Connected but off
  Dell Inc. 24\"  (HDMI-1)
";
        assert_eq!(
            render(&office(), &Config::default(), Options::default()),
            expected
        );
    }

    #[test]
    fn office_screens_verbose() {
        let report = render(&office(), &Config::default(), Options { verbose: true });
        assert!(report.contains("      DELL U2419H (DEL, ABC1234) on DP-1, at (1536, 0)\n"));
        assert!(report.ends_with(
            "Connected but off\n  Dell Inc. 24\"\n      DELL P2422H (DEL, XYZ987) on HDMI-1\n"
        ));
    }

    #[test]
    fn no_screen() {
        assert_eq!(
            render(
                &DisplayState::default(),
                &Config::default(),
                Options::default()
            ),
            "No screen is on.\n"
        );
    }

    #[test]
    fn given_names_are_shown_and_gnome_names_kept_in_verbose() {
        let mut config = Config::default();
        config.set_screen_name(
            &ScreenId::new("AUS", "XG27ACS", "T2LMTF031523"),
            Some("Big screen".to_owned()),
        );
        let expected = "\
Screens on, from left to right
   1  Built-in display  1920 x 1080 @ 60 Hz  100%  primary, built-in
      0x0764 (LGD) on eDP-1, at (0, 0)
   2  Big screen        2560 x 1440 @ 60 Hz  100%
      ASUSTek COMPUTER INC 27\", XG27ACS (AUS, T2LMTF031523) on HDMI-1, at (1920, 0)
";
        assert_eq!(
            render(&home(), &config, Options { verbose: true }),
            expected
        );
    }
}
