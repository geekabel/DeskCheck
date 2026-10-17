//! Rules that find the common sound problems, with a fix for each.
//!
//! Each rule is a small pure function of an [`AudioState`]. [`diagnose`]
//! runs them all, in a fixed order: first what stops the others from
//! hearing you, then what stops you from hearing them.
//!
//! A [`Finding`] holds the [`Problem`] and, when there is one, the [`Fix`].
//! Most fixes can be applied by DeskCheck; [`Fix::PlugInMicrophone`] is
//! advice for the user.

use super::{AudioState, DeviceId, Form, Input, StreamId, Volume};

/// The device a fix acts on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// An output (sink).
    Output(DeviceId),
    /// An input (source).
    Input(DeviceId),
}

/// A problem found in the sound setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// An app records a monitor: in a call, the others hear a copy of an
    /// output, not the user.
    RecordsMonitor {
        /// The recording stream.
        stream: StreamId,
        /// Name of the app.
        app: String,
        /// The monitor it records.
        monitor: DeviceId,
    },
    /// The default input is a monitor. New recordings will hear the
    /// speakers, not the user.
    DefaultInputIsMonitor {
        /// The monitor.
        monitor: DeviceId,
    },
    /// No microphone is plugged in.
    NoMicrophone,
    /// A microphone in use (the default one, or one an app records) is
    /// muted.
    MicrophoneMuted {
        /// The microphone.
        input: DeviceId,
    },
    /// A microphone in use has its volume at 0.
    MicrophoneSilent {
        /// The microphone.
        input: DeviceId,
    },
    /// An output in use (the default one, or one an app plays to) is muted.
    OutputMuted {
        /// The output.
        output: DeviceId,
    },
    /// An output in use has its volume at 0.
    OutputSilent {
        /// The output.
        output: DeviceId,
    },
}

/// A way to fix a problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fix {
    /// Move a recording stream to another input, without stopping it.
    MoveRecording {
        /// The stream.
        stream: StreamId,
        /// The microphone to move it to.
        to: DeviceId,
    },
    /// Make this microphone the default input.
    SetDefaultInput(DeviceId),
    /// Turn the sound of a device back on.
    Unmute(Target),
    /// Set the volume of a device.
    SetVolume(Target, Volume),
    /// Only the user can fix it: plug in a headset or a microphone.
    PlugInMicrophone,
}

impl Fix {
    /// `true` when DeskCheck can apply this fix itself.
    #[must_use]
    pub fn is_automatic(&self) -> bool {
        !matches!(self, Self::PlugInMicrophone)
    }
}

/// A problem and its fix, if there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// What is wrong.
    pub problem: Problem,
    /// How to fix it.
    pub fix: Option<Fix>,
}

impl Finding {
    fn new(problem: Problem, fix: Option<Fix>) -> Self {
        Self { problem, fix }
    }
}

/// Rank of a microphone for calls: lower is better.
fn call_rank(input: &Input) -> u8 {
    match input.hardware.form {
        Form::Headset => 0,
        Form::Microphone => 1,
        Form::Webcam => 2,
        _ => 3,
    }
}

/// The microphone to suggest: the default input when it is a microphone,
/// else the best one for calls (a headset first), in list order.
#[must_use]
pub fn best_microphone(state: &AudioState) -> Option<&Input> {
    state
        .default_input_device()
        .filter(|input| !input.is_monitor())
        .or_else(|| state.microphones().min_by_key(|input| call_rank(input)))
}

/// Rule: an app records a monitor.
fn records_monitor(state: &AudioState) -> Vec<Finding> {
    let mic = best_microphone(state);
    state
        .recordings_from_monitors()
        .map(|(recording, monitor)| {
            Finding::new(
                Problem::RecordsMonitor {
                    stream: recording.id,
                    app: recording.app.name.clone(),
                    monitor: monitor.id.clone(),
                },
                mic.map(|mic| Fix::MoveRecording {
                    stream: recording.id,
                    to: mic.id.clone(),
                }),
            )
        })
        .collect()
}

/// Rule: the default input is a monitor.
fn default_input_is_monitor(state: &AudioState) -> Vec<Finding> {
    state
        .default_input_monitor()
        .map(|monitor| {
            Finding::new(
                Problem::DefaultInputIsMonitor {
                    monitor: monitor.id.clone(),
                },
                best_microphone(state).map(|mic| Fix::SetDefaultInput(mic.id.clone())),
            )
        })
        .into_iter()
        .collect()
}

/// Rule: no microphone at all.
fn no_microphone(state: &AudioState) -> Vec<Finding> {
    if state.microphones().next().is_some() {
        return Vec::new();
    }
    vec![Finding::new(
        Problem::NoMicrophone,
        Some(Fix::PlugInMicrophone),
    )]
}

/// Adds `id` to `ids` once, keeping the order.
fn push_once<'a>(ids: &mut Vec<&'a DeviceId>, id: &'a DeviceId) {
    if !ids.contains(&id) {
        ids.push(id);
    }
}

/// Microphones in use: the default input, then those apps record from.
fn microphones_in_use(state: &AudioState) -> Vec<&Input> {
    let mut ids = Vec::new();
    if let Some(id) = &state.default_input {
        push_once(&mut ids, id);
    }
    for recording in &state.recordings {
        push_once(&mut ids, &recording.input);
    }
    ids.into_iter()
        .filter_map(|id| state.input(id))
        .filter(|input| !input.is_monitor())
        .collect()
}

/// Rule: a microphone in use is muted, or at volume 0.
fn microphone_off(state: &AudioState) -> Vec<Finding> {
    microphones_in_use(state)
        .into_iter()
        .filter_map(|mic| {
            let target = Target::Input(mic.id.clone());
            if mic.muted {
                Some(Finding::new(
                    Problem::MicrophoneMuted {
                        input: mic.id.clone(),
                    },
                    Some(Fix::Unmute(target)),
                ))
            } else if mic.volume.is_silent() {
                Some(Finding::new(
                    Problem::MicrophoneSilent {
                        input: mic.id.clone(),
                    },
                    Some(Fix::SetVolume(target, Volume::NORMAL)),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Rule: an output in use is muted, or at volume 0.
fn output_off(state: &AudioState) -> Vec<Finding> {
    let mut ids = Vec::new();
    if let Some(id) = &state.default_output {
        push_once(&mut ids, id);
    }
    for playback in &state.playbacks {
        push_once(&mut ids, &playback.output);
    }
    ids.into_iter()
        .filter_map(|id| state.output(id))
        .filter_map(|output| {
            let target = Target::Output(output.id.clone());
            if output.muted {
                Some(Finding::new(
                    Problem::OutputMuted {
                        output: output.id.clone(),
                    },
                    Some(Fix::Unmute(target)),
                ))
            } else if output.volume.is_silent() {
                Some(Finding::new(
                    Problem::OutputSilent {
                        output: output.id.clone(),
                    },
                    Some(Fix::SetVolume(target, Volume::NORMAL)),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// All rules, in the order their findings are shown.
const RULES: [fn(&AudioState) -> Vec<Finding>; 5] = [
    records_monitor,
    default_input_is_monitor,
    no_microphone,
    microphone_off,
    output_off,
];

/// Runs every rule and returns what they found. An empty list means no
/// problem.
///
/// ```
/// use deskcheck_core::audio::{AudioState, Fix, Problem, diagnose};
///
/// // Nothing plugged in at all.
/// let findings = diagnose(&AudioState::default());
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].problem, Problem::NoMicrophone);
/// assert_eq!(findings[0].fix, Some(Fix::PlugInMicrophone));
/// ```
#[must_use]
pub fn diagnose(state: &AudioState) -> Vec<Finding> {
    RULES.iter().flat_map(|rule| rule(state)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{App, Connection, Hardware, InputKind, Output, Playback, Recording};

    const SPEAKERS: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo";
    const LAPTOP_MIC: &str = "alsa_input.pci-0000_00_1f.3.analog-stereo";
    const SPEAKERS_MONITOR: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor";
    const HEADSET_MIC: &str = "alsa_input.usb-Jabra-00.mono-fallback";
    const WEBCAM_MIC: &str = "alsa_input.usb-Logitech_C920-02.analog-stereo";

    fn hardware(form: Form) -> Hardware {
        Hardware {
            form,
            connection: Connection::Usb,
        }
    }

    fn mic(id: &str, form: Form) -> Input {
        Input {
            id: DeviceId::new(id),
            description: id.to_owned(),
            hardware: hardware(form),
            kind: InputKind::Capture,
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    fn recording(index: u32, input: &str) -> Recording {
        Recording {
            id: StreamId::new(index),
            app: App {
                name: "Firefox".to_owned(),
                ..App::default()
            },
            media: None,
            input: DeviceId::new(input),
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    /// Godwin's home setup, with nothing wrong.
    fn home() -> AudioState {
        AudioState {
            outputs: vec![Output {
                id: DeviceId::new(SPEAKERS),
                description: "Built-in Audio Analog Stereo".to_owned(),
                hardware: hardware(Form::Speakers),
                volume: Volume::from_percent(142),
                muted: false,
            }],
            inputs: vec![
                Input {
                    id: DeviceId::new(SPEAKERS_MONITOR),
                    description: "Monitor of Built-in Audio Analog Stereo".to_owned(),
                    hardware: hardware(Form::Speakers),
                    kind: InputKind::Monitor {
                        of: DeviceId::new(SPEAKERS),
                    },
                    volume: Volume::NORMAL,
                    muted: false,
                },
                mic(LAPTOP_MIC, Form::Microphone),
            ],
            default_output: Some(DeviceId::new(SPEAKERS)),
            default_input: Some(DeviceId::new(LAPTOP_MIC)),
            playbacks: vec![Playback {
                id: StreamId::new(626),
                app: App {
                    name: "Firefox".to_owned(),
                    ..App::default()
                },
                media: None,
                output: DeviceId::new(SPEAKERS),
                volume: Volume::NORMAL,
                muted: false,
            }],
            recordings: vec![],
        }
    }

    fn problems(state: &AudioState) -> Vec<Problem> {
        diagnose(state).into_iter().map(|f| f.problem).collect()
    }

    #[test]
    fn a_good_setup_has_no_problem() {
        assert_eq!(diagnose(&home()), []);
    }

    #[test]
    fn an_app_recording_a_monitor_is_moved_to_the_default_microphone() {
        let mut state = home();
        state.recordings.push(recording(42, SPEAKERS_MONITOR));
        assert_eq!(
            diagnose(&state),
            [Finding::new(
                Problem::RecordsMonitor {
                    stream: StreamId::new(42),
                    app: "Firefox".to_owned(),
                    monitor: DeviceId::new(SPEAKERS_MONITOR),
                },
                Some(Fix::MoveRecording {
                    stream: StreamId::new(42),
                    to: DeviceId::new(LAPTOP_MIC),
                }),
            )]
        );
    }

    #[test]
    fn a_monitor_as_default_input_is_replaced_by_the_best_microphone() {
        let mut state = home();
        state.inputs.push(mic(WEBCAM_MIC, Form::Webcam));
        state.inputs.push(mic(HEADSET_MIC, Form::Headset));
        state.default_input = Some(DeviceId::new(SPEAKERS_MONITOR));
        assert_eq!(
            diagnose(&state),
            [Finding::new(
                Problem::DefaultInputIsMonitor {
                    monitor: DeviceId::new(SPEAKERS_MONITOR),
                },
                Some(Fix::SetDefaultInput(DeviceId::new(HEADSET_MIC))),
            )]
        );
    }

    #[test]
    fn the_best_microphone_prefers_the_default_then_a_headset() {
        let mut state = home();
        state.inputs.push(mic(HEADSET_MIC, Form::Headset));
        assert_eq!(best_microphone(&state).unwrap().id.as_str(), LAPTOP_MIC);
        state.default_input = None;
        assert_eq!(best_microphone(&state).unwrap().id.as_str(), HEADSET_MIC);
        state.inputs.retain(Input::is_monitor);
        assert!(best_microphone(&state).is_none());
    }

    #[test]
    fn no_microphone_asks_to_plug_one_in() {
        let mut state = home();
        state.inputs.retain(Input::is_monitor);
        state.default_input = Some(DeviceId::new(SPEAKERS_MONITOR));
        state.recordings.push(recording(42, SPEAKERS_MONITOR));
        let findings = diagnose(&state);
        assert_eq!(
            problems(&state),
            [
                Problem::RecordsMonitor {
                    stream: StreamId::new(42),
                    app: "Firefox".to_owned(),
                    monitor: DeviceId::new(SPEAKERS_MONITOR),
                },
                Problem::DefaultInputIsMonitor {
                    monitor: DeviceId::new(SPEAKERS_MONITOR),
                },
                Problem::NoMicrophone,
            ]
        );
        // Nothing to move to or switch to: only the advice is left.
        assert_eq!(findings[0].fix, None);
        assert_eq!(findings[1].fix, None);
        assert_eq!(findings[2].fix, Some(Fix::PlugInMicrophone));
        assert!(!Fix::PlugInMicrophone.is_automatic());
    }

    #[test]
    fn a_muted_default_microphone_is_unmuted() {
        let mut state = home();
        state.inputs[1].muted = true;
        assert_eq!(
            diagnose(&state),
            [Finding::new(
                Problem::MicrophoneMuted {
                    input: DeviceId::new(LAPTOP_MIC),
                },
                Some(Fix::Unmute(Target::Input(DeviceId::new(LAPTOP_MIC)))),
            )]
        );
    }

    #[test]
    fn a_silent_microphone_an_app_records_is_set_to_normal() {
        let mut state = home();
        let mut headset = mic(HEADSET_MIC, Form::Headset);
        headset.volume = Volume::from_percent(0);
        state.inputs.push(headset);
        state.recordings.push(recording(42, HEADSET_MIC));
        assert_eq!(
            diagnose(&state),
            [Finding::new(
                Problem::MicrophoneSilent {
                    input: DeviceId::new(HEADSET_MIC),
                },
                Some(Fix::SetVolume(
                    Target::Input(DeviceId::new(HEADSET_MIC)),
                    Volume::NORMAL
                )),
            )]
        );
    }

    #[test]
    fn a_muted_and_silent_device_is_reported_once_as_muted() {
        let mut state = home();
        state.inputs[1].muted = true;
        state.inputs[1].volume = Volume::from_percent(0);
        state.recordings.push(recording(42, LAPTOP_MIC));
        assert_eq!(
            problems(&state),
            [Problem::MicrophoneMuted {
                input: DeviceId::new(LAPTOP_MIC),
            }]
        );
    }

    #[test]
    fn microphones_nobody_uses_are_ignored() {
        let mut state = home();
        let mut webcam = mic(WEBCAM_MIC, Form::Webcam);
        webcam.muted = true;
        state.inputs.push(webcam);
        assert_eq!(diagnose(&state), []);
    }

    #[test]
    fn a_muted_or_silent_output_in_use_is_reported() {
        let mut state = home();
        state.outputs[0].muted = true;
        assert_eq!(
            diagnose(&state),
            [Finding::new(
                Problem::OutputMuted {
                    output: DeviceId::new(SPEAKERS),
                },
                Some(Fix::Unmute(Target::Output(DeviceId::new(SPEAKERS)))),
            )]
        );

        state.outputs[0].muted = false;
        state.outputs[0].volume = Volume::from_percent(0);
        assert_eq!(
            diagnose(&state)[0].fix,
            Some(Fix::SetVolume(
                Target::Output(DeviceId::new(SPEAKERS)),
                Volume::NORMAL
            ))
        );
    }

    #[test]
    fn problems_come_in_rule_order() {
        let mut state = home();
        state.outputs[0].muted = true;
        state.inputs[1].muted = true;
        state.recordings.push(recording(42, SPEAKERS_MONITOR));
        let order: Vec<&str> = problems(&state)
            .iter()
            .map(|problem| match problem {
                Problem::RecordsMonitor { .. } => "records monitor",
                Problem::MicrophoneMuted { .. } => "mic muted",
                Problem::OutputMuted { .. } => "output muted",
                _ => "other",
            })
            .collect();
        assert_eq!(order, ["records monitor", "mic muted", "output muted"]);
    }
}
