//! The name to show for a device or a screen: the name the user gave, or
//! else the name the system gives.

use std::borrow::Cow;

use deskcheck_core::audio::{Input, InputKind, Output};
use deskcheck_core::display::Screen;

use crate::Config;

impl Config {
    /// Name to show for an output.
    ///
    /// ```
    /// use deskcheck_config::Config;
    /// use deskcheck_core::audio::{DeviceId, Hardware, Output, Volume};
    ///
    /// let output = Output {
    ///     id: DeviceId::new("alsa_output.pci-0000_00_1f.3.analog-stereo"),
    ///     description: "Built-in Audio Analog Stereo".to_owned(),
    ///     hardware: Hardware::default(),
    ///     volume: Volume::NORMAL,
    ///     muted: false,
    /// };
    /// let mut config = Config::default();
    /// assert_eq!(config.output_label(&output), "Built-in Audio Analog Stereo");
    ///
    /// config.set_audio_name(&output.id, Some("Laptop speakers".to_owned()));
    /// assert_eq!(config.output_label(&output), "Laptop speakers");
    /// ```
    #[must_use]
    pub fn output_label<'a>(&'a self, output: &'a Output) -> &'a str {
        self.audio_name(&output.id).unwrap_or(&output.description)
    }

    /// Name to show for an input.
    ///
    /// A monitor without its own name follows the name of its output:
    /// "Monitor of Laptop speakers".
    #[must_use]
    pub fn input_label<'a>(&'a self, input: &'a Input) -> Cow<'a, str> {
        if let Some(name) = self.audio_name(&input.id) {
            return Cow::Borrowed(name);
        }
        match &input.kind {
            InputKind::Monitor { of } => match self.audio_name(of) {
                Some(output) => Cow::Owned(format!("Monitor of {output}")),
                None => Cow::Borrowed(&input.description),
            },
            InputKind::Capture => Cow::Borrowed(&input.description),
        }
    }

    /// Name to show for a screen: the given name, else the name from GNOME,
    /// else the product.
    #[must_use]
    pub fn screen_label<'a>(&'a self, screen: &'a Screen) -> &'a str {
        self.screen_name(&screen.id)
            .unwrap_or_else(|| screen.name())
    }
}

#[cfg(test)]
mod tests {
    use deskcheck_core::audio::{DeviceId, Hardware, Volume};
    use deskcheck_core::display::{Mode, Placement, ScreenId};

    use super::*;

    const SPEAKERS: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo";

    fn monitor() -> Input {
        Input {
            id: DeviceId::new(format!("{SPEAKERS}.monitor")),
            description: "Monitor of Built-in Audio Analog Stereo".to_owned(),
            hardware: Hardware::default(),
            kind: InputKind::Monitor {
                of: DeviceId::new(SPEAKERS),
            },
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    #[test]
    fn a_monitor_follows_the_name_of_its_output() {
        let mut config = Config::default();
        assert_eq!(
            config.input_label(&monitor()),
            "Monitor of Built-in Audio Analog Stereo"
        );
        config.set_audio_name(&DeviceId::new(SPEAKERS), Some("Laptop speakers".to_owned()));
        assert_eq!(config.input_label(&monitor()), "Monitor of Laptop speakers");
        config.set_audio_name(&monitor().id, Some("Echo".to_owned()));
        assert_eq!(config.input_label(&monitor()), "Echo");
    }

    #[test]
    fn a_microphone_uses_its_own_name() {
        let mut mic = monitor();
        mic.id = DeviceId::new("alsa_input.pci-0000_00_1f.3.analog-stereo");
        mic.description = "Built-in Audio Analog Stereo".to_owned();
        mic.kind = InputKind::Capture;
        let mut config = Config::default();
        config.set_audio_name(&DeviceId::new(SPEAKERS), Some("Laptop speakers".to_owned()));
        assert_eq!(config.input_label(&mic), "Built-in Audio Analog Stereo");
        config.set_audio_name(&mic.id, Some("Laptop mic".to_owned()));
        assert_eq!(config.input_label(&mic), "Laptop mic");
    }

    #[test]
    fn a_screen_label_falls_back_to_gnome_then_product() {
        let mut screen = Screen {
            id: ScreenId::new("AUS", "XG27ACS", "T2LMTF031523"),
            connector: "HDMI-1".to_owned(),
            display_name: Some("ASUSTek COMPUTER INC 27\"".to_owned()),
            builtin: false,
            current_mode: Some(Mode {
                width: 2560,
                height: 1440,
                refresh_rate: 60.0,
            }),
            placement: Some(Placement {
                x: 1920,
                y: 0,
                scale: 1.0,
                primary: false,
            }),
        };
        let mut config = Config::default();
        assert_eq!(config.screen_label(&screen), "ASUSTek COMPUTER INC 27\"");
        config.set_screen_name(&screen.id, Some("Big screen".to_owned()));
        assert_eq!(config.screen_label(&screen), "Big screen");
        config.set_screen_name(&screen.id, None);
        screen.display_name = None;
        assert_eq!(config.screen_label(&screen), "XG27ACS");
    }
}
