//! Short texts about the state, shown by the window. They are pure, so they
//! are tested without a display.

use deskcheck_core::display::DisplayState;
use deskcheck_pulse::ServerKind;

/// The badge of the place, until places are detected (DC-27).
pub const NO_PLACE: &str = "No place yet";

/// The line about the sound server, at the bottom of the sidebar.
pub fn sound_footer(server: Option<&Result<ServerKind, String>>) -> String {
    match server {
        None => "Sound: …".to_owned(),
        Some(Ok(kind)) => format!("Sound: {}", kind.label()),
        Some(Err(_)) => "Sound: no sound server".to_owned(),
    }
}

/// The name of the kind of session, from `XDG_SESSION_TYPE`.
pub fn session_label(session_type: Option<&str>) -> Option<&'static str> {
    match session_type?.to_ascii_lowercase().as_str() {
        "wayland" => Some("Wayland"),
        "x11" => Some("X11"),
        _ => None,
    }
}

/// The line about the screens, at the bottom of the sidebar.
pub fn screens_footer(
    screens: Option<&Result<DisplayState, String>>,
    session_type: Option<&str>,
) -> String {
    match (screens, session_label(session_type)) {
        (None, _) => "Screens: …".to_owned(),
        (Some(Ok(_)), Some(session)) => format!("Screens: GNOME, {session}"),
        (Some(Ok(_)), None) => "Screens: GNOME".to_owned(),
        (Some(Err(_)), _) => "Screens: not a GNOME session".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sound_footer_names_the_server() {
        assert_eq!(sound_footer(None), "Sound: …");
        assert_eq!(
            sound_footer(Some(&Ok(ServerKind::PulseAudio))),
            "Sound: PulseAudio"
        );
        assert_eq!(
            sound_footer(Some(&Ok(ServerKind::PipeWire))),
            "Sound: PipeWire"
        );
        assert_eq!(
            sound_footer(Some(&Err("no server".to_owned()))),
            "Sound: no sound server"
        );
    }

    #[test]
    fn the_screens_footer_names_the_session() {
        let ok = Ok(DisplayState::default());
        assert_eq!(
            screens_footer(Some(&ok), Some("wayland")),
            "Screens: GNOME, Wayland"
        );
        assert_eq!(
            screens_footer(Some(&ok), Some("x11")),
            "Screens: GNOME, X11"
        );
        assert_eq!(screens_footer(Some(&ok), Some("tty")), "Screens: GNOME");
        assert_eq!(
            screens_footer(Some(&Err("not gnome".to_owned())), Some("wayland")),
            "Screens: not a GNOME session"
        );
        assert_eq!(screens_footer(None, None), "Screens: …");
    }
}
