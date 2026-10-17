//! Conversion from the raw sound server data to the DeskCheck model.

use std::collections::HashMap;

use deskcheck_core::audio::{
    App, AudioState, DeviceId, Input, InputKind, Output, Playback, Recording, StreamId, Volume,
};

use libpulse_binding::volume::Volume as PaVolume;

use crate::raw::{Snapshot, StreamProps};

/// Name used when a stream says nothing about its application.
const UNKNOWN_APP: &str = "Unknown application";

/// Converts a sound server volume to a percentage, rounded to the nearest unit.
/// The normal volume of the sound server (`PA_VOLUME_NORM`) is 100 %.
pub(crate) fn volume_from_raw(raw: u32) -> Volume {
    let norm = u64::from(PaVolume::NORMAL.0);
    let percent = (u64::from(raw) * 100 + norm / 2) / norm;
    Volume::from_percent(u16::try_from(percent).unwrap_or(u16::MAX))
}

/// Builds the application from the stream properties.
fn app(props: &StreamProps) -> App {
    let name = props
        .app_name
        .clone()
        .or_else(|| props.binary.clone())
        .or_else(|| props.stream_name.clone())
        .unwrap_or_else(|| UNKNOWN_APP.to_owned());
    App {
        name,
        binary: props.binary.clone(),
        icon_name: props.icon_name.clone(),
    }
}

/// Converts a snapshot to the DeskCheck model.
///
/// Streams are linked to devices by index in the sound server, and to
/// devices by name in DeskCheck. A stream whose device is not in the
/// snapshot (it was unplugged while reading) is left out.
pub(crate) fn audio_state(snapshot: &Snapshot) -> AudioState {
    let sink_names: HashMap<u32, &str> = snapshot
        .sinks
        .iter()
        .map(|sink| (sink.index, sink.name.as_str()))
        .collect();
    let source_names: HashMap<u32, &str> = snapshot
        .sources
        .iter()
        .map(|source| (source.index, source.name.as_str()))
        .collect();

    let outputs = snapshot
        .sinks
        .iter()
        .map(|sink| Output {
            id: DeviceId::new(sink.name.as_str()),
            description: sink.description.clone(),
            volume: volume_from_raw(sink.volume),
            muted: sink.mute,
        })
        .collect();

    let inputs = snapshot
        .sources
        .iter()
        .map(|source| {
            let monitored = source.monitor_of_sink_name.clone().or_else(|| {
                source
                    .monitor_of_sink
                    .and_then(|index| sink_names.get(&index))
                    .map(|name| (*name).to_owned())
            });
            let kind = match monitored {
                Some(sink) => InputKind::Monitor {
                    of: DeviceId::new(sink),
                },
                // A monitor of a sink we did not get: it stays a monitor, so it
                // is never taken for a microphone.
                None if source.monitor_of_sink.is_some() => InputKind::Monitor {
                    of: DeviceId::new(""),
                },
                None => InputKind::Capture,
            };
            Input {
                id: DeviceId::new(source.name.as_str()),
                description: source.description.clone(),
                kind,
                volume: volume_from_raw(source.volume),
                muted: source.mute,
            }
        })
        .collect();

    let playbacks = snapshot
        .sink_inputs
        .iter()
        .filter_map(|stream| {
            let sink = sink_names.get(&stream.sink)?;
            Some(Playback {
                id: StreamId::new(stream.index),
                app: app(&stream.props),
                media: stream.props.media_name.clone(),
                output: DeviceId::new(*sink),
                volume: volume_from_raw(stream.volume),
                muted: stream.mute,
            })
        })
        .collect();

    let recordings = snapshot
        .source_outputs
        .iter()
        .filter_map(|stream| {
            let source = source_names.get(&stream.source)?;
            Some(Recording {
                id: StreamId::new(stream.index),
                app: app(&stream.props),
                media: stream.props.media_name.clone(),
                input: DeviceId::new(*source),
                volume: volume_from_raw(stream.volume),
                muted: stream.mute,
            })
        })
        .collect();

    AudioState {
        outputs,
        inputs,
        default_output: snapshot.default_sink.as_deref().map(DeviceId::new),
        default_input: snapshot.default_source.as_deref().map(DeviceId::new),
        playbacks,
        recordings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::{Sink, SinkInput, Source, SourceOutput};

    const NORM: u32 = PaVolume::NORMAL.0;

    /// The office case, as PulseAudio reports it: Firefox records from the
    /// monitor of the HDMI output of the Dell screen.
    fn office() -> Snapshot {
        let firefox = StreamProps {
            app_name: Some("Firefox".to_owned()),
            binary: Some("firefox".to_owned()),
            icon_name: Some("firefox".to_owned()),
            media_name: Some("AudioStream".to_owned()),
            stream_name: Some("AudioStream".to_owned()),
        };
        Snapshot {
            default_sink: Some("alsa_output.usb-Headset-00.analog-stereo".to_owned()),
            default_source: Some("alsa_input.usb-Headset-00.mono-fallback".to_owned()),
            sinks: vec![
                Sink {
                    index: 1,
                    name: "alsa_output.usb-Headset-00.analog-stereo".to_owned(),
                    description: "USB Headset Analog Stereo".to_owned(),
                    volume: NORM / 2,
                    mute: false,
                },
                Sink {
                    index: 2,
                    name: "alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1".to_owned(),
                    description: "HDMI / DisplayPort 2".to_owned(),
                    volume: NORM,
                    mute: false,
                },
            ],
            sources: vec![
                Source {
                    index: 3,
                    name: "alsa_input.usb-Headset-00.mono-fallback".to_owned(),
                    description: "USB Headset Mono".to_owned(),
                    monitor_of_sink: None,
                    monitor_of_sink_name: None,
                    volume: NORM,
                    mute: false,
                },
                Source {
                    index: 4,
                    name: "alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1.monitor".to_owned(),
                    description: "Monitor of HDMI / DisplayPort 2".to_owned(),
                    monitor_of_sink: Some(2),
                    monitor_of_sink_name: Some(
                        "alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1".to_owned(),
                    ),
                    volume: NORM,
                    mute: false,
                },
            ],
            sink_inputs: vec![SinkInput {
                index: 10,
                sink: 1,
                props: firefox.clone(),
                volume: NORM,
                mute: false,
            }],
            source_outputs: vec![SourceOutput {
                index: 42,
                source: 4,
                props: firefox,
                volume: NORM,
                mute: false,
            }],
        }
    }

    #[test]
    fn volume_conversion_rounds_to_percent() {
        assert_eq!(volume_from_raw(0).percent(), 0);
        assert_eq!(volume_from_raw(NORM).percent(), 100);
        assert_eq!(volume_from_raw(NORM / 2).percent(), 50);
        assert_eq!(volume_from_raw(NORM * 3 / 2).percent(), 150);
        // 0.625 % rounds up, 0.3 % rounds down.
        assert_eq!(volume_from_raw(410).percent(), 1);
        assert_eq!(volume_from_raw(200).percent(), 0);
        // Far above any real volume: capped instead of wrapping around.
        assert_eq!(volume_from_raw(u32::MAX).percent(), u16::MAX);
    }

    #[test]
    fn devices_and_defaults_are_converted() {
        let state = audio_state(&office());
        assert_eq!(state.outputs.len(), 2);
        assert_eq!(state.inputs.len(), 2);
        assert_eq!(
            state.default_output_device().unwrap().description,
            "USB Headset Analog Stereo"
        );
        assert_eq!(
            state.default_input_device().unwrap().description,
            "USB Headset Mono"
        );
        assert_eq!(state.outputs[0].volume.percent(), 50);
    }

    #[test]
    fn monitor_source_becomes_a_monitor_input() {
        let state = audio_state(&office());
        let monitor = &state.inputs[1];
        assert_eq!(
            monitor.kind,
            InputKind::Monitor {
                of: DeviceId::new("alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1")
            }
        );
        assert_eq!(state.microphones().count(), 1);
    }

    #[test]
    fn monitor_without_sink_name_uses_the_sink_index() {
        let mut snapshot = office();
        snapshot.sources[1].monitor_of_sink_name = None;
        let state = audio_state(&snapshot);
        assert_eq!(
            state.inputs[1].kind,
            InputKind::Monitor {
                of: DeviceId::new("alsa_output.pci-0000_00_1f.3.hdmi-stereo-extra1")
            }
        );
    }

    #[test]
    fn monitor_of_an_unknown_sink_stays_a_monitor() {
        let mut snapshot = office();
        snapshot.sources[1].monitor_of_sink_name = None;
        snapshot.sources[1].monitor_of_sink = Some(99);
        let state = audio_state(&snapshot);
        assert!(state.inputs[1].is_monitor());
        assert_eq!(state.microphones().count(), 1);
    }

    #[test]
    fn streams_are_linked_to_devices_by_name() {
        let state = audio_state(&office());
        assert_eq!(state.playbacks.len(), 1);
        assert_eq!(
            state.playbacks[0].output.as_str(),
            "alsa_output.usb-Headset-00.analog-stereo"
        );
        assert_eq!(state.recordings.len(), 1);
        let recording = &state.recordings[0];
        assert_eq!(recording.id, StreamId::new(42));
        assert_eq!(recording.app.name, "Firefox");
        assert!(state.input(&recording.input).unwrap().is_monitor());
    }

    #[test]
    fn stream_on_a_missing_device_is_left_out() {
        let mut snapshot = office();
        snapshot.source_outputs[0].source = 99;
        assert_eq!(audio_state(&snapshot).recordings, [] as [Recording; 0]);
    }

    #[test]
    fn app_name_falls_back_to_binary_then_stream_name() {
        let mut props = StreamProps {
            binary: Some("parec".to_owned()),
            stream_name: Some("record".to_owned()),
            ..StreamProps::default()
        };
        assert_eq!(app(&props).name, "parec");
        props.binary = None;
        assert_eq!(app(&props).name, "record");
        assert_eq!(app(&StreamProps::default()).name, UNKNOWN_APP);
    }

    #[test]
    fn empty_snapshot_gives_empty_state() {
        assert_eq!(audio_state(&Snapshot::default()), AudioState::default());
    }
}
