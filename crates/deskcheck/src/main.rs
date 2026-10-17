//! `deskcheck` command line tool.

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use deskcheck_config::{Config, Store};

mod audio_report;
mod display_report;
mod rename;

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
fn show<S, E: std::fmt::Display>(state: Result<S, E>, render: impl Fn(&S) -> String) -> ExitCode {
    match state {
        Ok(state) => {
            print!("{}", render(&state));
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
    fn displays_command_and_verbose_flag() {
        let cli = Cli::try_parse_from(["deskcheck", "displays", "--verbose"]).unwrap();
        assert!(matches!(cli.command, Command::Displays { verbose: true }));
        let cli = Cli::try_parse_from(["deskcheck", "displays"]).unwrap();
        assert!(matches!(cli.command, Command::Displays { verbose: false }));
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
