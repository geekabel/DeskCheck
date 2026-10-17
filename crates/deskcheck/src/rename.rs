//! `deskcheck rename`: give a name to an output, an input or a screen.
//!
//! Which device is found by [`crate::select`]. Everything here is pure, so
//! it is tested without a sound server or GNOME.

use deskcheck_config::Config;
use deskcheck_core::audio::AudioState;
use deskcheck_core::display::DisplayState;

use crate::select::{Error, find_input, find_output, find_screen};

/// Checks the new name. `None` means `--reset`.
fn checked(name: Option<&str>) -> Result<Option<String>, Error> {
    match name.map(str::trim) {
        Some("") => Err(Error::EmptyName),
        Some(name) => Ok(Some(name.to_owned())),
        None => Ok(None),
    }
}

/// The sentence printed after a change.
fn message(kind: &str, before: &str, system: &str, new: Option<&str>) -> String {
    match new {
        Some(new) => format!("{kind} \"{before}\" is now called \"{new}\"."),
        None if before == system => format!("{kind} \"{system}\" has no name of its own."),
        None => format!("{kind} \"{before}\" is called \"{system}\" again."),
    }
}

/// Renames an output, or removes its name when `name` is `None`.
///
/// # Errors
///
/// Returns an [`Error`] when `which` does not name exactly one output, or
/// when the new name is empty.
pub fn output(
    config: &mut Config,
    state: &AudioState,
    which: &str,
    name: Option<&str>,
) -> Result<String, Error> {
    let name = checked(name)?;
    let output = find_output(config, state, which)?;
    let before = config.output_label(output).to_owned();
    config.set_audio_name(&output.id, name.clone());
    Ok(message(
        "Output",
        &before,
        &output.description,
        name.as_deref(),
    ))
}

/// Renames a microphone, or removes its name when `name` is `None`.
///
/// # Errors
///
/// Returns an [`Error`] when `which` does not name exactly one microphone,
/// or when the new name is empty.
pub fn input(
    config: &mut Config,
    state: &AudioState,
    which: &str,
    name: Option<&str>,
) -> Result<String, Error> {
    let name = checked(name)?;
    let input = find_input(config, state, which)?;
    let before = config.input_label(input).into_owned();
    config.set_audio_name(&input.id, name.clone());
    Ok(message(
        "Input",
        &before,
        &input.description,
        name.as_deref(),
    ))
}

/// Renames a screen, or removes its name when `name` is `None`.
///
/// # Errors
///
/// Returns an [`Error`] when `which` does not name exactly one screen, or
/// when the new name is empty.
pub fn screen(
    config: &mut Config,
    state: &DisplayState,
    which: &str,
    name: Option<&str>,
) -> Result<String, Error> {
    let name = checked(name)?;
    let screen = find_screen(config, state, which)?;
    let before = config.screen_label(screen).to_owned();
    config.set_screen_name(&screen.id, name.clone());
    Ok(message("Screen", &before, screen.name(), name.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{HEADSET, MIC, MONITOR, SPEAKERS, audio, screens};
    use deskcheck_core::audio::{DeviceId, Input};
    use deskcheck_core::display::ScreenId;

    fn named(config: &Config, id: &str) -> Option<String> {
        config.audio_name(&DeviceId::new(id)).map(str::to_owned)
    }

    #[test]
    fn the_default_output_is_renamed() {
        let mut config = Config::default();
        let message = output(&mut config, &audio(), "default", Some("Laptop speakers")).unwrap();
        assert_eq!(
            message,
            "Output \"Built-in Audio Analog Stereo\" is now called \"Laptop speakers\"."
        );
        assert_eq!(named(&config, SPEAKERS).as_deref(), Some("Laptop speakers"));
    }

    #[test]
    fn an_output_is_found_by_a_part_of_its_name() {
        let mut config = Config::default();
        output(&mut config, &audio(), "jabra", Some("Headset")).unwrap();
        assert_eq!(named(&config, HEADSET).as_deref(), Some("Headset"));
        // Then by its new name, and back to the system name.
        let message = output(&mut config, &audio(), "headset", None).unwrap();
        assert_eq!(
            message,
            "Output \"Headset\" is called \"Jabra EVOLVE LINK Analog Stereo\" again."
        );
        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_whole_name_wins_over_a_part() {
        // "analog stereo" is in both outputs, but the technical name of the
        // speakers is matched as a whole.
        let mut config = Config::default();
        output(&mut config, &audio(), SPEAKERS, Some("Speakers")).unwrap();
        assert_eq!(named(&config, SPEAKERS).as_deref(), Some("Speakers"));
    }

    #[test]
    fn several_matches_are_listed() {
        let mut config = Config::default();
        let error = output(&mut config, &audio(), "analog", Some("x")).unwrap_err();
        let Error::Ambiguous { matches, .. } = &error else {
            panic!("{error:?}");
        };
        assert_eq!(matches.len(), 2);
        assert!(
            error
                .to_string()
                .contains(&format!("  Built-in Audio Analog Stereo  ({SPEAKERS})"))
        );
        assert_eq!(config, Config::default());
    }

    #[test]
    fn nothing_found_lists_the_choices() {
        let error = output(&mut Config::default(), &audio(), "hdmi", Some("x")).unwrap_err();
        let message = error.to_string();
        assert!(
            message.starts_with("no output matches \"hdmi\". Choose one of:\n  "),
            "{message}"
        );
        assert!(message.contains(HEADSET), "{message}");
    }

    #[test]
    fn the_microphone_is_renamed_but_not_the_monitor() {
        let mut config = Config::default();
        // Only one microphone has this name: the monitor is not a choice.
        input(
            &mut config,
            &audio(),
            "Built-in Audio Analog Stereo",
            Some("Laptop mic"),
        )
        .unwrap();
        assert_eq!(named(&config, MIC).as_deref(), Some("Laptop mic"));

        let error = input(&mut config, &audio(), "monitor", Some("x")).unwrap_err();
        assert!(matches!(error, Error::NotFound { .. }), "{error:?}");

        let mut state = audio();
        state.default_input = Some(DeviceId::new(MONITOR));
        let error = input(&mut config, &state, "default", Some("x")).unwrap_err();
        assert_eq!(error, Error::DefaultIsMonitor);
    }

    #[test]
    fn screens_are_found_by_number_connector_or_name() {
        let mut config = Config::default();
        let message = screen(&mut config, &screens(), "2", Some("Big screen")).unwrap();
        assert_eq!(
            message,
            "Screen \"ASUSTek COMPUTER INC 27\"\" is now called \"Big screen\"."
        );
        let asus = ScreenId::new("AUS", "XG27ACS", "T2LMTF031523");
        assert_eq!(config.screen_name(&asus), Some("Big screen"));

        screen(&mut config, &screens(), "edp-1", Some("Laptop")).unwrap();
        let laptop = ScreenId::new("LGD", "0x0764", "0x00000000");
        assert_eq!(config.screen_name(&laptop), Some("Laptop"));

        // A screen that is off has no number, but can be named by its name.
        screen(&mut config, &screens(), "dell", Some("Office")).unwrap();
        let dell = ScreenId::new("DEL", "DELL U2419H", "ABC1234");
        assert_eq!(config.screen_name(&dell), Some("Office"));
    }

    #[test]
    fn an_unknown_screen_number_lists_the_screens() {
        let error = screen(&mut Config::default(), &screens(), "3", Some("x")).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("\n  1  Built-in display  (0x0764 (LGD) on eDP-1)"),
            "{message}"
        );
        assert!(
            message.contains("\n  off  Dell Inc. 24\"  (DELL U2419H (DEL, ABC1234) on DP-1)"),
            "{message}"
        );
    }

    #[test]
    fn no_microphone_at_all() {
        let mut state = audio();
        state.inputs.retain(Input::is_monitor);
        let error = input(&mut Config::default(), &state, "jabra", Some("x")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "nothing matches \"jabra\": there is no microphone to rename"
        );
    }

    #[test]
    fn a_blank_choice_matches_nothing() {
        let error = output(&mut Config::default(), &audio(), " ", Some("x")).unwrap_err();
        assert!(matches!(error, Error::NotFound { .. }), "{error:?}");
    }

    #[test]
    fn an_empty_name_is_refused() {
        let mut config = Config::default();
        let error = output(&mut config, &audio(), "default", Some("  ")).unwrap_err();
        assert_eq!(error, Error::EmptyName);
    }

    #[test]
    fn reset_without_a_name_says_so() {
        let message = screen(&mut Config::default(), &screens(), "1", None).unwrap();
        assert_eq!(
            message,
            "Screen \"Built-in display\" has no name of its own."
        );
    }
}
