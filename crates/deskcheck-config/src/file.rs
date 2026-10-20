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
//!
//! [[places]]
//! name = "Office"
//! on_arrival = "apply"
//! outputs = ["alsa_output.usb-Jabra-00.analog-stereo"]
//! inputs = ["alsa_input.usb-Jabra-00.mono-fallback"]
//!
//! [[places.conditions]]
//! kind = "screen"
//! vendor = "DEL"
//! product = "DELL U2419H"
//! serial = "ABC1234"
//!
//! [[places.conditions]]
//! kind = "wifi"
//! ssid = "UTBM"
//! ```
//!
//! Places came after the first files of version 1. They are optional, so
//! older files are still version 1.

use std::fmt;

use deskcheck_core::audio::DeviceId;
use deskcheck_core::display::ScreenId;
use deskcheck_core::places::{Condition, OnArrival, Place, SavedLayout, SavedScreen};
use serde::{Deserialize, Serialize};

use crate::{Color, ColorError, Config, DeviceIcon};

/// Version of the format written by this DeskCheck.
pub const FORMAT_VERSION: u32 = 1;

/// Lines written at the top of the file.
const HEADER: &str = "\
# DeskCheck settings: names given to devices and screens, screen colors, places.
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
    /// A place is not written the right way.
    BadPlace(String),
    /// The icon of a sound device is not one DeskCheck knows.
    BadIcon(String),
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
            Self::BadPlace(reason) => write!(f, "bad place: {reason}"),
            Self::BadIcon(icon) => write!(f, "unknown device icon `{icon}`"),
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
    /// `true` once the first-run assistant was seen.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    setup_done: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    audio: Vec<AudioV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    screens: Vec<ScreenV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    places: Vec<PlaceV1>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceV1 {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    on_arrival: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    never_outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    inputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    conditions: Vec<ConditionV1>,
    /// The layout of the screens; absent when none was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layout: Option<Vec<SavedScreenV1>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditionV1 {
    /// `screen`, `wifi` or `no-external-screen`.
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    product: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    serial: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ssid: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedScreenV1 {
    vendor: String,
    product: String,
    #[serde(default)]
    serial: String,
    x: i32,
    y: i32,
    scale: f64,
    #[serde(default)]
    primary: bool,
    width: u32,
    height: u32,
    refresh_rate: f64,
}

fn condition_from_v1(condition: ConditionV1) -> Result<Condition, ParseError> {
    match condition.kind.as_str() {
        "screen" => match (condition.vendor, condition.product) {
            (Some(vendor), Some(product)) => Ok(Condition::Screen(ScreenId::new(
                vendor,
                product,
                condition.serial.unwrap_or_default(),
            ))),
            _ => Err(ParseError::BadPlace(
                "a screen condition needs a vendor and a product".to_owned(),
            )),
        },
        "wifi" => condition
            .ssid
            .map(Condition::Wifi)
            .ok_or_else(|| ParseError::BadPlace("a wifi condition needs an ssid".to_owned())),
        "no-external-screen" => Ok(Condition::NoExternalScreen),
        other => Err(ParseError::BadPlace(format!(
            "unknown condition kind \"{other}\" (screen, wifi or no-external-screen)"
        ))),
    }
}

fn condition_to_v1(condition: &Condition) -> ConditionV1 {
    let empty = ConditionV1 {
        kind: String::new(),
        vendor: None,
        product: None,
        serial: None,
        ssid: None,
    };
    match condition {
        Condition::Screen(id) => ConditionV1 {
            kind: "screen".to_owned(),
            vendor: Some(id.vendor.clone()),
            product: Some(id.product.clone()),
            serial: Some(id.serial.clone()).filter(|s| !s.is_empty()),
            ..empty
        },
        Condition::Wifi(ssid) => ConditionV1 {
            kind: "wifi".to_owned(),
            ssid: Some(ssid.clone()),
            ..empty
        },
        Condition::NoExternalScreen => ConditionV1 {
            kind: "no-external-screen".to_owned(),
            ..empty
        },
    }
}

fn place_from_v1(place: PlaceV1) -> Result<Place, ParseError> {
    let on_arrival = match place.on_arrival.as_deref() {
        None => OnArrival::default(),
        Some(key) => OnArrival::from_key(key).ok_or_else(|| {
            ParseError::BadPlace(format!(
                "unknown on_arrival \"{key}\" (apply, ask or nothing)"
            ))
        })?,
    };
    Ok(Place {
        name: place.name,
        conditions: place
            .conditions
            .into_iter()
            .map(condition_from_v1)
            .collect::<Result<_, _>>()?,
        outputs: place.outputs.into_iter().map(DeviceId::new).collect(),
        never_outputs: place.never_outputs.into_iter().map(DeviceId::new).collect(),
        inputs: place.inputs.into_iter().map(DeviceId::new).collect(),
        layout: place.layout.map(|screens| SavedLayout {
            screens: screens
                .into_iter()
                .map(|s| SavedScreen {
                    id: ScreenId::new(s.vendor, s.product, s.serial),
                    x: s.x,
                    y: s.y,
                    scale: s.scale,
                    primary: s.primary,
                    width: s.width,
                    height: s.height,
                    refresh_rate: s.refresh_rate,
                })
                .collect(),
        }),
        on_arrival,
    })
}

fn place_to_v1(place: &Place) -> PlaceV1 {
    let ids = |list: &[DeviceId]| list.iter().map(|id| id.as_str().to_owned()).collect();
    PlaceV1 {
        name: place.name.clone(),
        on_arrival: Some(place.on_arrival.key().to_owned()),
        outputs: ids(&place.outputs),
        never_outputs: ids(&place.never_outputs),
        inputs: ids(&place.inputs),
        conditions: place.conditions.iter().map(condition_to_v1).collect(),
        layout: place.layout.as_ref().map(|layout| {
            layout
                .screens
                .iter()
                .map(|s| SavedScreenV1 {
                    vendor: s.id.vendor.clone(),
                    product: s.id.product.clone(),
                    serial: s.id.serial.clone(),
                    x: s.x,
                    y: s.y,
                    scale: s.scale,
                    primary: s.primary,
                    width: s.width,
                    height: s.height,
                    refresh_rate: s.refresh_rate,
                })
                .collect()
        }),
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioV1 {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// `headset`, `speakers`, `screen`, `laptop`, `microphone` or `webcam`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
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
    config.set_setup_done(file.setup_done);
    for audio in file.audio {
        let id = DeviceId::new(audio.id);
        let icon = audio
            .icon
            .map(|key| DeviceIcon::from_key(&key).ok_or(ParseError::BadIcon(key)))
            .transpose()?;
        config.set_audio_name(&id, audio.name);
        config.set_audio_icon(&id, icon);
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
    for place in file.places {
        let place = place_from_v1(place)?;
        config.places_mut().push(place);
    }
    Ok(config)
}

fn to_v1(config: &Config) -> V1 {
    V1 {
        version: 1,
        setup_done: config.setup_done(),
        audio: config
            .audio_entries()
            .iter()
            .map(|entry| AudioV1 {
                id: entry.id.as_str().to_owned(),
                name: entry.name.clone(),
                icon: entry.icon.map(|icon| icon.key().to_owned()),
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
        places: config.places().iter().map(place_to_v1).collect(),
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

    const GODWIN: &str = r##"# DeskCheck settings: names given to devices and screens, screen colors, places.
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
    fn places_round_trip() {
        let mut config = godwin();
        config.places_mut().push(Place {
            name: "Office".to_owned(),
            conditions: vec![
                Condition::Screen(ScreenId::new("DEL", "DELL U2419H", "ABC1234")),
                Condition::Wifi("UTBM".to_owned()),
            ],
            outputs: vec![DeviceId::new("headset")],
            never_outputs: vec![DeviceId::new("dell")],
            inputs: vec![DeviceId::new("headset.mic")],
            layout: Some(SavedLayout {
                screens: vec![SavedScreen {
                    id: ScreenId::new("DEL", "DELL U2419H", "ABC1234"),
                    x: 0,
                    y: 0,
                    scale: 1.25,
                    primary: true,
                    width: 1920,
                    height: 1080,
                    refresh_rate: 60.0,
                }],
            }),
            on_arrival: OnArrival::Ask,
        });
        config.places_mut().push(Place {
            name: "On the move".to_owned(),
            conditions: vec![Condition::NoExternalScreen],
            ..Place::default()
        });
        let text = write(&config);
        assert!(
            text.contains("[[places.conditions]]\nkind = \"wifi\"\nssid = \"UTBM\""),
            "{text}"
        );
        assert_eq!(parse(&text).unwrap(), config);
    }

    #[test]
    fn a_bad_place_is_an_error() {
        let error =
            parse("version = 1\n[[places]]\nname = \"x\"\n[[places.conditions]]\nkind = \"moon\"")
                .unwrap_err();
        assert!(
            error.to_string().contains("unknown condition kind"),
            "{error}"
        );
        let error =
            parse("version = 1\n[[places]]\nname = \"x\"\non_arrival = \"later\"").unwrap_err();
        assert!(matches!(error, ParseError::BadPlace(_)));
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

    #[test]
    fn icons_and_the_first_run_round_trip() {
        let mut config = Config::default();
        let usb = DeviceId::new("usb");
        config.set_audio_icon(&usb, Some(DeviceIcon::Headset));
        config.set_setup_done(true);
        let text = write(&config);
        assert!(text.contains("setup_done = true"), "{text}");
        assert!(text.contains("icon = \"headset\""), "{text}");
        let read = parse(&text).unwrap();
        assert_eq!(read, config);
        assert_eq!(read.audio_icon(&usb), Some(DeviceIcon::Headset));
        assert_eq!(read.audio_name(&usb), None);
        let error = parse("version = 1\n[[audio]]\nid = \"x\"\nicon = \"tuba\"").unwrap_err();
        assert_eq!(error, ParseError::BadIcon("tuba".to_owned()));
        // Without the line, the assistant is still to see.
        assert!(!parse("version = 1").unwrap().setup_done());
    }
}
