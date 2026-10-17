//! `deskcheck` command line tool.

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use deskcheck_config::{Config, Store};

/// Prints like `print!`, but ignores errors: when the reader is gone, as in
/// `deskcheck audio | head -3`, there is nobody left to tell.
macro_rules! say {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = write!(std::io::stdout(), $($arg)*);
    }};
}

/// Prints a line like `println!`, but ignores errors (see `say!`).
macro_rules! sayln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

mod audio_report;
mod display_report;
mod doctor;
#[cfg(test)]
mod fixtures;
mod levels;
mod rename;
mod select;
mod sound;
mod sound_check;
mod watch;

/// See and fix your sound devices and screens.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show where the sound goes, which microphone hears you, and which
    /// applications use them.
    Audio {
        /// Also list every device with its technical name.
        #[arg(short, long)]
        verbose: bool,
    },
    /// List the screens that are on, with their number, name, resolution
    /// and scale, and say which one is primary.
    Displays {
        /// Also show each screen's identity, connector and position.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Say which screen the mouse is on. On Wayland, it needs the DeskCheck
    /// extension for GNOME Shell.
    Mouse,
    /// Find the sound problems and say how to fix them.
    ///
    /// Exit code: 0 when there is no problem, 1 when the sound server
    /// cannot be read, 2 when problems are found (or left after --fix).
    Doctor {
        /// Apply the fixes DeskCheck can do, then check again.
        #[arg(long)]
        fix: bool,
    },
    /// Print each change of the sound and the screens, live: plug in a
    /// headset or a screen to see it. Press Ctrl+C to stop.
    Watch,
    /// Show a live level bar for each microphone: speak, and the bar of
    /// the one that hears you moves. Press Ctrl+C to stop.
    Levels {
        /// Stop after this many seconds.
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// Test a device: play a short sound on an output, or record a few
    /// seconds from a microphone and play them back.
    Test {
        /// `output` or `input`.
        #[arg(value_enum)]
        kind: sound::Kind,
        /// Which one: `default`, the technical name, or a part of its name.
        #[arg(default_value = "default")]
        which: String,
        /// How long to record, from 1 to 10 seconds (input only).
        #[arg(long, default_value_t = sound_check::DEFAULT_SECONDS, value_parser = sound_check::parse_seconds)]
        seconds: u64,
    },
    /// Make an output or a microphone the default one.
    ///
    /// Apps follow the default, except those moved by hand with `move`.
    Use {
        /// `output` or `input`.
        #[arg(value_enum)]
        kind: sound::Kind,
        /// Which one: `default`, the technical name, or a part of its name.
        which: String,
    },
    /// Set the volume of an output or a microphone.
    Volume {
        /// `output` or `input`.
        #[arg(value_enum)]
        kind: sound::Kind,
        /// Which one: `default`, the technical name, or a part of its name.
        which: String,
        /// The volume, from 0% to 150%, like 80 or 80%.
        #[arg(value_parser = sound::parse_volume)]
        volume: deskcheck_core::audio::Volume,
    },
    /// Turn the sound of an output or a microphone off.
    Mute {
        /// `output` or `input`.
        #[arg(value_enum)]
        kind: sound::Kind,
        /// Which one: `default`, the technical name, or a part of its name.
        which: String,
    },
    /// Turn the sound of an output or a microphone back on.
    Unmute {
        /// `output` or `input`.
        #[arg(value_enum)]
        kind: sound::Kind,
        /// Which one: `default`, the technical name, or a part of its name.
        which: String,
    },
    /// Move what an app plays or records to another device, without
    /// stopping it.
    ///
    /// Example, for a call in Firefox: `deskcheck move recording firefox jabra`.
    Move {
        /// `playback` (to an output) or `recording` (from a microphone).
        #[arg(value_enum)]
        streams: sound::Streams,
        /// The app: a part of its name, like `firefox`.
        app: String,
        /// The device: `default`, the technical name, or a part of its name.
        which: String,
    },
    /// Give a name to a sound device or a screen, or remove it.
    ///
    /// The name is shown everywhere in DeskCheck. `--verbose` still shows
    /// the name from the system.
    Rename {
        #[command(subcommand)]
        what: RenameWhat,
    },
}

#[derive(Debug, Subcommand)]
enum RenameWhat {
    /// Rename an output (speakers, headset...).
    #[command(
        override_usage = "deskcheck rename output <WHICH> <NAME>\n       deskcheck rename output <WHICH> --reset"
    )]
    Output(RenameArgs),
    /// Rename an input (a microphone).
    #[command(
        override_usage = "deskcheck rename input <WHICH> <NAME>\n       deskcheck rename input <WHICH> --reset"
    )]
    Input(RenameArgs),
    /// Rename a screen.
    #[command(
        override_usage = "deskcheck rename screen <WHICH> <NAME>\n       deskcheck rename screen <WHICH> --reset"
    )]
    Screen(RenameArgs),
}

#[derive(Debug, Args)]
struct RenameArgs {
    /// Which one: `default`, a screen number from `deskcheck displays`, a
    /// connector like HDMI-1, the technical name, or a part of its name.
    which: String,
    #[command(flatten)]
    new: NewName,
}

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
struct NewName {
    /// The new name.
    name: Option<String>,
    /// Remove the given name and use the name from the system again.
    #[arg(long)]
    reset: bool,
}

/// Loads the settings for a report. A broken file is reported, and the
/// report goes on without the given names.
fn settings() -> Config {
    match Store::new().and_then(|store| store.load()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("deskcheck: {error}\ndeskcheck: given names are not used");
            Config::default()
        }
    }
}

/// Plans a sound command with `plan`, sends its actions, and returns the
/// sentence to print.
fn run_sound(
    plan: impl FnOnce(&Config, &deskcheck_core::audio::AudioState) -> Result<sound::Plan, select::Error>,
) -> Result<String, String> {
    let config = settings();
    let state = deskcheck_pulse::read_state().map_err(|error| error.to_string())?;
    let plan = plan(&config, &state).map_err(|error| error.to_string())?;
    for action in &plan.actions {
        deskcheck_pulse::apply(action).map_err(|error| error.to_string())?;
    }
    Ok(format!("{}\n", plan.done))
}

/// Runs `deskcheck watch`: one channel for the changes of both sources.
fn run_watch() -> Result<String, String> {
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Instant;

    use deskcheck_core::events::{Change, coalesce};

    let (changes, received) = std::sync::mpsc::channel();
    let _sound =
        deskcheck_pulse::SoundWatcher::start(changes.clone()).map_err(|error| error.to_string())?;
    let screens = deskcheck_mutter::ScreenWatcher::start(changes);
    match &screens {
        Ok(_) => sayln!("Watching the sound and the screens. Press Ctrl+C to stop."),
        Err(error) => {
            sayln!("Watching the sound. Press Ctrl+C to stop.");
            sayln!("The screens are not watched: {error}");
        }
    }
    let start = Instant::now();
    let mut last = watch::LastLines::default();
    loop {
        let first = received
            .recv()
            .map_err(|_| "both watches stopped".to_owned())?;
        // Gathers the burst that usually follows the first change.
        let mut burst = vec![first];
        let end = Instant::now() + watch::BURST;
        while let Some(wait) = end.checked_duration_since(Instant::now()) {
            match received.recv_timeout(wait) {
                Ok(change) => burst.push(change),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        let burst = coalesce(burst);
        let config = settings();
        let time = watch::elapsed(start.elapsed());
        if burst.iter().any(Change::is_sound) {
            match deskcheck_pulse::read_state() {
                Ok(state) => {
                    for change in burst.iter().filter(|c| c.is_sound()) {
                        let line = watch::sound_line(change, &config, &state);
                        if last.is_new(change, &line) {
                            sayln!("  {time}  {line}");
                        }
                    }
                }
                Err(error) => sayln!("  {time}  Cannot read the sound: {error}"),
            }
        }
        if burst.contains(&Change::Screens) {
            match deskcheck_mutter::read_state() {
                Ok(state) => {
                    let line = watch::screens_line(&config, &state);
                    if last.is_new(&Change::Screens, &line) {
                        sayln!("  {time}  {line}");
                    }
                }
                Err(error) => sayln!("  {time}  Cannot read the screens: {error}"),
            }
        }
        for change in &burst {
            if let Change::Lost { source, reason } = change {
                sayln!("  {time}  {}", watch::lost_line(source, reason));
            }
        }
    }
}

/// Runs `deskcheck levels`.
fn run_levels(seconds: Option<u64>) -> Result<String, String> {
    use std::io::{IsTerminal, Write as _};
    use std::time::{Duration, Instant};

    let config = settings();
    let state = deskcheck_pulse::read_state().map_err(|error| error.to_string())?;
    let mics: Vec<_> = state.microphones().collect();
    if mics.is_empty() {
        return Err("no microphone is plugged in".to_owned());
    }
    let ids: Vec<_> = mics.iter().map(|mic| mic.id.clone()).collect();
    let mut rows: Vec<levels::Row> = mics
        .iter()
        .map(|mic| {
            levels::Row::new(
                config.input_label(mic).into_owned(),
                state.default_input.as_ref() == Some(&mic.id),
            )
        })
        .collect();
    let meters = deskcheck_pulse::Meters::start(&ids).map_err(|error| error.to_string())?;

    let terminal = std::io::stdout().is_terminal();
    let frame_time = Duration::from_secs(1) / deskcheck_pulse::UPDATES_PER_SECOND;
    let deadline = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(
        stdout,
        "Speak: the bar of the microphone that hears you moves."
    );
    if deadline.is_none() {
        let _ = writeln!(stdout, "Press Ctrl+C to stop.");
    }
    let _ = writeln!(stdout);

    let mut drawn = 0;
    let mut last_print = Instant::now();
    while deadline.is_none_or(|deadline| Instant::now() < deadline) {
        // The loudest peak of each microphone during this frame.
        let mut peaks: Vec<Option<f32>> = vec![None; rows.len()];
        let frame_end = Instant::now() + frame_time;
        while let Some(wait) = frame_end.checked_duration_since(Instant::now()) {
            match meters.events().recv_timeout(wait) {
                Ok(deskcheck_pulse::Event::Peak { input, peak }) => {
                    if let Some(at) = ids.iter().position(|id| *id == input) {
                        peaks[at] = Some(peaks[at].map_or(peak, |p: f32| p.max(peak)));
                    }
                }
                Ok(deskcheck_pulse::Event::Lost(input)) => {
                    if let Some(at) = ids.iter().position(|id| *id == input) {
                        rows[at].lost = true;
                    }
                }
                Ok(deskcheck_pulse::Event::Failed(error)) => return Err(error.to_string()),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("the level meters stopped".to_owned());
                }
            }
        }
        for (row, peak) in rows.iter_mut().zip(peaks) {
            match peak {
                Some(peak) => row.bar.update(peak),
                None => row.bar.fall(),
            };
        }
        let frame = levels::render_frame(&rows);
        if terminal {
            let _ = write!(stdout, "{}", levels::redraw(drawn, &frame));
            drawn = rows.len();
        } else if last_print.elapsed() >= Duration::from_secs(1) {
            // Not a terminal: one frame per second, without moving the cursor.
            let _ = writeln!(stdout, "{frame}");
            last_print = Instant::now();
        }
        let _ = stdout.flush();
    }
    Ok(String::new())
}

/// Runs `deskcheck test output`.
fn test_output(
    config: &Config,
    state: &deskcheck_core::audio::AudioState,
    which: &str,
) -> Result<String, String> {
    let output = select::find_output(config, state, which).map_err(|error| error.to_string())?;
    let label = config.output_label(output);
    if let Some(warning) = sound_check::off_warning(
        sound::Kind::Output,
        label,
        output.muted,
        output.volume,
        which,
    ) {
        sayln!("{warning}");
    }
    sayln!("Playing a test sound on \"{label}\"...");
    let chime = deskcheck_core::audio::tone::chime(deskcheck_pulse::TEST_RATE);
    deskcheck_pulse::play(&output.id, &chime).map_err(|error| error.to_string())?;
    Ok("Done. If you heard nothing, look at the volume with `deskcheck audio`.\n".to_owned())
}

/// Runs `deskcheck test input`: records, says what was heard, plays back.
fn test_input(
    config: &Config,
    state: &deskcheck_core::audio::AudioState,
    which: &str,
    seconds: u64,
) -> Result<String, String> {
    use std::io::{IsTerminal, Write as _};

    let input = select::find_input(config, state, which).map_err(|error| error.to_string())?;
    let label = config.input_label(input);
    if let Some(warning) =
        sound_check::off_warning(sound::Kind::Input, &label, input.muted, input.volume, which)
    {
        sayln!("{warning}");
    }
    let unit = if seconds == 1 { "second" } else { "seconds" };
    sayln!("Recording {seconds} {unit} from \"{label}\". Speak now.");
    let terminal = std::io::stdout().is_terminal();
    let samples =
        deskcheck_pulse::record(&input.id, std::time::Duration::from_secs(seconds), |done| {
            if terminal {
                say!("\r  {}", sound_check::progress_bar(done));
                let _ = std::io::stdout().flush();
            }
        })
        .map_err(|error| error.to_string())?;
    if terminal {
        sayln!();
    }
    sayln!(
        "{}",
        sound_check::recording_verdict(deskcheck_core::audio::tone::loudest_db(&samples))
    );
    if deskcheck_core::audio::tone::is_silence(&samples) {
        return Ok("Nothing to play back.\n".to_owned());
    }
    let output = state
        .default_output_device()
        .ok_or_else(|| "there is no default output to play it back".to_owned())?;
    sayln!("Playing it back on \"{}\"...", config.output_label(output));
    deskcheck_pulse::play(&output.id, &samples).map_err(|error| error.to_string())?;
    Ok("Done.\n".to_owned())
}

/// Runs `deskcheck test`.
fn run_test(kind: sound::Kind, which: &str, seconds: u64) -> Result<String, String> {
    let config = settings();
    let state = deskcheck_pulse::read_state().map_err(|error| error.to_string())?;
    match kind {
        sound::Kind::Output => test_output(&config, &state, which),
        sound::Kind::Input => test_input(&config, &state, which, seconds),
    }
}

/// Exit code when problems are found.
const PROBLEMS_FOUND: u8 = 2;

/// Runs `deskcheck doctor`. Returns the report and whether problems are
/// left.
fn run_doctor(fix: bool) -> Result<(String, bool), String> {
    let config = settings();
    let state = deskcheck_pulse::read_state().map_err(|error| error.to_string())?;
    let findings = deskcheck_core::audio::diagnose(&state);
    if !fix || doctor::automatic_fixes(&findings).is_empty() {
        let report = doctor::render_check(&config, &state, &findings);
        return Ok((report, !findings.is_empty()));
    }
    let outcomes = doctor::fix_all(&findings, |fix| {
        let action = deskcheck_pulse::Action::from_fix(fix)
            .ok_or_else(|| "DeskCheck cannot do this fix itself.".to_owned())?;
        deskcheck_pulse::apply(&action).map_err(|error| error.to_string())
    });
    let after = deskcheck_pulse::read_state().map_err(|error| error.to_string())?;
    let left = deskcheck_core::audio::diagnose(&after);
    let report = doctor::render_fixed(&config, &state, &findings, &outcomes, &left);
    Ok((report, !left.is_empty()))
}

/// Renames, then saves the settings. The file is not touched when
/// anything fails, and a broken file is never overwritten.
fn run_rename(what: RenameWhat) -> Result<String, String> {
    let store = Store::new().map_err(|error| error.to_string())?;
    let before = store.load().map_err(|error| error.to_string())?;
    let mut config = before.clone();
    let message = match what {
        RenameWhat::Output(args) => {
            let state = deskcheck_pulse::read_state().map_err(|error| error.to_string())?;
            rename::output(&mut config, &state, &args.which, args.new.name.as_deref())
        }
        RenameWhat::Input(args) => {
            let state = deskcheck_pulse::read_state().map_err(|error| error.to_string())?;
            rename::input(&mut config, &state, &args.which, args.new.name.as_deref())
        }
        RenameWhat::Screen(args) => {
            let state = deskcheck_mutter::read_state().map_err(|error| error.to_string())?;
            rename::screen(&mut config, &state, &args.which, args.new.name.as_deref())
        }
    }
    .map_err(|error| error.to_string())?;
    if config == before {
        return Ok(format!("{message}\n"));
    }
    store.save(&config).map_err(|error| error.to_string())?;
    Ok(format!("{message}\nSaved in {}\n", store.path().display()))
}

/// Prints the report, or the error on stderr with a failure exit code.
/// `deskcheck mouse`: the screen under the mouse.
fn run_mouse() -> Result<String, String> {
    use deskcheck_mutter::pointer::{PointerReader, Session};
    let reader = PointerReader::new().map_err(|error| error.to_string())?;
    let (x, y) = reader.position().map_err(|error| error.to_string())?;
    let state = deskcheck_mutter::read_state().map_err(|error| error.to_string())?;
    let physical = reader.session() == Session::X11;
    let settings = settings();
    Ok(match state.screen_at(x, y, physical) {
        Some(screen) => {
            let number = state
                .numbered()
                .iter()
                .find(|(_, s)| s.id == screen.id)
                .map_or(0, |(number, _)| *number);
            format!(
                "The mouse is on screen {number}, {} (at {x}, {y}).\n",
                settings.screen_label(screen)
            )
        }
        None => format!("The mouse is at {x}, {y}, on no screen that is on.\n"),
    })
}

fn show<S, E: std::fmt::Display>(state: Result<S, E>, render: impl Fn(&S) -> String) -> ExitCode {
    match state {
        Ok(state) => {
            say!("{}", render(&state));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("deskcheck: {error}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Audio { verbose } => show(deskcheck_pulse::read_state(), |state| {
            audio_report::render(state, &settings(), audio_report::Options { verbose })
        }),
        Command::Displays { verbose } => show(deskcheck_mutter::read_state(), |state| {
            display_report::render(state, &settings(), display_report::Options { verbose })
        }),
        Command::Mouse => show(run_mouse(), String::clone),
        Command::Doctor { fix } => match run_doctor(fix) {
            Ok((report, problems_left)) => {
                say!("{report}");
                if problems_left {
                    ExitCode::from(PROBLEMS_FOUND)
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(error) => {
                eprintln!("deskcheck: {error}");
                ExitCode::FAILURE
            }
        },
        Command::Watch => show(run_watch(), String::clone),
        Command::Levels { seconds } => show(run_levels(seconds), String::clone),
        Command::Test {
            kind,
            which,
            seconds,
        } => show(run_test(kind, &which, seconds), String::clone),
        Command::Use { kind, which } => show(
            run_sound(|config, state| sound::use_device(kind, config, state, &which)),
            String::clone,
        ),
        Command::Volume {
            kind,
            which,
            volume,
        } => show(
            run_sound(|config, state| sound::set_volume(kind, config, state, &which, volume)),
            String::clone,
        ),
        Command::Mute { kind, which } => show(
            run_sound(|config, state| sound::set_mute(kind, config, state, &which, true)),
            String::clone,
        ),
        Command::Unmute { kind, which } => show(
            run_sound(|config, state| sound::set_mute(kind, config, state, &which, false)),
            String::clone,
        ),
        Command::Move {
            streams,
            app,
            which,
        } => show(
            run_sound(|config, state| sound::move_app(streams, config, state, &app, &which)),
            String::clone,
        ),
        Command::Rename { what } => show(run_rename(what), String::clone),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn audio_command_and_verbose_flag() {
        let cli = Cli::try_parse_from(["deskcheck", "audio", "-v"]).unwrap();
        assert!(matches!(cli.command, Command::Audio { verbose: true }));
        let cli = Cli::try_parse_from(["deskcheck", "audio"]).unwrap();
        assert!(matches!(cli.command, Command::Audio { verbose: false }));
    }

    #[test]
    fn mouse_command() {
        let cli = Cli::try_parse_from(["deskcheck", "mouse"]).unwrap();
        assert!(matches!(cli.command, Command::Mouse));
    }

    #[test]
    fn displays_command_and_verbose_flag() {
        let cli = Cli::try_parse_from(["deskcheck", "displays", "--verbose"]).unwrap();
        assert!(matches!(cli.command, Command::Displays { verbose: true }));
        let cli = Cli::try_parse_from(["deskcheck", "displays"]).unwrap();
        assert!(matches!(cli.command, Command::Displays { verbose: false }));
    }

    #[test]
    fn doctor_command_and_fix_flag() {
        let cli = Cli::try_parse_from(["deskcheck", "doctor"]).unwrap();
        assert!(matches!(cli.command, Command::Doctor { fix: false }));
        let cli = Cli::try_parse_from(["deskcheck", "doctor", "--fix"]).unwrap();
        assert!(matches!(cli.command, Command::Doctor { fix: true }));
    }

    #[test]
    fn levels_runs_until_stopped_or_for_some_seconds() {
        let cli = Cli::try_parse_from(["deskcheck", "levels"]).unwrap();
        assert!(matches!(cli.command, Command::Levels { seconds: None }));
        let cli = Cli::try_parse_from(["deskcheck", "levels", "--seconds", "5"]).unwrap();
        assert!(matches!(cli.command, Command::Levels { seconds: Some(5) }));
    }

    #[test]
    fn test_takes_a_device_and_seconds() {
        let cli = Cli::try_parse_from(["deskcheck", "test", "output"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Test { kind: sound::Kind::Output, ref which, seconds: 3 } if which == "default"
        ));
        let cli =
            Cli::try_parse_from(["deskcheck", "test", "input", "jabra", "--seconds", "5"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Test { kind: sound::Kind::Input, ref which, seconds: 5 } if which == "jabra"
        ));
        assert!(Cli::try_parse_from(["deskcheck", "test", "input", "--seconds", "60"]).is_err());
    }

    #[test]
    fn sound_commands_are_parsed() {
        let cli = Cli::try_parse_from(["deskcheck", "use", "input", "jabra"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Use { kind: sound::Kind::Input, ref which } if which == "jabra"
        ));
        let cli = Cli::try_parse_from(["deskcheck", "volume", "output", "default", "80%"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Volume { kind: sound::Kind::Output, volume, .. }
                if volume == deskcheck_core::audio::Volume::from_percent(80)
        ));
        let cli =
            Cli::try_parse_from(["deskcheck", "move", "recording", "firefox", "jabra"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Move { streams: sound::Streams::Recording, ref app, .. } if app == "firefox"
        ));
        assert!(Cli::try_parse_from(["deskcheck", "mute", "speakers", "default"]).is_err());
        assert!(Cli::try_parse_from(["deskcheck", "volume", "output", "default", "200"]).is_err());
    }

    #[test]
    fn rename_takes_a_name_or_reset() {
        let cli =
            Cli::try_parse_from(["deskcheck", "rename", "screen", "2", "Big screen"]).unwrap();
        let Command::Rename {
            what: RenameWhat::Screen(args),
        } = cli.command
        else {
            panic!("{cli:?}");
        };
        assert_eq!(args.which, "2");
        assert_eq!(args.new.name.as_deref(), Some("Big screen"));
        assert!(!args.new.reset);

        let cli =
            Cli::try_parse_from(["deskcheck", "rename", "output", "default", "--reset"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Rename {
                what: RenameWhat::Output(RenameArgs {
                    new: NewName {
                        name: None,
                        reset: true
                    },
                    ..
                })
            }
        ));
    }

    #[test]
    fn rename_refuses_both_or_neither() {
        use clap::error::ErrorKind;
        let neither = Cli::try_parse_from(["deskcheck", "rename", "input", "default"]).unwrap_err();
        assert_eq!(neither.kind(), ErrorKind::MissingRequiredArgument);
        let both =
            Cli::try_parse_from(["deskcheck", "rename", "input", "default", "Mic", "--reset"])
                .unwrap_err();
        assert_eq!(both.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn a_command_is_required() {
        assert!(Cli::try_parse_from(["deskcheck"]).is_err());
    }
}
