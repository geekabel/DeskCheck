//! Prints what the sound server reports, to check DeskCheck on a real machine.
//!
//! ```sh
//! cargo run -p deskcheck-pulse --example audio_dump
//! ```

use std::process::ExitCode;

use deskcheck_core::audio::{AudioState, DeviceId, InputKind};

fn mark(state_default: Option<&DeviceId>, id: &DeviceId) -> &'static str {
    if state_default == Some(id) { "*" } else { " " }
}

fn print(state: &AudioState) {
    println!("Outputs (* = default):");
    for output in &state.outputs {
        let muted = if output.muted { ", muted" } else { "" };
        println!(
            " {} {} [{}{muted}]\n     {}, icon {}\n     {}",
            mark(state.default_output.as_ref(), &output.id),
            output.description,
            output.volume,
            output.hardware,
            output.hardware.icon_name(),
            output.id
        );
    }

    println!("\nInputs (* = default):");
    for input in &state.inputs {
        let muted = if input.muted { ", muted" } else { "" };
        let kind = match &input.kind {
            InputKind::Capture => "microphone".to_owned(),
            InputKind::Monitor { of } => format!("MONITOR of {of}"),
        };
        println!(
            " {} {} [{}{muted}] ({kind})\n     {}, icon {}\n     {}",
            mark(state.default_input.as_ref(), &input.id),
            input.description,
            input.volume,
            input.hardware,
            input.hardware.icon_name(),
            input.id
        );
    }

    println!("\nApplications playing:");
    for playback in &state.playbacks {
        println!(
            "   {} {} -> {}",
            playback.id, playback.app.name, playback.output
        );
    }

    println!("\nApplications recording:");
    for recording in &state.recordings {
        let warning = if state
            .input(&recording.input)
            .is_some_and(deskcheck_core::audio::Input::is_monitor)
        {
            "  <- records a MONITOR, not a microphone"
        } else {
            ""
        };
        println!(
            "   {} {} <- {}{warning}",
            recording.id, recording.app.name, recording.input
        );
    }
}

fn main() -> ExitCode {
    match deskcheck_pulse::read_state() {
        Ok(state) => {
            print(&state);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("audio_dump: {error}");
            ExitCode::FAILURE
        }
    }
}
