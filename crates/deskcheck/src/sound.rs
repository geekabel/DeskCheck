//! Commands that change the sound: `use`, `volume`, `mute`, `unmute` and
//! `move`.
//!
//! Each command is planned by a pure function: it finds what the user
//! means and returns the [`Action`]s to send, with the sentence to print
//! once they are done. The caller sends the actions, so the plans are
//! tested without a sound server.

use std::fmt::Write as _;

use deskcheck_config::Config;
use deskcheck_core::audio::{AudioState, DeviceId, StreamId, Target, Volume};
use deskcheck_pulse::Action;

use crate::select::{AppStreams, Error, find_app, find_input, find_output};

/// Output or input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    /// An output: speakers, headset...
    Output,
    /// An input: a microphone.
    Input,
}

/// Playback or recording streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Streams {
    /// What an app plays, moved to an output.
    Playback,
    /// What an app records, moved to a microphone.
    Recording,
}

/// What to send to the sound server, and what to say after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The changes, in order. Empty when there is nothing to do.
    pub actions: Vec<Action>,
    /// The sentence to print once the changes are done.
    pub done: String,
}

impl Plan {
    fn nothing(done: String) -> Self {
        Self {
            actions: Vec::new(),
            done,
        }
    }
}

/// Highest volume accepted, as in the settings of GNOME with
/// "over-amplification" on.
pub const MAX_VOLUME: u16 = 150;

/// Reads a volume like `80` or `80%`, from 0 to [`MAX_VOLUME`].
///
/// # Errors
///
/// Returns a message when the text is not a whole number in that range.
pub fn parse_volume(text: &str) -> Result<Volume, String> {
    let trimmed = text.trim();
    let digits = trimmed.strip_suffix('%').unwrap_or(trimmed).trim_end();
    let percent: u16 = digits
        .parse()
        .map_err(|_| format!("`{text}` is not a volume like 80 or 80%"))?;
    if percent > MAX_VOLUME {
        return Err(format!(
            "{percent}% is too loud: choose a volume from 0% to {MAX_VOLUME}%"
        ));
    }
    Ok(Volume::from_percent(percent))
}

/// A device found by kind: its target, name to show, and mute state.
struct Device {
    target: Target,
    label: String,
    muted: bool,
    is_default: bool,
}

fn find_device(
    kind: Kind,
    config: &Config,
    state: &AudioState,
    which: &str,
) -> Result<Device, Error> {
    match kind {
        Kind::Output => {
            let output = find_output(config, state, which)?;
            Ok(Device {
                target: Target::Output(output.id.clone()),
                label: config.output_label(output).to_owned(),
                muted: output.muted,
                is_default: state.default_output.as_ref() == Some(&output.id),
            })
        }
        Kind::Input => {
            let input = find_input(config, state, which)?;
            Ok(Device {
                target: Target::Input(input.id.clone()),
                label: config.input_label(input).into_owned(),
                muted: input.muted,
                is_default: state.default_input.as_ref() == Some(&input.id),
            })
        }
    }
}

fn title(kind: Kind) -> &'static str {
    match kind {
        Kind::Output => "Output",
        Kind::Input => "Input",
    }
}

fn word(kind: Kind) -> &'static str {
    match kind {
        Kind::Output => "output",
        Kind::Input => "input",
    }
}

/// `deskcheck use`: make a device the default one. Apps that were not
/// moved by hand follow the default.
///
/// # Errors
///
/// Returns an [`Error`] when `which` does not name exactly one device.
pub fn use_device(
    kind: Kind,
    config: &Config,
    state: &AudioState,
    which: &str,
) -> Result<Plan, Error> {
    let device = find_device(kind, config, state, which)?;
    let (title, label) = (title(kind), &device.label);
    if device.is_default {
        return Ok(Plan::nothing(format!(
            "{title} \"{label}\" is already the default."
        )));
    }
    let action = match device.target {
        Target::Output(id) => Action::SetDefaultOutput(id),
        Target::Input(id) => Action::SetDefaultInput(id),
    };
    Ok(Plan {
        actions: vec![action],
        done: format!("{title} \"{label}\" is now the default."),
    })
}

/// `deskcheck volume`: set the volume of a device.
///
/// # Errors
///
/// Returns an [`Error`] when `which` does not name exactly one device.
pub fn set_volume(
    kind: Kind,
    config: &Config,
    state: &AudioState,
    which: &str,
    volume: Volume,
) -> Result<Plan, Error> {
    let device = find_device(kind, config, state, which)?;
    let mut done = format!("{} \"{}\" is now at {volume}.", title(kind), device.label);
    if device.muted {
        // Quoted when it has spaces, so the command can be pasted as is.
        let which = if which.contains(char::is_whitespace) {
            format!("\"{which}\"")
        } else {
            which.to_owned()
        };
        let _ = write!(
            done,
            "\nIt is still muted: run `deskcheck unmute {} {which}` to hear it.",
            word(kind)
        );
    }
    Ok(Plan {
        actions: vec![Action::SetVolume(device.target, volume)],
        done,
    })
}

/// `deskcheck mute` (`mute` is `true`) and `deskcheck unmute`.
///
/// # Errors
///
/// Returns an [`Error`] when `which` does not name exactly one device.
pub fn set_mute(
    kind: Kind,
    config: &Config,
    state: &AudioState,
    which: &str,
    mute: bool,
) -> Result<Plan, Error> {
    let device = find_device(kind, config, state, which)?;
    let (title, label) = (title(kind), &device.label);
    match (mute, device.muted) {
        (true, true) => Ok(Plan::nothing(format!(
            "{title} \"{label}\" is already muted."
        ))),
        (false, false) => Ok(Plan::nothing(format!("{title} \"{label}\" is not muted."))),
        (true, false) => Ok(Plan {
            actions: vec![Action::SetMute(device.target, true)],
            done: format!("{title} \"{label}\" is now muted."),
        }),
        (false, true) => Ok(Plan {
            actions: vec![Action::SetMute(device.target, false)],
            done: format!("{title} \"{label}\" is on again."),
        }),
    }
}

/// The streams of each app, in the order they appear.
fn group_streams<'a>(
    streams: impl Iterator<Item = (StreamId, &'a str, Option<&'a str>, &'a DeviceId)>,
) -> (Vec<AppStreams<'a>>, Vec<(&'a str, StreamId, &'a DeviceId)>) {
    let mut apps: Vec<AppStreams> = Vec::new();
    let mut all = Vec::new();
    for (id, name, binary, device) in streams {
        match apps.iter_mut().find(|app| app.name == name) {
            Some(app) => app.streams += 1,
            None => apps.push(AppStreams {
                name,
                binary,
                streams: 1,
            }),
        }
        all.push((name, id, device));
    }
    (apps, all)
}

fn streams_label(name: &str, count: usize) -> String {
    if count > 1 {
        format!("{name} ({count} streams)")
    } else {
        name.to_owned()
    }
}

/// `deskcheck move`: move every stream of an app to another device,
/// without stopping it. Level meters cannot be moved and are left out.
///
/// # Errors
///
/// Returns an [`Error`] when `app` does not name exactly one app with
/// streams of that kind, or `which` does not name exactly one device.
pub fn move_app(
    streams: Streams,
    config: &Config,
    state: &AudioState,
    app: &str,
    which: &str,
) -> Result<Plan, Error> {
    let (apps, all) = match streams {
        Streams::Playback => group_streams(state.playbacks.iter().map(|p| {
            (
                p.id,
                p.app.name.as_str(),
                p.app.binary.as_deref(),
                &p.output,
            )
        })),
        Streams::Recording => group_streams(
            state
                .recordings
                .iter()
                .filter(|r| !r.level_meter)
                .map(|r| (r.id, r.app.name.as_str(), r.app.binary.as_deref(), &r.input)),
        ),
    };
    let kind = match streams {
        Streams::Playback => "playing app",
        Streams::Recording => "recording app",
    };
    let found = find_app(kind, app, &apps)?;
    let (to, label, verb) = match streams {
        Streams::Playback => {
            let output = find_output(config, state, which)?;
            (
                &output.id,
                config.output_label(output).to_owned(),
                "plays to",
            )
        }
        Streams::Recording => {
            let input = find_input(config, state, which)?;
            (
                &input.id,
                config.input_label(input).into_owned(),
                "records from",
            )
        }
    };
    let ours: Vec<_> = all
        .iter()
        .filter(|(name, _, _)| *name == found.name)
        .collect();
    let to_move: Vec<StreamId> = ours
        .iter()
        .filter(|(_, _, device)| *device != to)
        .map(|(_, id, _)| *id)
        .collect();
    let who = streams_label(found.name, ours.len());
    if to_move.is_empty() {
        return Ok(Plan::nothing(format!("{who} already {verb} \"{label}\".")));
    }
    let actions = to_move
        .into_iter()
        .map(|stream| match streams {
            Streams::Playback => Action::MovePlayback {
                stream,
                to: to.clone(),
            },
            Streams::Recording => Action::MoveRecording {
                stream,
                to: to.clone(),
            },
        })
        .collect();
    Ok(Plan {
        actions,
        done: format!("{who} now {verb} \"{label}\"."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{HEADSET, MIC, MONITOR, SPEAKERS, audio, device_input};
    use deskcheck_core::audio::{App, InputKind, Playback, Recording};

    const HEADSET_MIC: &str = "alsa_input.usb-GN_Netcom_A_S_Jabra_EVOLVE_LINK-00.mono-fallback";

    fn app(name: &str, binary: &str) -> App {
        App {
            name: name.to_owned(),
            binary: Some(binary.to_owned()),
            icon_name: None,
        }
    }

    fn playback(index: u32, app: App, output: &str) -> Playback {
        Playback {
            id: StreamId::new(index),
            app,
            media: None,
            output: DeviceId::new(output),
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    fn recording(index: u32, app: App, input: &str, level_meter: bool) -> Recording {
        Recording {
            id: StreamId::new(index),
            app,
            media: None,
            input: DeviceId::new(input),
            volume: Volume::NORMAL,
            muted: false,
            level_meter,
        }
    }

    /// A Teams call in Firefox at the office: the headset is plugged in,
    /// but Firefox still plays to the speakers and records the monitor.
    fn call() -> AudioState {
        let mut state = audio();
        state.inputs.push(device_input(
            HEADSET_MIC,
            "Jabra EVOLVE LINK Mono",
            InputKind::Capture,
        ));
        let firefox = app("Firefox", "firefox");
        state.playbacks = vec![
            playback(10, firefox.clone(), SPEAKERS),
            playback(11, firefox.clone(), SPEAKERS),
            playback(12, app("Spotify", "spotify"), HEADSET),
        ];
        state.recordings = vec![
            recording(20, firefox, MONITOR, false),
            recording(
                21,
                app("GNOME Settings", "gnome-control-center"),
                MONITOR,
                true,
            ),
        ];
        state
    }

    fn id(name: &str) -> DeviceId {
        DeviceId::new(name)
    }

    #[test]
    fn volumes_are_read_with_or_without_percent() {
        assert_eq!(parse_volume("80"), Ok(Volume::from_percent(80)));
        assert_eq!(parse_volume(" 80 % "), Ok(Volume::from_percent(80)));
        assert_eq!(parse_volume("0%"), Ok(Volume::from_percent(0)));
        assert_eq!(parse_volume("150"), Ok(Volume::from_percent(150)));
        assert!(parse_volume("151").unwrap_err().contains("too loud"));
        for bad in ["", "%", "loud", "-5", "8.5", "80%%"] {
            assert!(parse_volume(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn use_makes_a_device_the_default() {
        let config = Config::default();
        let plan = use_device(Kind::Output, &config, &call(), "jabra").unwrap();
        assert_eq!(plan.actions, [Action::SetDefaultOutput(id(HEADSET))]);
        assert_eq!(
            plan.done,
            "Output \"Jabra EVOLVE LINK Analog Stereo\" is now the default."
        );

        let plan = use_device(Kind::Input, &config, &call(), "jabra").unwrap();
        assert_eq!(plan.actions, [Action::SetDefaultInput(id(HEADSET_MIC))]);
    }

    #[test]
    fn use_says_when_it_is_already_the_default() {
        let plan = use_device(Kind::Input, &Config::default(), &call(), "default").unwrap();
        assert_eq!(plan.actions, []);
        assert_eq!(
            plan.done,
            "Input \"Built-in Audio Analog Stereo\" is already the default."
        );
    }

    #[test]
    fn a_monitor_cannot_be_chosen_as_input() {
        let error = use_device(Kind::Input, &Config::default(), &call(), "monitor").unwrap_err();
        assert!(matches!(error, Error::NotFound { .. }), "{error:?}");
    }

    #[test]
    fn volume_uses_the_given_name_and_warns_when_muted() {
        let mut config = Config::default();
        config.set_audio_name(&id(SPEAKERS), Some("Laptop speakers".to_owned()));
        let mut state = call();
        state.outputs[0].muted = true;
        let plan = set_volume(
            Kind::Output,
            &config,
            &state,
            "laptop",
            Volume::from_percent(80),
        )
        .unwrap();
        assert_eq!(
            plan.actions,
            [Action::SetVolume(
                Target::Output(id(SPEAKERS)),
                Volume::from_percent(80)
            )]
        );
        assert_eq!(
            plan.done,
            "Output \"Laptop speakers\" is now at 80%.\n\
             It is still muted: run `deskcheck unmute output laptop` to hear it."
        );
    }

    #[test]
    fn mute_and_unmute_only_when_needed() {
        let config = Config::default();
        let state = call();
        let plan = set_mute(Kind::Input, &config, &state, MIC, true).unwrap();
        assert_eq!(
            plan.actions,
            [Action::SetMute(Target::Input(id(MIC)), true)]
        );
        assert_eq!(
            plan.done,
            "Input \"Built-in Audio Analog Stereo\" is now muted."
        );

        let plan = set_mute(Kind::Input, &config, &state, MIC, false).unwrap();
        assert_eq!(plan.actions, []);
        assert_eq!(
            plan.done,
            "Input \"Built-in Audio Analog Stereo\" is not muted."
        );

        let mut muted = state;
        muted.inputs[1].muted = true;
        let plan = set_mute(Kind::Input, &config, &muted, MIC, false).unwrap();
        assert_eq!(
            plan.actions,
            [Action::SetMute(Target::Input(id(MIC)), false)]
        );
        assert_eq!(
            plan.done,
            "Input \"Built-in Audio Analog Stereo\" is on again."
        );
    }

    #[test]
    fn move_sends_every_stream_of_the_app() {
        let plan = move_app(
            Streams::Playback,
            &Config::default(),
            &call(),
            "firefox",
            "jabra",
        )
        .unwrap();
        assert_eq!(
            plan.actions,
            [
                Action::MovePlayback {
                    stream: StreamId::new(10),
                    to: id(HEADSET)
                },
                Action::MovePlayback {
                    stream: StreamId::new(11),
                    to: id(HEADSET)
                },
            ]
        );
        assert_eq!(
            plan.done,
            "Firefox (2 streams) now plays to \"Jabra EVOLVE LINK Analog Stereo\"."
        );
    }

    #[test]
    fn move_fixes_the_teams_case() {
        let plan = move_app(
            Streams::Recording,
            &Config::default(),
            &call(),
            "firefox",
            "jabra",
        )
        .unwrap();
        assert_eq!(
            plan.actions,
            [Action::MoveRecording {
                stream: StreamId::new(20),
                to: id(HEADSET_MIC)
            }]
        );
        assert_eq!(
            plan.done,
            "Firefox now records from \"Jabra EVOLVE LINK Mono\"."
        );
    }

    #[test]
    fn level_meters_are_not_moved() {
        let error = move_app(
            Streams::Recording,
            &Config::default(),
            &call(),
            "gnome",
            "jabra",
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "no recording app matches \"gnome\". Choose one of:\n  Firefox"
        );
    }

    #[test]
    fn move_says_when_nothing_needs_to_move() {
        let plan = move_app(
            Streams::Playback,
            &Config::default(),
            &call(),
            "spotify",
            "jabra",
        )
        .unwrap();
        assert_eq!(plan.actions, []);
        assert_eq!(
            plan.done,
            "Spotify already plays to \"Jabra EVOLVE LINK Analog Stereo\"."
        );
    }

    #[test]
    fn an_app_is_found_by_its_program_too() {
        let mut state = call();
        state.playbacks[2].app.name = "Music".to_owned();
        let plan = move_app(
            Streams::Playback,
            &Config::default(),
            &state,
            "spotify",
            "built-in",
        )
        .unwrap();
        assert_eq!(
            plan.actions,
            [Action::MovePlayback {
                stream: StreamId::new(12),
                to: id(SPEAKERS)
            }]
        );
    }
}
