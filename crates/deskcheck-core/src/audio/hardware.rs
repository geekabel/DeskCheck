//! What a sound device is: a headset, speakers, a webcam, a screen...
//!
//! The sound server describes each device with a few clues:
//!
//! - `device.form_factor`: `headset`, `headphone`, `speaker`, `webcam`,
//!   `microphone`, `internal`, `tv`... (set by udev or by the driver);
//! - `device.bus`: `usb`, `pci`, `bluetooth`...;
//! - the name of the active port, from the ALSA mixer paths of PulseAudio:
//!   `analog-output-headphones`, `analog-input-internal-mic`,
//!   `hdmi-output-0`...;
//! - the device name and description, for example
//!   `alsa_output.pci-0000_00_1f.3.hdmi-stereo`.
//!
//! [`classify`] turns these clues into a [`Hardware`]: a [`Form`] (what the
//! device is) and a [`Connection`] (how it is plugged).

use std::fmt;

/// What a sound device is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Form {
    /// Headphones with a microphone, wired or Bluetooth.
    Headset,
    /// Headphones without a microphone.
    Headphones,
    /// Speakers, built in or external.
    Speakers,
    /// A microphone, built in or external.
    Microphone,
    /// The microphone of a webcam.
    Webcam,
    /// The sound of a screen or a TV, through HDMI or DisplayPort.
    Screen,
    /// The clues are not enough to tell.
    #[default]
    Unknown,
}

impl fmt::Display for Form {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Headset => "headset",
            Self::Headphones => "headphones",
            Self::Speakers => "speakers",
            Self::Microphone => "microphone",
            Self::Webcam => "webcam",
            Self::Screen => "screen",
            Self::Unknown => "unknown",
        })
    }
}

/// How a sound device is connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Connection {
    /// Part of the computer: the internal sound card or its jack.
    BuiltIn,
    /// A USB device.
    Usb,
    /// A Bluetooth device.
    Bluetooth,
    /// HDMI or DisplayPort, so through a screen.
    Hdmi,
    /// The clues are not enough to tell.
    #[default]
    Unknown,
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BuiltIn => "built-in",
            Self::Usb => "USB",
            Self::Bluetooth => "Bluetooth",
            Self::Hdmi => "HDMI",
            Self::Unknown => "unknown",
        })
    }
}

/// What a device is and how it is connected.
///
/// ```
/// use deskcheck_core::audio::{Connection, Form, Hardware};
///
/// let headset = Hardware { form: Form::Headset, connection: Connection::Usb };
/// assert_eq!(headset.icon_name(), "audio-headset");
/// assert_eq!(headset.to_string(), "headset (USB)");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Hardware {
    /// What the device is.
    pub form: Form,
    /// How it is connected.
    pub connection: Connection,
}

impl Hardware {
    /// Name of the icon for this device, from the freedesktop icon names
    /// used by the Adwaita theme. GTK can add `-symbolic` for the
    /// one-color version.
    ///
    /// Built-in speakers and microphones get the computer icon, so the
    /// laptop is easy to tell apart from external devices.
    #[must_use]
    pub fn icon_name(self) -> &'static str {
        match (self.form, self.connection) {
            (Form::Headset, _) => "audio-headset",
            (Form::Headphones, _) => "audio-headphones",
            (Form::Webcam, _) => "camera-web",
            (Form::Screen, _) | (_, Connection::Hdmi) => "video-display",
            (Form::Speakers | Form::Microphone | Form::Unknown, Connection::BuiltIn) => "computer",
            (Form::Speakers, _) => "audio-speakers",
            (Form::Microphone, _) => "audio-input-microphone",
            (Form::Unknown, _) => "audio-card",
        }
    }
}

impl fmt::Display for Hardware {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.form, self.connection)
    }
}

/// Whether a device plays sound or gives sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// The device plays sound (an output, or the monitor of an output).
    Output,
    /// The device gives sound (a microphone, a line in).
    Input,
}

/// The clues the sound server gives about a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Clues<'a> {
    /// Device name, for example `alsa_output.pci-0000_00_1f.3.hdmi-stereo`.
    pub name: &'a str,
    /// Device description, for example `Built-in Audio Digital Stereo (HDMI)`.
    pub description: &'a str,
    /// `device.form_factor` property.
    pub form_factor: Option<&'a str>,
    /// `device.bus` property.
    pub bus: Option<&'a str>,
    /// Name of the active port, for example `analog-output-headphones`.
    pub port: Option<&'a str>,
}

impl Clues<'_> {
    fn port(&self) -> String {
        self.port.unwrap_or_default().to_ascii_lowercase()
    }

    /// HDMI and DisplayPort sound goes through a screen.
    fn is_screen(&self) -> bool {
        let name = self.name.to_ascii_lowercase();
        let description = self.description.to_ascii_lowercase();
        self.port().starts_with("hdmi")
            || name.contains("hdmi")
            || description.contains("hdmi")
            || description.contains("displayport")
            || self.form_factor == Some("tv")
    }
}

/// Form from the `device.form_factor` property, if it says enough.
fn form_from_form_factor(direction: Direction, form_factor: &str) -> Option<Form> {
    match form_factor {
        "headset" | "handset" | "hands-free" => Some(Form::Headset),
        "headphone" => Some(Form::Headphones),
        "speaker" | "hifi" | "car" => Some(Form::Speakers),
        "microphone" => Some(Form::Microphone),
        "webcam" => Some(Form::Webcam),
        "tv" => Some(Form::Screen),
        "internal" => Some(match direction {
            Direction::Output => Form::Speakers,
            Direction::Input => Form::Microphone,
        }),
        _ => None,
    }
}

/// Form from the name of the active port, if it says enough.
fn form_from_port(direction: Direction, port: &str) -> Option<Form> {
    if port.contains("headset") {
        Some(Form::Headset)
    } else if port.contains("headphone") {
        // "analog-input-headphone-mic" is the microphone of a headset.
        Some(match direction {
            Direction::Output => Form::Headphones,
            Direction::Input => Form::Headset,
        })
    } else if port.contains("speaker") {
        Some(Form::Speakers)
    } else if port.contains("mic") {
        Some(Form::Microphone)
    } else {
        None
    }
}

/// Turns the clues of the sound server into a [`Hardware`].
///
/// The order of the rules:
///
/// 1. HDMI or DisplayPort means the sound of a screen.
/// 2. Then `device.form_factor`, the most direct clue.
/// 3. Then the name of the active port (a plugged jack changes it).
/// 4. The connection comes from `device.bus`, or "built-in" for an
///    internal device.
///
/// ```
/// use deskcheck_core::audio::{classify, Clues, Connection, Direction, Form};
///
/// let clues = Clues {
///     name: "alsa_input.pci-0000_00_1f.3.analog-stereo",
///     description: "Built-in Audio Analog Stereo",
///     bus: Some("pci"),
///     port: Some("analog-input-internal-mic"),
///     ..Clues::default()
/// };
/// let hardware = classify(Direction::Input, &clues);
/// assert_eq!(hardware.form, Form::Microphone);
/// assert_eq!(hardware.connection, Connection::BuiltIn);
/// ```
#[must_use]
pub fn classify(direction: Direction, clues: &Clues<'_>) -> Hardware {
    if clues.is_screen() {
        return Hardware {
            form: Form::Screen,
            connection: Connection::Hdmi,
        };
    }

    let port = clues.port();
    let form = clues
        .form_factor
        .and_then(|value| form_from_form_factor(direction, value))
        .or_else(|| form_from_port(direction, &port))
        .unwrap_or_default();

    let connection = match clues.bus {
        Some("usb") => Connection::Usb,
        Some("bluetooth") => Connection::Bluetooth,
        Some("pci" | "isa") => Connection::BuiltIn,
        _ if clues.form_factor == Some("internal") || port.contains("internal") => {
            Connection::BuiltIn
        }
        _ => Connection::Unknown,
    };

    Hardware { form, connection }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clues in the shape PulseAudio gives them on Ubuntu 22.04. The port
    /// names come from the ALSA mixer paths shipped with PulseAudio.
    struct Case {
        direction: Direction,
        clues: Clues<'static>,
        form: Form,
        connection: Connection,
        icon: &'static str,
    }

    const CASES: [Case; 10] = [
        // Laptop speakers.
        Case {
            direction: Direction::Output,
            clues: Clues {
                name: "alsa_output.pci-0000_00_1f.3.analog-stereo",
                description: "Built-in Audio Analog Stereo",
                form_factor: None,
                bus: Some("pci"),
                port: Some("analog-output-speaker"),
            },
            form: Form::Speakers,
            connection: Connection::BuiltIn,
            icon: "computer",
        },
        // Same card, wired headphones in the jack.
        Case {
            direction: Direction::Output,
            clues: Clues {
                name: "alsa_output.pci-0000_00_1f.3.analog-stereo",
                description: "Built-in Audio Analog Stereo",
                form_factor: None,
                bus: Some("pci"),
                port: Some("analog-output-headphones"),
            },
            form: Form::Headphones,
            connection: Connection::BuiltIn,
            icon: "audio-headphones",
        },
        // Laptop microphone.
        Case {
            direction: Direction::Input,
            clues: Clues {
                name: "alsa_input.pci-0000_00_1f.3.analog-stereo",
                description: "Built-in Audio Analog Stereo",
                form_factor: None,
                bus: Some("pci"),
                port: Some("analog-input-internal-mic"),
            },
            form: Form::Microphone,
            connection: Connection::BuiltIn,
            icon: "computer",
        },
        // Microphone of a wired headset in the jack.
        Case {
            direction: Direction::Input,
            clues: Clues {
                name: "alsa_input.pci-0000_00_1f.3.analog-stereo",
                description: "Built-in Audio Analog Stereo",
                form_factor: None,
                bus: Some("pci"),
                port: Some("analog-input-headset-mic"),
            },
            form: Form::Headset,
            connection: Connection::BuiltIn,
            icon: "audio-headset",
        },
        // Sound of an external screen, through HDMI or DisplayPort.
        Case {
            direction: Direction::Output,
            clues: Clues {
                name: "alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1",
                description: "Built-in Audio Digital Stereo (HDMI 2)",
                form_factor: None,
                bus: Some("pci"),
                port: Some("hdmi-output-1"),
            },
            form: Form::Screen,
            connection: Connection::Hdmi,
            icon: "video-display",
        },
        // Monitor of that output: still the screen.
        Case {
            direction: Direction::Output,
            clues: Clues {
                name: "alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1.monitor",
                description: "Monitor of Built-in Audio Digital Stereo (HDMI 2)",
                form_factor: None,
                bus: Some("pci"),
                port: None,
            },
            form: Form::Screen,
            connection: Connection::Hdmi,
            icon: "video-display",
        },
        // USB headset.
        Case {
            direction: Direction::Output,
            clues: Clues {
                name: "alsa_output.usb-Logitech_USB_Headset-00.analog-stereo",
                description: "USB Headset Analog Stereo",
                form_factor: Some("headset"),
                bus: Some("usb"),
                port: Some("analog-output"),
            },
            form: Form::Headset,
            connection: Connection::Usb,
            icon: "audio-headset",
        },
        // Microphone of a USB webcam.
        Case {
            direction: Direction::Input,
            clues: Clues {
                name: "alsa_input.usb-046d_HD_Pro_Webcam_C920-02.analog-stereo",
                description: "HD Pro Webcam C920 Analog Stereo",
                form_factor: Some("webcam"),
                bus: Some("usb"),
                port: Some("analog-input-mic"),
            },
            form: Form::Webcam,
            connection: Connection::Usb,
            icon: "camera-web",
        },
        // Bluetooth headphones.
        Case {
            direction: Direction::Output,
            clues: Clues {
                name: "bluez_sink.00_11_22_33_44_55.a2dp_sink",
                description: "Bluetooth Headphones",
                form_factor: Some("headphone"),
                bus: Some("bluetooth"),
                port: Some("headphone-output"),
            },
            form: Form::Headphones,
            connection: Connection::Bluetooth,
            icon: "audio-headphones",
        },
        // A virtual device with no clue at all.
        Case {
            direction: Direction::Output,
            clues: Clues {
                name: "auto_null",
                description: "Dummy Output",
                form_factor: None,
                bus: None,
                port: None,
            },
            form: Form::Unknown,
            connection: Connection::Unknown,
            icon: "audio-card",
        },
    ];

    #[test]
    fn known_devices_are_classified() {
        for case in &CASES {
            let hardware = classify(case.direction, &case.clues);
            assert_eq!(hardware.form, case.form, "form of {}", case.clues.name);
            assert_eq!(
                hardware.connection, case.connection,
                "connection of {}",
                case.clues.name
            );
            assert_eq!(
                hardware.icon_name(),
                case.icon,
                "icon of {}",
                case.clues.name
            );
        }
    }

    #[test]
    fn form_factor_wins_over_the_port() {
        let clues = Clues {
            name: "alsa_input.usb-Webcam-02.analog-stereo",
            form_factor: Some("webcam"),
            port: Some("analog-input-mic"),
            ..Clues::default()
        };
        assert_eq!(classify(Direction::Input, &clues).form, Form::Webcam);
    }

    #[test]
    fn internal_form_factor_depends_on_the_direction() {
        let clues = Clues {
            form_factor: Some("internal"),
            ..Clues::default()
        };
        let output = classify(Direction::Output, &clues);
        let input = classify(Direction::Input, &clues);
        assert_eq!(output.form, Form::Speakers);
        assert_eq!(input.form, Form::Microphone);
        assert_eq!(output.connection, Connection::BuiltIn);
    }

    #[test]
    fn displayport_in_the_description_means_a_screen() {
        let clues = Clues {
            name: "alsa_output.pci-0000_01_00.1.pro-output-3",
            description: "DisplayPort 1 Output",
            ..Clues::default()
        };
        assert_eq!(classify(Direction::Output, &clues).form, Form::Screen);
    }

    #[test]
    fn display_texts() {
        assert_eq!(Form::Webcam.to_string(), "webcam");
        assert_eq!(Connection::BuiltIn.to_string(), "built-in");
        assert_eq!(Hardware::default().to_string(), "unknown (unknown)");
    }
}
