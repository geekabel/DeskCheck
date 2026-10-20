//! Prints where the configuration file is and what DeskCheck reads in it.
//!
//! It only reads: run it with `cargo run -p deskcheck-config --example config_dump`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let store = match deskcheck_config::Store::new() {
        Ok(store) => store,
        Err(error) => {
            eprintln!("config_dump: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("File: {}", store.path().display());
    println!("Exists: {}", store.path().exists());
    match store.load() {
        Ok(config) => {
            println!("Sound device names: {}", config.audio_entries().len());
            for entry in config.audio_entries() {
                let name = entry.name.as_deref().unwrap_or("(no name)");
                let icon = entry
                    .icon
                    .map(deskcheck_config::DeviceIcon::key)
                    .unwrap_or_default();
                println!("  {} = {name} {icon}", entry.id);
            }
            println!("Screens: {}", config.screen_entries().len());
            for entry in config.screen_entries() {
                let name = entry.name.as_deref().unwrap_or("(no name)");
                let color = entry.color.map(|c| c.to_string()).unwrap_or_default();
                println!("  {} = {name} {color}", entry.id);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("config_dump: {error}");
            ExitCode::FAILURE
        }
    }
}
