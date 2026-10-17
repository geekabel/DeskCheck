//! `deskcheck` command line tool.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod audio_report;

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Audio { verbose } => match deskcheck_pulse::read_state() {
            Ok(state) => {
                print!(
                    "{}",
                    audio_report::render(&state, audio_report::Options { verbose })
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("deskcheck: {error}");
                ExitCode::FAILURE
            }
        },
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
    fn a_command_is_required() {
        assert!(Cli::try_parse_from(["deskcheck"]).is_err());
    }
}
