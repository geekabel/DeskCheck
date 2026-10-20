//! Starting DeskCheck with the session (DC-33).
//!
//! As the XDG Autostart specification says, a desktop file in
//! `$XDG_CONFIG_HOME/autostart` (`~/.config/autostart`) is started when
//! the user logs in. DeskCheck writes one named after its id, which runs
//! it with `--background`: it starts in the top bar, without its window,
//! and recognizes the place at once. Removing the file turns it off.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use deskcheck_core::{APP_ID, APP_NAME};

/// The option that starts DeskCheck without its window.
pub const BACKGROUND: &str = "background";

/// The folder of the files started at login.
fn folder() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("autostart"))
}

/// The file DeskCheck writes there.
pub fn file() -> Option<PathBuf> {
    folder().map(|folder| folder.join(format!("{APP_ID}.desktop")))
}

/// The text of the file, starting `program`.
pub fn desktop_entry(program: &Path) -> String {
    // The Exec key quotes arguments with spaces in double quotes.
    let program = program.display().to_string();
    let exec = if program.contains(' ') {
        format!("\"{}\"", program.replace('"', "\\\""))
    } else {
        program
    };
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={APP_NAME}\n\
         Comment=See and fix your sound devices and screens\n\
         Exec={exec} --{BACKGROUND}\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

/// `true` when the text turns the start at login off: `Hidden=true`, or
/// `X-GNOME-Autostart-enabled=false` as GNOME writes it.
pub fn turned_off(text: &str) -> bool {
    text.lines().map(str::trim).any(|line| {
        line.eq_ignore_ascii_case("Hidden=true")
            || line.eq_ignore_ascii_case("X-GNOME-Autostart-enabled=false")
    })
}

/// `true` when DeskCheck starts with the session.
pub fn enabled() -> bool {
    file()
        .and_then(|file| fs::read_to_string(file).ok())
        .is_some_and(|text| !turned_off(&text))
}

/// Starts DeskCheck with the session, or not.
///
/// # Errors
///
/// Returns the error of the file system, or `NotFound` without a home
/// folder.
pub fn set_enabled(on: bool) -> io::Result<()> {
    let file = file().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder"))?;
    if on {
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder)?;
        }
        let program = std::env::current_exe()?;
        fs::write(&file, desktop_entry(&program))
    } else {
        match fs::remove_file(&file) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_starts_deskcheck_in_the_background() {
        let text = desktop_entry(Path::new("/usr/bin/deskcheck-app"));
        assert!(text.starts_with("[Desktop Entry]\n"));
        assert!(text.contains("\nExec=/usr/bin/deskcheck-app --background\n"));
        assert!(!turned_off(&text));
        let spaced = desktop_entry(Path::new("/home/me/my apps/deskcheck-app"));
        assert!(spaced.contains("Exec=\"/home/me/my apps/deskcheck-app\" --background"));
    }

    #[test]
    fn gnome_can_turn_it_off_in_the_file() {
        assert!(turned_off(
            "[Desktop Entry]\nX-GNOME-Autostart-enabled=false\n"
        ));
        assert!(turned_off("[Desktop Entry]\nHidden=true\n"));
        assert!(!turned_off("[Desktop Entry]\nExec=x\n"));
    }
}
