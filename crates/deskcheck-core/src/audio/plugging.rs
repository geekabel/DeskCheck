//! What to do when a sound device is plugged in (DC-31).
//!
//! A headset or headphones just plugged in are almost always meant to be
//! used: DeskCheck makes them the default output, and the microphone of
//! the same headset the default microphone. Speakers, the sound of a
//! screen or a webcam do not take the sound: plugging a screen should not
//! move a call to its speakers.
//!
//! Everything here is pure: the states before and after come from the
//! sound server.

use super::{AudioState, DeviceId, Form};

/// The devices that were not there before.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plugged {
    /// New outputs.
    pub outputs: Vec<DeviceId>,
    /// New microphones (never monitors).
    pub inputs: Vec<DeviceId>,
}

impl Plugged {
    /// The devices of `after` that `before` did not have.
    #[must_use]
    pub fn between(before: &AudioState, after: &AudioState) -> Self {
        Self {
            outputs: after
                .outputs
                .iter()
                .filter(|output| before.output(&output.id).is_none())
                .map(|output| output.id.clone())
                .collect(),
            inputs: after
                .inputs
                .iter()
                .filter(|input| !input.is_monitor() && before.input(&input.id).is_none())
                .map(|input| input.id.clone())
                .collect(),
        }
    }

    /// `true` when nothing was plugged in.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty() && self.inputs.is_empty()
    }

    /// Adds the devices of `other` not listed yet: devices of one headset
    /// can come in two readings of the sound server.
    pub fn add(&mut self, other: &Self) {
        for id in &other.outputs {
            if !self.outputs.contains(id) {
                self.outputs.push(id.clone());
            }
        }
        for id in &other.inputs {
            if !self.inputs.contains(id) {
                self.inputs.push(id.clone());
            }
        }
    }
}

/// The part of a device id that names the device itself, the same for
/// its output and its microphone: `usb-Jabra-00` for
/// `alsa_output.usb-Jabra-00.analog-stereo` and
/// `alsa_input.usb-Jabra-00.mono-fallback`.
#[must_use]
pub fn device_key(id: &DeviceId) -> &str {
    let id = id.as_str();
    let rest = id.split_once('.').map_or(id, |(_, rest)| rest);
    rest.rsplit_once('.').map_or(rest, |(device, _)| device)
}

/// What DeskCheck changes after a device is plugged in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Switch {
    /// The output to use, a headset or headphones just plugged in.
    pub output: Option<DeviceId>,
    /// The microphone to use, the one of that headset.
    pub input: Option<DeviceId>,
    /// What was plugged in, for the words of the notification.
    pub form: Form,
}

impl Switch {
    /// `true` when nothing is to change.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.output.is_none() && self.input.is_none()
    }
}

/// What to use after `plugged` came in: a headset or headphones, and the
/// microphone of the same headset. `never` lists outputs never to use
/// (those of the place). Devices already in use are kept in the answer,
/// so DeskCheck can tell it even when the sound server switched first.
#[must_use]
pub fn switch_for(state: &AudioState, plugged: &Plugged, never: &[DeviceId]) -> Switch {
    let output = plugged.outputs.iter().find_map(|id| {
        let output = state.output(id)?;
        let wanted = matches!(output.hardware.form, Form::Headset | Form::Headphones);
        (wanted && !never.contains(id)).then_some(output)
    });
    let Some(output) = output else {
        let refused = plugged.outputs.iter().any(|id| never.contains(id));
        if refused {
            // A headset not used at this place: its microphone neither.
            return Switch::default();
        }
        // A headset seen only by its microphone (Bluetooth can show it
        // first): its microphone is used.
        let input = plugged.inputs.iter().find(|id| {
            state
                .input(id)
                .is_some_and(|input| input.hardware.form == Form::Headset)
        });
        return Switch {
            output: None,
            input: input.cloned(),
            form: Form::Headset,
        };
    };
    let key = device_key(&output.id);
    let input = plugged
        .inputs
        .iter()
        .chain(state.inputs.iter().map(|input| &input.id))
        .find(|id| {
            device_key(id) == key && state.input(id).is_some_and(|input| !input.is_monitor())
        })
        .cloned();
    Switch {
        output: Some(output.id.clone()),
        input,
        form: output.hardware.form,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{Connection, Hardware, Input, InputKind, Output, Volume};

    fn output(id: &str, form: Form) -> Output {
        Output {
            id: DeviceId::new(id),
            description: id.to_owned(),
            hardware: Hardware {
                form,
                connection: Connection::Usb,
            },
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    fn input(id: &str, form: Form, kind: InputKind) -> Input {
        Input {
            id: DeviceId::new(id),
            description: id.to_owned(),
            hardware: Hardware {
                form,
                connection: Connection::Usb,
            },
            kind,
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    fn desk() -> AudioState {
        AudioState {
            outputs: vec![output("alsa_output.pci-0.analog-stereo", Form::Speakers)],
            inputs: vec![input(
                "alsa_input.pci-0.analog-stereo",
                Form::Microphone,
                InputKind::Capture,
            )],
            default_output: Some(DeviceId::new("alsa_output.pci-0.analog-stereo")),
            default_input: Some(DeviceId::new("alsa_input.pci-0.analog-stereo")),
            ..AudioState::default()
        }
    }

    fn with_headset() -> AudioState {
        let mut state = desk();
        state.outputs.push(output(
            "alsa_output.usb-Jabra-00.analog-stereo",
            Form::Headset,
        ));
        state.inputs.push(input(
            "alsa_output.usb-Jabra-00.analog-stereo.monitor",
            Form::Headset,
            InputKind::Monitor {
                of: DeviceId::new("alsa_output.usb-Jabra-00.analog-stereo"),
            },
        ));
        state.inputs.push(input(
            "alsa_input.usb-Jabra-00.mono-fallback",
            Form::Headset,
            InputKind::Capture,
        ));
        state
    }

    #[test]
    fn the_output_and_the_microphone_of_a_device_share_a_key() {
        assert_eq!(
            device_key(&DeviceId::new("alsa_output.usb-Jabra-00.analog-stereo")),
            "usb-Jabra-00"
        );
        assert_eq!(
            device_key(&DeviceId::new("alsa_input.usb-Jabra-00.mono-fallback")),
            "usb-Jabra-00"
        );
        assert_eq!(
            device_key(&DeviceId::new("bluez_sink.00_1B_66.a2dp_sink")),
            "00_1B_66"
        );
        assert_eq!(device_key(&DeviceId::new("speakers")), "speakers");
    }

    #[test]
    fn a_headset_plugged_in_takes_the_sound_and_the_microphone() {
        let plugged = Plugged::between(&desk(), &with_headset());
        assert_eq!(plugged.outputs.len(), 1);
        // The monitor of the headset is not a microphone.
        assert_eq!(
            plugged.inputs,
            [DeviceId::new("alsa_input.usb-Jabra-00.mono-fallback")]
        );
        let switch = switch_for(&with_headset(), &plugged, &[]);
        assert_eq!(
            switch.output,
            Some(DeviceId::new("alsa_output.usb-Jabra-00.analog-stereo"))
        );
        assert_eq!(
            switch.input,
            Some(DeviceId::new("alsa_input.usb-Jabra-00.mono-fallback"))
        );
        assert_eq!(switch.form, Form::Headset);
        // Never at this place: nothing changes.
        let never = [DeviceId::new("alsa_output.usb-Jabra-00.analog-stereo")];
        assert!(switch_for(&with_headset(), &plugged, &never).is_empty());
    }

    #[test]
    fn a_screen_or_a_webcam_does_not_take_the_sound() {
        let mut after = desk();
        after
            .outputs
            .push(output("alsa_output.hdmi.hdmi-stereo", Form::Screen));
        after.inputs.push(input(
            "alsa_input.usb-Webcam.analog-stereo",
            Form::Webcam,
            InputKind::Capture,
        ));
        let plugged = Plugged::between(&desk(), &after);
        assert!(!plugged.is_empty());
        assert!(switch_for(&after, &plugged, &[]).is_empty());
    }

    #[test]
    fn devices_coming_in_two_readings_are_gathered() {
        let mut first = Plugged {
            outputs: vec![DeviceId::new("a")],
            inputs: Vec::new(),
        };
        first.add(&Plugged {
            outputs: vec![DeviceId::new("a")],
            inputs: vec![DeviceId::new("b")],
        });
        assert_eq!(first.outputs.len(), 1);
        assert_eq!(first.inputs, [DeviceId::new("b")]);
        assert!(Plugged::default().is_empty());
    }
}
