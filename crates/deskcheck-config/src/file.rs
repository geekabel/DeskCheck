//! The text format of the configuration file, version by version.
//!
//! Each version of the format has its own structs here. Reading a file
//! first reads its `version`, then the structs of that version, then turns
//! them into a [`Config`]. A future version 2 will add its structs and a
//! conversion from version 1, so old files keep working.
//!
//! Version 1 looks like this:
//!
//! ```toml
//! version = 1
//!
//! [[audio]]
//! id = "alsa_output.pci-0000_00_1f.3.analog-stereo"
//! name = "Laptop speakers"
//!
//! [[screens]]
//! vendor = "AUS"
//! product = "XG27ACS"
//! serial = "T2LMTF031523"
//! name = "Big screen"
//! color = "#3584e4"
//! ```

use std::fmt;

use deskcheck_core::audio::DeviceId;
use deskcheck_core::display::ScreenId;
use serde::{Deserialize, Serialize};

use crate::{Color, ColorError, Config};

/// Version of the format written by this DeskCheck.
pub const FORMAT_VERSION: u32 = 1;

/// Lines written at the top of the file.
const HEADER: &str = "\
# DeskCheck settings: names given to devices and screens, screen colors.
# DeskCheck rewrites this file when you change a setting in the app.
";

/// Why the text of a configuration file cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The text is not valid TOML, or a field is unknown or of the wrong
    /// type. The message comes from the `toml` crate and shows the line.
    Toml(String),
    /// The file has no `version` field.
    NoVersion,
    /// The file was written by a newer DeskCheck.
    TooNew(u32),
    /// The version is not one DeskCheck ever wrote.
    UnknownVersion(u32),
    /// A screen color is not in the `#rrggbb` form.
    BadColor(ColorError),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(message) => f.write_str(message.trim_end()),
            Self::NoVersion => f.write_str("the file has no `version` line, like `version = 1`"),
            Self::TooNew(found) => write!(
                f,
                "the file has version {found}, written by a newer DeskCheck \
                 (this one reads up to version {FORMAT_VERSION})"
            ),
            Self::UnknownVersion(found) => write!(f, "version {found} does not exist"),
            Self::BadColor(error) => write!(f, "bad screen color: {error}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<toml::de::Error> for ParseError {
    fn from(error: toml::de::Error) -> Self {
        Self::Toml(error.to_string())
    }
}

/// Only the version. Other fields are ignored at this step.
#[derive(Deserialize)]
struct Header {
    version: Option<u32>,
}

/// Version 1 of the file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1 {
    version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    audio: Vec<AudioV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    screens: Vec<ScreenV1>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioV1 {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenV1 {
    vendor: String,
    product: String,
    #[serde(default)]
    serial: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

fn from_v1(file: V1) -> Result<Config, ParseError> {
    let mut config = Config::default();
    for audio in file.audio {
        config.set_audio_name(&DeviceId::new(audio.id), Some(audio.name));
    }
    for screen in file.screens {
        let id = ScreenId::new(screen.vendor, screen.product, screen.serial);
        let color = screen
            .color
            .map(|color| color.parse::<Color>())
            .transpose()
            .map_err(ParseError::BadColor)?;
        config.set_screen_name(&id, screen.name);
        config.set_screen_color(&id, color);
    }
    Ok(config)
}

fn to_v1(config: &Config) -> V1 {
    V1 {
        version: 1,
        audio: config
            .audio_entries()
            .iter()
            .map(|entry| AudioV1 {
                id: entry.id.as_str().to_owned(),
                name: entry.name.clone(),
            })
            .collect(),
        screens: config
            .screen_entries()
            .iter()
            .map(|entry| ScreenV1 {
                vendor: entry.id.vendor.clone(),
                product: entry.id.product.clone(),
                serial: entry.id.serial.clone(),
                name: entry.name.clone(),
                color: entry.color.map(|color| color.to_string()),
            })
            .collect(),
    }
}

pub(crate) fn parse(text: &str) -> Result<Config, ParseError> {
    let header: Header = toml::from_str(text)?;
    match header.version {
        None => Err(ParseError::NoVersion),
        Some(1) => from_v1(toml::from_str(text)?),
        Some(found) if found > FORMAT_VERSION => Err(ParseError::TooNew(found)),
        Some(found) => Err(ParseError::UnknownVersion(found)),
    }
}

pub(crate) fn write(config: &Config) -> String {
    // A struct of strings, numbers and arrays of tables always turns into
    // TOML, so the error case cannot happen.
    let body = toml::to_string_pretty(&to_v1(config))
        .expect("version 1 of the config is always valid TOML");
    format!("{HEADER}\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GODWIN: &str = r##"# DeskCheck settings: names given to devices and screens, screen colors.
# DeskCheck rewrites this file when you change a setting in the app.

version = 1

[[audio]]
id = "alsa_output.pci-0000_00_1f.3.analog-stereo"
name = "Laptop speakers"

[[screens]]
vendor = "LGD"
product = "0x0764"
serial = "0x00000000"
name = "Laptop"

[[screens]]
vendor = "AUS"
product = "XG27ACS"
serial = "T2LMTF031523"
name = "Big screen"
color = "#3584e4"
"##;

    fn godwin() -> Config {
        let mut config = Config::default();
        config.set_audio_name(
            &DeviceId::new("alsa_output.pci-0000_00_1f.3.analog-stereo"),
            Some("Laptop speakers".to_owned()),
        );
        let laptop = ScreenId::new("LGD", "0x0764", "0x00000000");
        let asus = ScreenId::new("AUS", "XG27ACS", "T2LMTF031523");
        config.set_screen_name(&laptop, Some("Laptop".to_owned()));
        config.set_screen_name(&asus, Some("Big screen".to_owned()));
        config.set_screen_color(&asus, Some("#3584e4".parse().unwrap()));
        config
    }

    #[test]
    fn a_full_file_is_read() {
        assert_eq!(parse(GODWIN).unwrap(), godwin());
    }

    #[test]
    fn the_file_is_written_as_expected() {
        assert_eq!(write(&godwin()), GODWIN);
    }

    #[test]
    fn an_empty_config_round_trips() {
        let text = write(&Config::default());
        assert!(text.ends_with("\nversion = 1\n"), "{text}");
        assert_eq!(parse(&text).unwrap(), Config::default());
        assert_eq!(parse("version = 1").unwrap(), Config::default());
    }

    #[test]
    fn the_version_is_required_and_checked() {
        assert_eq!(parse(""), Err(ParseError::NoVersion));
        assert_eq!(parse("version = 2"), Err(ParseError::TooNew(2)));
        assert_eq!(parse("version = 0"), Err(ParseError::UnknownVersion(0)));
        let error = parse("version = -1").unwrap_err();
        assert!(matches!(error, ParseError::Toml(_)), "{error:?}");
    }

    #[test]
    fn a_newer_file_is_not_read_as_version_1() {
        // A field this version does not know must not stop the version check.
        let error = parse("version = 2\n[[windows]]\nname = \"x\"").unwrap_err();
        assert_eq!(error, ParseError::TooNew(2));
        assert!(error.to_string().contains("newer DeskCheck"));
    }

    #[test]
    fn an_unknown_field_is_an_error() {
        let error = parse("version = 1\n[[audio]]\nid = \"a\"\nnmae = \"b\"").unwrap_err();
        let ParseError::Toml(message) = error else {
            panic!("{error:?}");
        };
        assert!(message.contains("nmae"), "{message}");
    }

    #[test]
    fn a_bad_color_is_an_error() {
        let text = "version = 1\n[[screens]]\nvendor = \"AUS\"\nproduct = \"X\"\ncolor = \"blue\"";
        assert_eq!(
            parse(text),
            Err(ParseError::BadColor(ColorError("blue".to_owned())))
        );
    }

    #[test]
    fn the_last_entry_wins() {
        let text = "version = 1\n\
            [[audio]]\nid = \"a\"\nname = \"First\"\n\
            [[audio]]\nid = \"a\"\nname = \"Second\"";
        let config = parse(text).unwrap();
        assert_eq!(config.audio_entries().len(), 1);
        assert_eq!(config.audio_name(&DeviceId::new("a")), Some("Second"));
    }

    #[test]
    fn a_screen_without_serial_is_read() {
        let text =
            "version = 1\n[[screens]]\nvendor = \"BOE\"\nproduct = \"0x0a1c\"\nname = \"Laptop\"";
        let config = parse(text).unwrap();
        assert_eq!(
            config.screen_name(&ScreenId::new("BOE", "0x0a1c", "")),
            Some("Laptop")
        );
    }
}
