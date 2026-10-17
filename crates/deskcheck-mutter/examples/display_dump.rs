//! Prints what GNOME reports about the screens, to check DeskCheck on a
//! real machine.
//!
//! ```sh
//! cargo run -p deskcheck-mutter --example display_dump
//! ```

use std::process::ExitCode;

use deskcheck_core::display::DisplayState;

fn print(state: &DisplayState) {
    println!("Screens, left to right (* = primary):");
    for (number, screen) in state.enabled().iter().enumerate() {
        let mark = if screen.is_primary() { "*" } else { " " };
        let builtin = if screen.builtin { ", built-in" } else { "" };
        let mode = screen
            .current_mode
            .map_or_else(|| "no mode".to_owned(), |mode| mode.to_string());
        let placement = screen.placement.map_or_else(String::new, |p| {
            format!(", at ({}, {}), scale {}", p.x, p.y, p.scale)
        });
        println!(
            " {mark} {} {}{builtin}\n     {} on {}, {mode}{placement}",
            number + 1,
            screen.display_name.as_deref().unwrap_or("(no name)"),
            screen.id,
            screen.connector,
        );
    }

    let off: Vec<_> = state.screens.iter().filter(|s| !s.is_enabled()).collect();
    if !off.is_empty() {
        println!("\nConnected but off:");
        for screen in off {
            println!("   {} on {}", screen.id, screen.connector);
        }
    }
}

fn main() -> ExitCode {
    match deskcheck_mutter::read_state() {
        Ok(state) => {
            print(&state);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("display_dump: {error}");
            ExitCode::FAILURE
        }
    }
}
