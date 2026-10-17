//! Text report for `deskcheck audio`: who hears what.
//!
//! [`render`] is a pure function of the sound state and the settings, so
//! the report is tested without a sound server. Devices show the name the
//! user gave them; `--verbose` also shows the name from the system.

use std::fmt::Write as _;

use deskcheck_config::Config;
use deskcheck_core::audio::{
    AudioState, DeviceId, Finding, Fix, Input, Output, Problem, Target, Volume, diagnose,
};

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

fn output_line(config: &Config, output: &Output) -> String {
    device_line(
        config.output_label(output),
        output.hardware,
        output.volume,
        output.muted,
    )
}

fn input_line(config: &Config, input: &Input) -> String {
    device_line(
        &config.input_label(input),
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

/// Name of a device for the stream lists, or its technical name when the
/// device is not known.
fn output_name(config: &Config, state: &AudioState, id: &DeviceId) -> String {
    state.output(id).map_or_else(
        || id.to_string(),
        |output| config.output_label(output).to_owned(),
    )
}

fn input_name(config: &Config, state: &AudioState, id: &DeviceId) -> String {
    state.input(id).map_or_else(
        || id.to_string(),
        |input| config.input_label(input).into_owned(),
    )
}

/// Second line of a device in verbose mode: the technical name, and the
/// name from the system when the user gave another one.
fn technical(config: &Config, id: &DeviceId, description: &str) -> String {
    if config.audio_name(id).is_some() {
        format!("{description}, {id}")
    } else {
        id.to_string()
    }
}

/// Name to show for a device a finding names, or its technical name when
/// it is gone.
fn device_name(config: &Config, state: &AudioState, id: &DeviceId) -> String {
    if let Some(output) = state.output(id) {
        return config.output_label(output).to_owned();
    }
    state.input(id).map_or_else(
        || id.to_string(),
        |input| config.input_label(input).into_owned(),
    )
}

fn target_name(config: &Config, state: &AudioState, target: &Target) -> String {
    match target {
        Target::Output(id) | Target::Input(id) => device_name(config, state, id),
    }
}

/// The problem, in one sentence.
pub(crate) fn problem_text(config: &Config, state: &AudioState, problem: &Problem) -> String {
    let name = |id: &DeviceId| device_name(config, state, id);
    match problem {
        Problem::RecordsMonitor { app, monitor, .. } => format!(
            "{app} records \"{}\". This is a copy of an output, not a microphone: \
             in a call, nobody hears you.",
            name(monitor)
        ),
        Problem::DefaultInputIsMonitor { monitor } => format!(
            "The default input is \"{}\", a copy of an output, not a microphone.",
            name(monitor)
        ),
        Problem::NoMicrophone => "No microphone is plugged in.".to_owned(),
        Problem::MicrophoneMuted { input } => {
            format!(
                "The microphone \"{}\" is muted: nobody hears you.",
                name(input)
            )
        }
        Problem::MicrophoneSilent { input } => {
            format!(
                "The microphone \"{}\" is at 0%: nobody hears you.",
                name(input)
            )
        }
        Problem::OutputMuted { output } => {
            format!(
                "The output \"{}\" is muted: you hear nothing.",
                name(output)
            )
        }
        Problem::OutputSilent { output } => {
            format!(
                "The output \"{}\" is at 0%: you hear nothing.",
                name(output)
            )
        }
    }
}

/// The fix, as an instruction.
pub(crate) fn fix_text(config: &Config, state: &AudioState, fix: &Fix) -> String {
    match fix {
        Fix::MoveRecording { stream, to } => {
            let app = state
                .recordings
                .iter()
                .find(|recording| recording.id == *stream)
                .map_or_else(|| format!("stream {stream}"), |r| r.app.name.clone());
            format!("move {app} to \"{}\".", device_name(config, state, to))
        }
        Fix::SetDefaultInput(id) => format!(
            "make \"{}\" the default input.",
            device_name(config, state, id)
        ),
        Fix::Unmute(target) => format!(
            "turn the sound of \"{}\" back on.",
            target_name(config, state, target)
        ),
        Fix::SetVolume(target, volume) => format!(
            "set \"{}\" to {volume}.",
            target_name(config, state, target)
        ),
        Fix::PlugInMicrophone => "plug in a headset or a microphone.".to_owned(),
    }
}

/// A finding as text: the problem, then the fix on its own line.
pub fn describe(config: &Config, state: &AudioState, finding: &Finding) -> String {
    let mut text = problem_text(config, state, &finding.problem);
    if let Some(fix) = &finding.fix {
        let _ = write!(text, "\n    Fix: {}", fix_text(config, state, fix));
    }
    text
}

/// Builds the report shown by `deskcheck audio`.
pub fn render(state: &AudioState, config: &Config, options: Options) -> String {
    let mut out = String::new();

    let hear = state
        .default_output_device()
        .map_or_else(|| "none".to_owned(), |output| output_line(config, output));
    let heard = state
        .default_input_device()
        .map_or_else(|| "none".to_owned(), |input| input_line(config, input));
    let _ = writeln!(out, "You hear through    {hear}");
    let _ = writeln!(out, "They hear you via   {heard}");

    let playing = group(
        state
            .playbacks
            .iter()
            .map(|p| (p.app.name.as_str(), output_name(config, state, &p.output))),
    );
    let recording = group(
        state
            .recordings
            .iter()
            .map(|r| (r.app.name.as_str(), input_name(config, state, &r.input))),
    );
    write_rows(&mut out, "Applications playing", &playing, "->");
    write_rows(&mut out, "Applications recording", &recording, "<-");

    if options.verbose {
        let _ = writeln!(out, "\nAll outputs");
        for output in &state.outputs {
            let _ = writeln!(
                out,
                "  {}\n    {}",
                output_line(config, output),
                technical(config, &output.id, &output.description)
            );
        }
        let _ = writeln!(out, "\nAll inputs");
        for input in &state.inputs {
            let kind = if input.is_monitor() {
                "monitor"
            } else {
                "microphone"
            };
            let _ = writeln!(
                out,
                "  {} [{kind}]\n    {}",
                input_line(config, input),
                technical(config, &input.id, &input.description)
            );
        }
    }

    let findings = diagnose(state);
    if findings.is_empty() {
        let _ = writeln!(out, "\nNo problem found.");
    } else {
        let _ = writeln!(out, "\nProblems");
        for finding in &findings {
            let _ = writeln!(out, "  ! {}", describe(config, state, finding));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{
        App, Connection, Form, Hardware, InputKind, Playback, Recording, StreamId,
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
        assert_eq!(
            render(&home(), &Config::default(), Options::default()),
            expected
        );
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
        let report = render(&state, &Config::default(), Options::default());
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
        let report = render(&state, &Config::default(), Options::default());
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
        let report = render(&state, &Config::default(), Options::default());
        assert!(report.ends_with(
            "  ! No microphone is plugged in.\n    Fix: plug in a headset or a microphone.\n"
        ));
    }

    #[test]
    fn verbose_lists_every_device_with_its_technical_name() {
        let report = render(&home(), &Config::default(), Options { verbose: true });
        assert!(report.contains("All outputs\n  Built-in Audio Analog Stereo"));
        assert!(report.contains(&format!("    {SPEAKERS}\n")));
        assert!(report.contains("[monitor]"));
        assert!(report.contains(&format!("    {MIC}\n")));
    }

    #[test]
    fn empty_state() {
        let report = render(
            &AudioState::default(),
            &Config::default(),
            Options::default(),
        );
        assert!(report.starts_with("You hear through    none\nThey hear you via   none\n"));
        assert!(report.ends_with("Problems\n  ! No microphone is plugged in.\n    Fix: plug in a headset or a microphone.\n"));
    }

    fn renamed() -> Config {
        let mut config = Config::default();
        config.set_audio_name(&DeviceId::new(SPEAKERS), Some("Laptop speakers".to_owned()));
        config.set_audio_name(&DeviceId::new(MIC), Some("Laptop mic".to_owned()));
        config
    }

    #[test]
    fn given_names_are_shown_everywhere() {
        let mut state = home();
        state.recordings.push(Recording {
            id: StreamId::new(42),
            app: app("Firefox"),
            media: None,
            input: DeviceId::new(MONITOR),
            volume: Volume::NORMAL,
            muted: false,
        });
        let report = render(&state, &renamed(), Options::default());
        assert!(report.starts_with(
            "You hear through    Laptop speakers  (speakers (built-in), 142%)\n\
             They hear you via   Laptop mic  (microphone (built-in), 100%)\n"
        ));
        assert!(report.contains("  Firefox (3 streams)        -> Laptop speakers\n"));
        assert!(report.contains("  Firefox  <- Monitor of Laptop speakers\n"));
        assert!(report.contains(
            "  ! Firefox records \"Monitor of Laptop speakers\". \
             This is a copy of an output, not a microphone: in a call, nobody hears you.\n    \
             Fix: move Firefox to \"Laptop mic\"."
        ));
        assert!(!report.contains("Built-in Audio"));
    }

    #[test]
    fn verbose_keeps_the_system_name_of_a_renamed_device() {
        let report = render(&home(), &renamed(), Options { verbose: true });
        assert!(report.contains(&format!(
            "All outputs\n  Laptop speakers  (speakers (built-in), 142%)\n    \
             Built-in Audio Analog Stereo, {SPEAKERS}\n"
        )));
        // The monitor has no name of its own: only its technical name.
        assert!(report.contains(&format!(
            "  Monitor of Laptop speakers  (speakers (built-in), 100%) [monitor]\n    {MONITOR}\n"
        )));
    }

    #[test]
    fn muted_devices_are_reported_with_their_given_names() {
        let mut state = home();
        state.outputs[0].muted = true;
        state.inputs[1].volume = Volume::from_percent(0);
        let report = render(&state, &renamed(), Options::default());
        assert!(
            report.ends_with(
                "Problems\n\
             \x20 ! The microphone \"Laptop mic\" is at 0%: nobody hears you.\n\
             \x20   Fix: set \"Laptop mic\" to 100%.\n\
             \x20 ! The output \"Laptop speakers\" is muted: you hear nothing.\n\
             \x20   Fix: turn the sound of \"Laptop speakers\" back on.\n"
            ),
            "{report}"
        );
    }
}
