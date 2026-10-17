//! Text report for `deskcheck audio`: who hears what.
//!
//! [`render`] is a pure function of the sound state, so the report is
//! tested without a sound server.

use std::fmt::Write as _;

use deskcheck_core::audio::{AudioState, Input, Output, Volume};

/// Options of the report.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Also show every device and the technical names.
    pub verbose: bool,
}

fn muted(muted: bool) -> &'static str {
    if muted { ", muted" } else { "" }
}

fn device_line(
    description: &str,
    hardware: impl std::fmt::Display,
    volume: Volume,
    is_muted: bool,
) -> String {
    format!("{description}  ({hardware}, {volume}{})", muted(is_muted))
}

fn output_line(output: &Output) -> String {
    device_line(
        &output.description,
        output.hardware,
        output.volume,
        output.muted,
    )
}

fn input_line(input: &Input) -> String {
    device_line(
        &input.description,
        input.hardware,
        input.volume,
        input.muted,
    )
}

/// Groups streams of the same app on the same device: `Firefox (3 streams)`.
fn group<'a>(items: impl Iterator<Item = (&'a str, String)>) -> Vec<(String, String)> {
    let mut groups: Vec<(&str, String, usize)> = Vec::new();
    for (app, device) in items {
        match groups
            .iter_mut()
            .find(|(a, d, _)| *a == app && *d == device)
        {
            Some(group) => group.2 += 1,
            None => groups.push((app, device, 1)),
        }
    }
    groups
        .into_iter()
        .map(|(app, device, count)| {
            let label = if count > 1 {
                format!("{app} ({count} streams)")
            } else {
                app.to_owned()
            };
            (label, device)
        })
        .collect()
}

/// Writes `label -> device` rows with aligned arrows, or `none`.
fn write_rows(out: &mut String, title: &str, rows: &[(String, String)], arrow: &str) {
    let _ = writeln!(out, "\n{title}");
    if rows.is_empty() {
        let _ = writeln!(out, "  none");
        return;
    }
    let width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    for (label, device) in rows {
        let _ = writeln!(out, "  {label:<width$}  {arrow} {device}");
    }
}

/// Name of a device for the stream lists: its description when known.
fn output_name(state: &AudioState, id: &deskcheck_core::audio::DeviceId) -> String {
    state
        .output(id)
        .map_or_else(|| id.to_string(), |output| output.description.clone())
}

fn input_name(state: &AudioState, id: &deskcheck_core::audio::DeviceId) -> String {
    state
        .input(id)
        .map_or_else(|| id.to_string(), |input| input.description.clone())
}

/// The problems this report can see, one line each, with a fix when there is one.
fn problems(state: &AudioState) -> Vec<String> {
    let best_microphone = state
        .default_input_device()
        .filter(|input| !input.is_monitor())
        .or_else(|| state.microphones().next());

    let mut problems = Vec::new();
    for (recording, monitor) in state.recordings_from_monitors() {
        let mut line = format!(
            "{} records \"{}\". This is a copy of an output, not a microphone: \
             in a call, nobody hears you.",
            recording.app.name, monitor.description
        );
        if let Some(mic) = best_microphone {
            let _ = write!(
                line,
                "\n    Fix: move {} to \"{}\".",
                recording.app.name, mic.description
            );
        }
        problems.push(line);
    }
    if let Some(monitor) = state.default_input_monitor() {
        let mut line = format!(
            "The default input is \"{}\", a copy of an output, not a microphone.",
            monitor.description
        );
        match best_microphone {
            Some(mic) => {
                let _ = write!(
                    line,
                    "\n    Fix: make \"{}\" the default input.",
                    mic.description
                );
            }
            None => line.push_str("\n    No microphone is plugged in."),
        }
        problems.push(line);
    }
    problems
}

/// Builds the report shown by `deskcheck audio`.
pub fn render(state: &AudioState, options: Options) -> String {
    let mut out = String::new();

    let hear = state
        .default_output_device()
        .map_or_else(|| "none".to_owned(), output_line);
    let heard = state
        .default_input_device()
        .map_or_else(|| "none".to_owned(), input_line);
    let _ = writeln!(out, "You hear through    {hear}");
    let _ = writeln!(out, "They hear you via   {heard}");

    let playing = group(
        state
            .playbacks
            .iter()
            .map(|p| (p.app.name.as_str(), output_name(state, &p.output))),
    );
    let recording = group(
        state
            .recordings
            .iter()
            .map(|r| (r.app.name.as_str(), input_name(state, &r.input))),
    );
    write_rows(&mut out, "Applications playing", &playing, "->");
    write_rows(&mut out, "Applications recording", &recording, "<-");

    if options.verbose {
        let _ = writeln!(out, "\nAll outputs");
        for output in &state.outputs {
            let _ = writeln!(out, "  {}\n    {}", output_line(output), output.id);
        }
        let _ = writeln!(out, "\nAll inputs");
        for input in &state.inputs {
            let kind = if input.is_monitor() {
                "monitor"
            } else {
                "microphone"
            };
            let _ = writeln!(out, "  {} [{kind}]\n    {}", input_line(input), input.id);
        }
    }

    let problems = problems(state);
    if problems.is_empty() {
        let _ = writeln!(out, "\nNo problem found.");
    } else {
        let _ = writeln!(out, "\nProblems");
        for problem in problems {
            let _ = writeln!(out, "  ! {problem}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{
        App, Connection, DeviceId, Form, Hardware, InputKind, Playback, Recording, StreamId,
    };

    const SPEAKERS: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo";
    const MIC: &str = "alsa_input.pci-0000_00_1f.3.analog-stereo";
    const MONITOR: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor";

    fn builtin(form: Form) -> Hardware {
        Hardware {
            form,
            connection: Connection::BuiltIn,
        }
    }

    fn app(name: &str) -> App {
        App {
            name: name.to_owned(),
            ..App::default()
        }
    }

    fn playback(index: u32, name: &str) -> Playback {
        Playback {
            id: StreamId::new(index),
            app: app(name),
            media: None,
            output: DeviceId::new(SPEAKERS),
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    /// Recorded on the real machine of Godwin, at home (17 Oct 2026),
    /// with `audio_dump`.
    fn home() -> AudioState {
        AudioState {
            outputs: vec![Output {
                id: DeviceId::new(SPEAKERS),
                description: "Built-in Audio Analog Stereo".to_owned(),
                hardware: builtin(Form::Speakers),
                volume: Volume::from_percent(142),
                muted: false,
            }],
            inputs: vec![
                Input {
                    id: DeviceId::new(MONITOR),
                    description: "Monitor of Built-in Audio Analog Stereo".to_owned(),
                    hardware: builtin(Form::Speakers),
                    kind: InputKind::Monitor {
                        of: DeviceId::new(SPEAKERS),
                    },
                    volume: Volume::NORMAL,
                    muted: false,
                },
                Input {
                    id: DeviceId::new(MIC),
                    description: "Built-in Audio Analog Stereo".to_owned(),
                    hardware: builtin(Form::Microphone),
                    kind: InputKind::Capture,
                    volume: Volume::NORMAL,
                    muted: false,
                },
            ],
            default_output: Some(DeviceId::new(SPEAKERS)),
            default_input: Some(DeviceId::new(MIC)),
            playbacks: vec![
                playback(290, "speech-dispatcher-dummy"),
                playback(626, "Firefox"),
                playback(627, "Firefox"),
                playback(1328, "Firefox"),
                playback(1363, "ALSA plug-in [fluidsynth]"),
            ],
            recordings: vec![],
        }
    }

    #[test]
    fn home_report() {
        let expected = "\
You hear through    Built-in Audio Analog Stereo  (speakers (built-in), 142%)
They hear you via   Built-in Audio Analog Stereo  (microphone (built-in), 100%)

Applications playing
  speech-dispatcher-dummy    -> Built-in Audio Analog Stereo
  Firefox (3 streams)        -> Built-in Audio Analog Stereo
  ALSA plug-in [fluidsynth]  -> Built-in Audio Analog Stereo

Applications recording
  none

No problem found.
";
        assert_eq!(render(&home(), Options::default()), expected);
    }

    #[test]
    fn an_app_recording_a_monitor_is_reported_with_a_fix() {
        let mut state = home();
        state.recordings.push(Recording {
            id: StreamId::new(42),
            app: app("Firefox"),
            media: None,
            input: DeviceId::new(MONITOR),
            volume: Volume::NORMAL,
            muted: false,
        });
        let report = render(&state, Options::default());
        assert!(report.contains(
            "  ! Firefox records \"Monitor of Built-in Audio Analog Stereo\". \
             This is a copy of an output, not a microphone: in a call, nobody hears you.\n    \
             Fix: move Firefox to \"Built-in Audio Analog Stereo\"."
        ));
        assert!(report.contains("Applications recording\n  Firefox  <- Monitor of Built-in"));
        assert!(!report.contains("No problem found."));
    }

    #[test]
    fn a_monitor_as_default_input_is_reported() {
        let mut state = home();
        state.default_input = Some(DeviceId::new(MONITOR));
        let report = render(&state, Options::default());
        assert!(report.contains("They hear you via   Monitor of Built-in Audio Analog Stereo"));
        assert!(report.contains(
            "  ! The default input is \"Monitor of Built-in Audio Analog Stereo\", \
             a copy of an output, not a microphone.\n    \
             Fix: make \"Built-in Audio Analog Stereo\" the default input."
        ));
    }

    #[test]
    fn no_microphone_at_all() {
        let mut state = home();
        state.inputs.retain(Input::is_monitor);
        state.default_input = Some(DeviceId::new(MONITOR));
        let report = render(&state, Options::default());
        assert!(report.contains("    No microphone is plugged in."));
    }

    #[test]
    fn verbose_lists_every_device_with_its_technical_name() {
        let report = render(&home(), Options { verbose: true });
        assert!(report.contains("All outputs\n  Built-in Audio Analog Stereo"));
        assert!(report.contains(&format!("    {SPEAKERS}\n")));
        assert!(report.contains("[monitor]"));
        assert!(report.contains(&format!("    {MIC}\n")));
    }

    #[test]
    fn empty_state() {
        let report = render(&AudioState::default(), Options::default());
        assert!(report.starts_with("You hear through    none\nThey hear you via   none\n"));
        assert!(report.ends_with("No problem found.\n"));
    }
}
