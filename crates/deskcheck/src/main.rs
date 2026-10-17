//! `deskcheck` command line tool.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod audio_report;
mod display_report;

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
            audio_report::render(state, audio_report::Options { verbose })
        }),
        Command::Displays { verbose } => show(deskcheck_mutter::read_state(), |state| {
            display_report::render(state, display_report::Options { verbose })
        }),
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
    fn a_command_is_required() {
        assert!(Cli::try_parse_from(["deskcheck"]).is_err());
    }
}
