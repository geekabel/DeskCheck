//! Configuration file of DeskCheck.
//!
//! The file keeps what the user chose: the names given to sound devices and
//! screens, and the color of each screen. It lives in
//! `~/.config/deskcheck/config.toml` (or under `$XDG_CONFIG_HOME`).
//!
//! - [`Config`] is the content, in memory.
//! - [`Config::from_toml`] and [`Config::to_toml`] read and write the text.
//! - [`Store`] loads and saves the file on disk. Saving is safe: the new
//!   content goes to a temporary file first, which then replaces the old
//!   file in one step, so a crash never leaves a half-written file.
//!
//! ```
//! use deskcheck_config::{Color, Config};
//! use deskcheck_core::display::ScreenId;
//!
//! let screen = ScreenId::new("AUS", "XG27ACS", "T2LMTF031523");
//! let mut config = Config::default();
//! config.set_screen_name(&screen, Some("Big screen".to_owned()));
//! config.set_screen_color(&screen, Some("#3584e4".parse::<Color>()?));
//!
//! let text = config.to_toml();
//! assert_eq!(Config::from_toml(&text)?, config);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::fmt;
use std::str::FromStr;

use deskcheck_core::audio::DeviceId;
use deskcheck_core::display::ScreenId;
use deskcheck_core::places::Place;

mod file;
mod labels;
mod store;

pub use file::{FORMAT_VERSION, ParseError};
pub use store::{Error, FILE_NAME, Store};

/// A color in the `#rrggbb` form.
///
/// ```
/// use deskcheck_config::Color;
///
/// let blue: Color = "#3584E4".parse().unwrap();
/// assert_eq!(blue, Color { red: 0x35, green: 0x84, blue: 0xe4 });
/// assert_eq!(blue.to_string(), "#3584e4");
/// assert!("blue".parse::<Color>().is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red, from 0 to 255.
    pub red: u8,
    /// Green, from 0 to 255.
    pub green: u8,
    /// Blue, from 0 to 255.
    pub blue: u8,
}

/// The text is not a color in the `#rrggbb` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorError(pub String);

impl fmt::Display for ColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not a color like #3584e4", self.0)
    }
}

impl std::error::Error for ColorError {}

impl FromStr for Color {
    type Err = ColorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || ColorError(text.to_owned());
        let digits = text.strip_prefix('#').ok_or_else(error)?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(error());
        }
        let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).map_err(|_| error());
        Ok(Self {
            red: channel(0)?,
            green: channel(2)?,
            blue: channel(4)?,
        })
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}

/// The picture the user chose for a sound device, shown instead of the
/// one DeskCheck guesses from the hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceIcon {
    /// A headset or headphones.
    Headset,
    /// Speakers.
    Speakers,
    /// The sound of a screen.
    Screen,
    /// The speakers or the microphone of the laptop.
    Laptop,
    /// A microphone.
    Microphone,
    /// The microphone of a webcam.
    Webcam,
}

impl DeviceIcon {
    /// Every icon, in the order of the choices.
    pub const ALL: [Self; 6] = [
        Self::Headset,
        Self::Speakers,
        Self::Screen,
        Self::Laptop,
        Self::Microphone,
        Self::Webcam,
    ];

    /// Its name in the settings file.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Headset => "headset",
            Self::Speakers => "speakers",
            Self::Screen => "screen",
            Self::Laptop => "laptop",
            Self::Microphone => "microphone",
            Self::Webcam => "webcam",
        }
    }

    /// Reads a name of the settings file.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|icon| icon.key() == key)
    }
}

/// What the user chose for a sound device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioEntry {
    /// The device.
    pub id: DeviceId,
    /// The name given by the user, if any.
    pub name: Option<String>,
    /// The icon chosen by the user, if any.
    pub icon: Option<DeviceIcon>,
}

/// What the user chose for a screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenEntry {
    /// The screen.
    pub id: ScreenId,
    /// The name given by the user, if any.
    pub name: Option<String>,
    /// The color of the screen, if any.
    pub color: Option<Color>,
}

/// The content of the configuration file.
///
/// Each device and each screen appears at most once.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Config {
    audio: Vec<AudioEntry>,
    screens: Vec<ScreenEntry>,
    places: Vec<Place>,
    setup_done: bool,
}

/// Turns an empty or blank name into `None`.
fn clean(name: Option<String>) -> Option<String> {
    name.map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

impl Config {
    /// The names given to sound devices, in the order of the file.
    #[must_use]
    pub fn audio_entries(&self) -> &[AudioEntry] {
        &self.audio
    }

    /// What the user chose for each screen, in the order of the file.
    #[must_use]
    pub fn screen_entries(&self) -> &[ScreenEntry] {
        &self.screens
    }

    fn audio_entry(&self, id: &DeviceId) -> Option<&AudioEntry> {
        self.audio.iter().find(|entry| &entry.id == id)
    }

    /// Changes the entry of a sound device, creating it if needed. An
    /// entry with neither a name nor an icon is removed.
    fn edit_audio(&mut self, id: &DeviceId, edit: impl FnOnce(&mut AudioEntry)) {
        let at = if let Some(at) = self.audio.iter().position(|entry| &entry.id == id) {
            at
        } else {
            self.audio.push(AudioEntry {
                id: id.clone(),
                name: None,
                icon: None,
            });
            self.audio.len() - 1
        };
        edit(&mut self.audio[at]);
        if self.audio[at].name.is_none() && self.audio[at].icon.is_none() {
            self.audio.remove(at);
        }
    }

    /// The name given to a sound device.
    #[must_use]
    pub fn audio_name(&self, id: &DeviceId) -> Option<&str> {
        self.audio_entry(id)?.name.as_deref()
    }

    /// Gives a name to a sound device. `None` or a blank name removes it.
    pub fn set_audio_name(&mut self, id: &DeviceId, name: Option<String>) {
        let name = clean(name);
        self.edit_audio(id, |entry| entry.name = name);
    }

    /// The icon chosen for a sound device.
    #[must_use]
    pub fn audio_icon(&self, id: &DeviceId) -> Option<DeviceIcon> {
        self.audio_entry(id)?.icon
    }

    /// Chooses the icon of a sound device. `None` goes back to the one
    /// DeskCheck guesses.
    pub fn set_audio_icon(&mut self, id: &DeviceId, icon: Option<DeviceIcon>) {
        self.edit_audio(id, |entry| entry.icon = icon);
    }

    /// `true` once the user went through the first-run assistant (DC-32),
    /// to the end or not.
    #[must_use]
    pub fn setup_done(&self) -> bool {
        self.setup_done
    }

    /// Marks the first-run assistant as seen.
    pub fn set_setup_done(&mut self, done: bool) {
        self.setup_done = done;
    }

    fn screen_entry(&self, id: &ScreenId) -> Option<&ScreenEntry> {
        self.screens.iter().find(|entry| &entry.id == id)
    }

    /// Changes the entry of a screen, creating it if needed. An entry with
    /// neither a name nor a color is removed.
    fn edit_screen(&mut self, id: &ScreenId, edit: impl FnOnce(&mut ScreenEntry)) {
        let at = if let Some(at) = self.screens.iter().position(|entry| &entry.id == id) {
            at
        } else {
            self.screens.push(ScreenEntry {
                id: id.clone(),
                name: None,
                color: None,
            });
            self.screens.len() - 1
        };
        edit(&mut self.screens[at]);
        if self.screens[at].name.is_none() && self.screens[at].color.is_none() {
            self.screens.remove(at);
        }
    }

    /// The name given to a screen.
    #[must_use]
    pub fn screen_name(&self, id: &ScreenId) -> Option<&str> {
        self.screen_entry(id)?.name.as_deref()
    }

    /// Gives a name to a screen. `None` or a blank name removes it.
    pub fn set_screen_name(&mut self, id: &ScreenId, name: Option<String>) {
        let name = clean(name);
        self.edit_screen(id, |entry| entry.name = name);
    }

    /// The color of a screen.
    #[must_use]
    pub fn screen_color(&self, id: &ScreenId) -> Option<Color> {
        self.screen_entry(id)?.color
    }

    /// Sets the color of a screen. `None` removes it.
    pub fn set_screen_color(&mut self, id: &ScreenId, color: Option<Color>) {
        self.edit_screen(id, |entry| entry.color = color);
    }

    /// The places, in the order of the file (and of the Places page).
    #[must_use]
    pub fn places(&self) -> &[Place] {
        &self.places
    }

    /// The places, to change them.
    pub fn places_mut(&mut self) -> &mut Vec<Place> {
        &mut self.places
    }

    /// Reads the text of a configuration file.
    ///
    /// When a device or a screen appears more than once, the last entry
    /// wins.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] when the text is not valid TOML, has no
    /// `version`, has a version this DeskCheck does not know, has an
    /// unknown field or a bad color.
    pub fn from_toml(text: &str) -> Result<Self, ParseError> {
        file::parse(text)
    }

    /// Writes the configuration as the text of a configuration file.
    #[must_use]
    pub fn to_toml(&self) -> String {
        file::write(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asus() -> ScreenId {
        ScreenId::new("AUS", "XG27ACS", "T2LMTF031523")
    }

    #[test]
    fn colors_are_read_and_written() {
        let color: Color = "#00ff7F".parse().unwrap();
        assert_eq!(
            color,
            Color {
                red: 0,
                green: 255,
                blue: 127
            }
        );
        assert_eq!(color.to_string(), "#00ff7f");
        for bad in [
            "", "#", "00ff7f", "#00ff7", "#00ff7f0", "#00gg7f", "#+0ff7f", "#ééé",
        ] {
            assert_eq!(
                bad.parse::<Color>(),
                Err(ColorError(bad.to_owned())),
                "{bad}"
            );
        }
    }

    #[test]
    fn audio_names_are_set_changed_and_removed() {
        let speakers = DeviceId::new("alsa_output.pci-0000_00_1f.3.analog-stereo");
        let mut config = Config::default();
        assert_eq!(config.audio_name(&speakers), None);

        config.set_audio_name(&speakers, Some("  Laptop speakers ".to_owned()));
        assert_eq!(config.audio_name(&speakers), Some("Laptop speakers"));
        config.set_audio_name(&speakers, Some("Speakers".to_owned()));
        assert_eq!(config.audio_entries().len(), 1);
        assert_eq!(config.audio_name(&speakers), Some("Speakers"));

        config.set_audio_name(&speakers, Some("   ".to_owned()));
        assert_eq!(config, Config::default());
        config.set_audio_name(&speakers, None);
        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_screen_keeps_its_name_and_color_apart() {
        let blue: Color = "#3584e4".parse().unwrap();
        let mut config = Config::default();
        config.set_screen_color(&asus(), Some(blue));
        config.set_screen_name(&asus(), Some("Big screen".to_owned()));
        assert_eq!(config.screen_name(&asus()), Some("Big screen"));
        assert_eq!(config.screen_color(&asus()), Some(blue));

        config.set_screen_name(&asus(), None);
        assert_eq!(config.screen_name(&asus()), None);
        assert_eq!(config.screen_color(&asus()), Some(blue));

        config.set_screen_color(&asus(), None);
        assert_eq!(config, Config::default());
    }

    #[test]
    fn screens_of_the_same_model_are_told_apart_by_serial() {
        let other = ScreenId::new("AUS", "XG27ACS", "OTHER0001");
        let mut config = Config::default();
        config.set_screen_name(&asus(), Some("Left".to_owned()));
        config.set_screen_name(&other, Some("Right".to_owned()));
        assert_eq!(config.screen_name(&asus()), Some("Left"));
        assert_eq!(config.screen_name(&other), Some("Right"));
    }

    #[test]
    fn a_device_keeps_its_icon_without_a_name() {
        let mut config = Config::default();
        let usb = DeviceId::new("usb");
        config.set_audio_name(&usb, Some("Headset".to_owned()));
        config.set_audio_icon(&usb, Some(DeviceIcon::Headset));
        config.set_audio_name(&usb, None);
        assert_eq!(config.audio_icon(&usb), Some(DeviceIcon::Headset));
        config.set_audio_icon(&usb, None);
        assert_eq!(config.audio_entries(), []);
        assert_eq!(DeviceIcon::from_key("webcam"), Some(DeviceIcon::Webcam));
        assert_eq!(DeviceIcon::from_key("tuba"), None);
    }
}
