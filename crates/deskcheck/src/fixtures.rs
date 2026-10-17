//! Devices and screens shared by the tests of the commands.

use deskcheck_core::audio::{
    AudioState, Connection, DeviceId, Form, Hardware, Input, InputKind, Output, Volume,
};
use deskcheck_core::display::{DisplayState, Mode, Placement, Screen, ScreenId};

pub const SPEAKERS: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo";
pub const MIC: &str = "alsa_input.pci-0000_00_1f.3.analog-stereo";
pub const MONITOR: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor";
pub const HEADSET: &str = "alsa_output.usb-GN_Netcom_A_S_Jabra_EVOLVE_LINK-00.analog-stereo";

pub fn device_output(id: &str, description: &str, form: Form) -> Output {
    Output {
        id: DeviceId::new(id),
        description: description.to_owned(),
        hardware: Hardware {
            form,
            connection: Connection::Unknown,
        },
        volume: Volume::NORMAL,
        muted: false,
    }
}

pub fn device_input(id: &str, description: &str, kind: InputKind) -> Input {
    Input {
        id: DeviceId::new(id),
        description: description.to_owned(),
        hardware: Hardware::default(),
        kind,
        volume: Volume::NORMAL,
        muted: false,
    }
}

/// Godwin's home devices, plus a headset as at the office.
pub fn audio() -> AudioState {
    AudioState {
        outputs: vec![
            device_output(SPEAKERS, "Built-in Audio Analog Stereo", Form::Speakers),
            device_output(HEADSET, "Jabra EVOLVE LINK Analog Stereo", Form::Headset),
        ],
        inputs: vec![
            device_input(
                MONITOR,
                "Monitor of Built-in Audio Analog Stereo",
                InputKind::Monitor {
                    of: DeviceId::new(SPEAKERS),
                },
            ),
            device_input(MIC, "Built-in Audio Analog Stereo", InputKind::Capture),
        ],
        default_output: Some(DeviceId::new(SPEAKERS)),
        default_input: Some(DeviceId::new(MIC)),
        playbacks: vec![],
        recordings: vec![],
    }
}

fn display(id: ScreenId, connector: &str, name: &str, x: i32, on: bool) -> Screen {
    Screen {
        id,
        connector: connector.to_owned(),
        display_name: Some(name.to_owned()),
        builtin: connector.starts_with("eDP"),
        current_mode: on.then_some(Mode {
            width: 1920,
            height: 1080,
            refresh_rate: 60.0,
        }),
        placement: on.then_some(Placement {
            x,
            y: 0,
            scale: 1.0,
            primary: x == 0,
        }),
    }
}

/// Godwin's home screens, plus an office screen plugged in but off.
pub fn screens() -> DisplayState {
    DisplayState {
        screens: vec![
            display(
                ScreenId::new("AUS", "XG27ACS", "T2LMTF031523"),
                "HDMI-1",
                "ASUSTek COMPUTER INC 27\"",
                1920,
                true,
            ),
            display(
                ScreenId::new("LGD", "0x0764", "0x00000000"),
                "eDP-1",
                "Built-in display",
                0,
                true,
            ),
            display(
                ScreenId::new("DEL", "DELL U2419H", "ABC1234"),
                "DP-1",
                "Dell Inc. 24\"",
                0,
                false,
            ),
        ],
    }
}
