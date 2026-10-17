//! Screens and how they are laid out.
//!
//! The fields follow `GetCurrentState` of the GNOME D-Bus interface
//! `org.gnome.Mutter.DisplayConfig`:
//!
//! - a [`Screen`] is a connected physical screen ("monitor" in Mutter);
//! - its [`ScreenId`] is the vendor, product and serial number read from the
//!   screen itself, so it stays the same on any cable or port;
//! - its [`Placement`] says where it sits on the desktop. A connected screen
//!   that is switched off has no placement.
//!
//! These types only hold data. Reading them from GNOME comes later.

use std::fmt;

/// Stable identity of a screen: vendor, product and serial number.
///
/// The connector name (`DP-1`, `HDMI-1`) is not part of the identity,
/// because it changes when the cable moves to another port.
///
/// ```
/// use deskcheck_core::display::ScreenId;
///
/// let id = ScreenId::new("DEL", "DELL U2419H", "ABC1234");
/// assert_eq!(id.to_string(), "DELL U2419H (DEL, ABC1234)");
/// assert!(id.has_serial());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScreenId {
    /// Vendor name or code, for example `DEL`.
    pub vendor: String,
    /// Product name, for example `DELL U2419H`.
    pub product: String,
    /// Serial number. Some screens give an empty or zero serial.
    pub serial: String,
}

impl ScreenId {
    /// Creates an identity from the vendor, product and serial number.
    pub fn new(
        vendor: impl Into<String>,
        product: impl Into<String>,
        serial: impl Into<String>,
    ) -> Self {
        Self {
            vendor: vendor.into(),
            product: product.into(),
            serial: serial.into(),
        }
    }

    /// Returns `true` when the serial number can tell two screens of the same
    /// model apart. An empty serial, or one made only of zeros, cannot.
    #[must_use]
    pub fn has_serial(&self) -> bool {
        let serial = self.serial.trim();
        let digits = serial
            .strip_prefix("0x")
            .or_else(|| serial.strip_prefix("0X"))
            .unwrap_or(serial);
        !digits.is_empty() && digits.chars().any(|c| c != '0')
    }
}

impl fmt::Display for ScreenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.has_serial() {
            write!(f, "{} ({}, {})", self.product, self.vendor, self.serial)
        } else {
            write!(f, "{} ({})", self.product, self.vendor)
        }
    }
}

/// A display mode: size in physical pixels and refresh rate.
///
/// ```
/// use deskcheck_core::display::Mode;
///
/// let mode = Mode { width: 1920, height: 1080, refresh_rate: 59.951 };
/// assert_eq!(mode.to_string(), "1920 x 1080 @ 60 Hz");
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Refresh rate in hertz.
    pub refresh_rate: f64,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} x {} @ {:.0} Hz",
            self.width, self.height, self.refresh_rate
        )
    }
}

/// Where a screen sits on the desktop (a "logical monitor" in Mutter).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Left edge, in logical pixels.
    pub x: i32,
    /// Top edge, in logical pixels.
    pub y: i32,
    /// Scale, for example `1.0` or `1.25`.
    pub scale: f64,
    /// `true` for the primary screen (the one with the top bar).
    pub primary: bool,
}

/// A connected physical screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    /// Stable identity.
    pub id: ScreenId,
    /// Connector name, for example `DP-1`. It can change between sessions.
    pub connector: String,
    /// Human readable name from GNOME (`display-name`), when it gives one.
    pub display_name: Option<String>,
    /// `true` for a screen built into the computer, like a laptop panel.
    pub builtin: bool,
    /// The mode in use, if the screen is on.
    pub current_mode: Option<Mode>,
    /// Position on the desktop, or `None` when the screen is off.
    pub placement: Option<Placement>,
}

impl Screen {
    /// Returns `true` when the screen is on and part of the desktop.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.placement.is_some()
    }

    /// Returns `true` for the primary screen.
    #[must_use]
    pub fn is_primary(&self) -> bool {
        self.placement.is_some_and(|placement| placement.primary)
    }

    /// Name to show: the name GNOME gives, or else the product name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.id.product)
    }
}

/// Everything known about screens at one moment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayState {
    /// All connected screens, on or off.
    pub screens: Vec<Screen>,
}

impl DisplayState {
    /// Finds a screen by its identity.
    #[must_use]
    pub fn screen(&self, id: &ScreenId) -> Option<&Screen> {
        self.screens.iter().find(|screen| &screen.id == id)
    }

    /// Returns the primary screen, if one is on.
    #[must_use]
    pub fn primary(&self) -> Option<&Screen> {
        self.screens.iter().find(|screen| screen.is_primary())
    }

    /// Returns the built-in screen, if the computer has one.
    #[must_use]
    pub fn builtin(&self) -> Option<&Screen> {
        self.screens.iter().find(|screen| screen.builtin)
    }

    /// Screens that are on, from left to right, then from top to bottom.
    #[must_use]
    pub fn enabled(&self) -> Vec<&Screen> {
        let mut screens: Vec<&Screen> = self.screens.iter().filter(|s| s.is_enabled()).collect();
        screens.sort_by_key(|screen| screen.placement.map(|p| (p.x, p.y)));
        screens
    }

    /// Screens that are on, with their number: 1 for the leftmost one.
    ///
    /// Every part of DeskCheck shows the same numbers, so a number on the
    /// screen matches the number in the list.
    #[must_use]
    pub fn numbered(&self) -> Vec<(usize, &Screen)> {
        self.enabled()
            .into_iter()
            .enumerate()
            .map(|(index, screen)| (index + 1, screen))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laptop() -> Screen {
        Screen {
            id: ScreenId::new("BOE", "0x0a1c", "0x00000000"),
            connector: "eDP-1".to_owned(),
            display_name: Some("Built-in display".to_owned()),
            builtin: true,
            current_mode: Some(Mode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
            }),
            placement: Some(Placement {
                x: 0,
                y: 0,
                scale: 1.25,
                primary: false,
            }),
        }
    }

    fn office_screen() -> Screen {
        Screen {
            id: ScreenId::new("DEL", "DELL U2419H", "ABC1234"),
            connector: "DP-1".to_owned(),
            display_name: Some("Dell 24\"".to_owned()),
            builtin: false,
            current_mode: Some(Mode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
            }),
            placement: Some(Placement {
                x: 1536,
                y: 0,
                scale: 1.0,
                primary: true,
            }),
        }
    }

    fn office() -> DisplayState {
        DisplayState {
            screens: vec![office_screen(), laptop()],
        }
    }

    #[test]
    fn identity_ignores_the_connector() {
        let mut moved = office_screen();
        moved.connector = "HDMI-1".to_owned();
        assert_eq!(moved.id, office_screen().id);
        assert!(office().screen(&moved.id).is_some());
    }

    #[test]
    fn serial_detection() {
        assert!(ScreenId::new("DEL", "U2419H", "ABC1234").has_serial());
        assert!(ScreenId::new("DEL", "U2419H", "0x0001a2b3").has_serial());
        assert!(!ScreenId::new("BOE", "0x0a1c", "0x00000000").has_serial());
        assert!(!ScreenId::new("BOE", "0x0a1c", "").has_serial());
        assert!(!ScreenId::new("BOE", "0x0a1c", "  ").has_serial());
    }

    #[test]
    fn screen_id_display_skips_a_missing_serial() {
        assert_eq!(laptop().id.to_string(), "0x0a1c (BOE)");
    }

    #[test]
    fn primary_and_builtin_are_found() {
        let state = office();
        assert_eq!(state.primary().unwrap().connector, "DP-1");
        assert_eq!(state.builtin().unwrap().connector, "eDP-1");
    }

    #[test]
    fn a_screen_without_placement_is_off() {
        let mut off = office_screen();
        off.placement = None;
        off.current_mode = None;
        assert!(!off.is_enabled());
        assert!(!off.is_primary());

        let state = DisplayState {
            screens: vec![off, laptop()],
        };
        assert!(state.primary().is_none());
        assert_eq!(state.enabled().len(), 1);
    }

    #[test]
    fn enabled_screens_go_from_left_to_right() {
        let state = office();
        let connectors: Vec<&str> = state
            .enabled()
            .iter()
            .map(|s| s.connector.as_str())
            .collect();
        assert_eq!(connectors, ["eDP-1", "DP-1"]);
    }

    #[test]
    fn name_falls_back_to_the_product() {
        let mut screen = office_screen();
        assert_eq!(screen.name(), "Dell 24\"");
        screen.display_name = None;
        assert_eq!(screen.name(), "DELL U2419H");
        screen.display_name = Some("  ".to_owned());
        assert_eq!(screen.name(), "DELL U2419H");
    }

    #[test]
    fn numbers_start_at_one_from_the_left() {
        let state = office();
        let numbered: Vec<(usize, &str)> = state
            .numbered()
            .into_iter()
            .map(|(number, screen)| (number, screen.connector.as_str()))
            .collect();
        assert_eq!(numbered, [(1, "eDP-1"), (2, "DP-1")]);
    }

    #[test]
    fn mode_display_rounds_the_refresh_rate() {
        let mode = Mode {
            width: 2560,
            height: 1440,
            refresh_rate: 143.912,
        };
        assert_eq!(mode.to_string(), "2560 x 1440 @ 144 Hz");
    }
}
