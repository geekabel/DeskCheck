//! `deskcheck rename`: give a name to an output, an input or a screen.
//!
//! The user says which device with a short text: `default`, a screen
//! number from `deskcheck displays`, a connector like `HDMI-1`, the
//! technical name, or any part of a name that matches only one device.
//! Everything here is pure, so it is tested without a sound server or GNOME.

use std::fmt;

use deskcheck_config::Config;
use deskcheck_core::audio::{AudioState, Input, Output};
use deskcheck_core::display::{DisplayState, Screen};

/// Why the device cannot be found or renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// `default` was asked, but there is no default device.
    NoDefault(&'static str),
    /// `default` was asked for the input, but the default input is a
    /// monitor, and only microphones can be renamed.
    DefaultIsMonitor,
    /// Nothing matches. The choices are listed.
    NotFound {
        /// `output`, `microphone` or `screen`.
        kind: &'static str,
        /// What the user typed.
        which: String,
        /// The devices that can be chosen.
        choices: Vec<String>,
    },
    /// Several devices match. They are listed.
    Ambiguous {
        /// `output`, `microphone` or `screen`.
        kind: &'static str,
        /// What the user typed.
        which: String,
        /// The devices that match.
        matches: Vec<String>,
    },
    /// The new name is empty.
    EmptyName,
}

fn write_list(f: &mut fmt::Formatter<'_>, items: &[String]) -> fmt::Result {
    for item in items {
        write!(f, "\n  {item}")?;
    }
    Ok(())
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDefault(kind) => write!(f, "there is no default {kind}"),
            Self::DefaultIsMonitor => f.write_str(
                "the default input is a monitor (a copy of an output), not a microphone; \
                 rename the output instead",
            ),
            Self::NotFound {
                kind,
                which,
                choices,
            } if choices.is_empty() => {
                write!(
                    f,
                    "nothing matches \"{which}\": there is no {kind} to rename"
                )
            }
            Self::NotFound {
                kind,
                which,
                choices,
            } => {
                write!(f, "no {kind} matches \"{which}\". Choose one of:")?;
                write_list(f, choices)
            }
            Self::Ambiguous {
                kind,
                which,
                matches,
            } => {
                write!(
                    f,
                    "\"{which}\" matches {} {kind}s. Be more precise:",
                    matches.len()
                )?;
                write_list(f, matches)
            }
            Self::EmptyName => f.write_str("the new name is empty; use --reset to remove a name"),
        }
    }
}

impl std::error::Error for Error {}

/// A device that can be chosen, with the texts that can name it.
struct Candidate<'a, T> {
    item: &'a T,
    /// Shown in error messages.
    shown: String,
    /// Texts that can name it: technical name, system name, given name...
    keys: Vec<String>,
    /// Screen number, `off` for a screen that is off. Empty for sound.
    number: String,
}

fn not_found<T>(kind: &'static str, which: &str, candidates: &[Candidate<'_, T>]) -> Error {
    Error::NotFound {
        kind,
        which: which.to_owned(),
        choices: candidates.iter().map(|c| c.shown.clone()).collect(),
    }
}

/// Finds the one candidate named by `which`: first by a whole key, then by
/// a part of a key. Case does not matter.
fn pick<'a, T>(
    kind: &'static str,
    which: &str,
    candidates: &[Candidate<'a, T>],
) -> Result<&'a T, Error> {
    let wanted = which.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(not_found(kind, which, candidates));
    }
    let lower = |key: &String| key.to_lowercase();
    let whole: Vec<&Candidate<T>> = candidates
        .iter()
        .filter(|c| c.keys.iter().map(lower).any(|key| key == wanted))
        .collect();
    let found = if whole.is_empty() {
        candidates
            .iter()
            .filter(|c| c.keys.iter().map(lower).any(|key| key.contains(&wanted)))
            .collect()
    } else {
        whole
    };
    match found.as_slice() {
        [one] => Ok(one.item),
        [] => Err(not_found(kind, which, candidates)),
        many => Err(Error::Ambiguous {
            kind,
            which: which.to_owned(),
            matches: many.iter().map(|c| c.shown.clone()).collect(),
        }),
    }
}

fn is_default(which: &str) -> bool {
    which.trim().eq_ignore_ascii_case("default")
}

fn find_output<'a>(
    config: &Config,
    state: &'a AudioState,
    which: &str,
) -> Result<&'a Output, Error> {
    if is_default(which) {
        return state
            .default_output_device()
            .ok_or(Error::NoDefault("output"));
    }
    let candidates: Vec<_> = state
        .outputs
        .iter()
        .map(|output| {
            let label = config.output_label(output);
            Candidate {
                item: output,
                shown: format!("{label}  ({})", output.id),
                keys: vec![
                    output.id.to_string(),
                    output.description.clone(),
                    label.to_owned(),
                ],
                number: String::new(),
            }
        })
        .collect();
    pick("output", which, &candidates)
}

/// Only microphones can be renamed: a monitor follows its output.
fn find_input<'a>(config: &Config, state: &'a AudioState, which: &str) -> Result<&'a Input, Error> {
    if is_default(which) {
        let input = state
            .default_input_device()
            .ok_or(Error::NoDefault("input"))?;
        return if input.is_monitor() {
            Err(Error::DefaultIsMonitor)
        } else {
            Ok(input)
        };
    }
    let candidates: Vec<_> = state
        .microphones()
        .map(|input| {
            let label = config.input_label(input).into_owned();
            Candidate {
                item: input,
                shown: format!("{label}  ({})", input.id),
                keys: vec![input.id.to_string(), input.description.clone(), label],
                number: String::new(),
            }
        })
        .collect();
    pick("microphone", which, &candidates)
}

/// A screen is named by its number, its connector, or a part of its names
/// or identity. A plain number is only ever a screen number.
fn find_screen<'a>(
    config: &Config,
    state: &'a DisplayState,
    which: &str,
) -> Result<&'a Screen, Error> {
    // Screens that are on, by number, then the screens that are off.
    let listed = state
        .numbered()
        .into_iter()
        .map(|(number, screen)| (number.to_string(), screen))
        .chain(
            state
                .screens
                .iter()
                .filter(|screen| !screen.is_enabled())
                .map(|screen| ("off".to_owned(), screen)),
        );
    let candidates: Vec<_> = listed
        .map(|(number, screen)| {
            let label = config.screen_label(screen);
            Candidate {
                item: screen,
                shown: format!("{number}  {label}  ({} on {})", screen.id, screen.connector),
                keys: vec![
                    screen.connector.clone(),
                    screen.name().to_owned(),
                    label.to_owned(),
                    screen.id.to_string(),
                ],
                number,
            }
        })
        .collect();
    if which.trim().parse::<usize>().is_ok() {
        return candidates
            .iter()
            .find(|c| c.number == which.trim())
            .map(|c| c.item)
            .ok_or_else(|| not_found("screen", which, &candidates));
    }
    pick("screen", which, &candidates)
}

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
    use deskcheck_core::audio::{Connection, DeviceId, Form, Hardware, InputKind, Volume};
    use deskcheck_core::display::{Mode, Placement, ScreenId};

    const SPEAKERS: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo";
    const MIC: &str = "alsa_input.pci-0000_00_1f.3.analog-stereo";
    const MONITOR: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor";
    const HEADSET: &str = "alsa_output.usb-GN_Netcom_A_S_Jabra_EVOLVE_LINK-00.analog-stereo";

    fn device_output(id: &str, description: &str, form: Form) -> Output {
        Output {
            id: DeviceId::new(id),
            description: description.to_owned(),
            hardware: Hardware {
                form,
                connection: Connection::Unknown,
            },
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    fn device_input(id: &str, description: &str, kind: InputKind) -> Input {
        Input {
            id: DeviceId::new(id),
            description: description.to_owned(),
            hardware: Hardware::default(),
            kind,
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    /// Godwin's home devices, plus a headset as at the office.
    fn audio() -> AudioState {
        AudioState {
            outputs: vec![
                device_output(SPEAKERS, "Built-in Audio Analog Stereo", Form::Speakers),
                device_output(HEADSET, "Jabra EVOLVE LINK Analog Stereo", Form::Headset),
            ],
            inputs: vec![
                device_input(
                    MONITOR,
                    "Monitor of Built-in Audio Analog Stereo",
                    InputKind::Monitor {
                        of: DeviceId::new(SPEAKERS),
                    },
                ),
                device_input(MIC, "Built-in Audio Analog Stereo", InputKind::Capture),
            ],
            default_output: Some(DeviceId::new(SPEAKERS)),
            default_input: Some(DeviceId::new(MIC)),
            playbacks: vec![],
            recordings: vec![],
        }
    }

    fn display(
        id: ScreenId,
        connector: &str,
        name: &str,
        x: i32,
        on: bool,
    ) -> deskcheck_core::display::Screen {
        Screen {
            id,
            connector: connector.to_owned(),
            display_name: Some(name.to_owned()),
            builtin: connector.starts_with("eDP"),
            current_mode: on.then_some(Mode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
            }),
            placement: on.then_some(Placement {
                x,
                y: 0,
                scale: 1.0,
                primary: x == 0,
            }),
        }
    }

    /// Godwin's home screens, plus an office screen plugged in but off.
    fn screens() -> DisplayState {
        DisplayState {
            screens: vec![
                display(
                    ScreenId::new("AUS", "XG27ACS", "T2LMTF031523"),
                    "HDMI-1",
                    "ASUSTek COMPUTER INC 27\"",
                    1920,
                    true,
                ),
                display(
                    ScreenId::new("LGD", "0x0764", "0x00000000"),
                    "eDP-1",
                    "Built-in display",
                    0,
                    true,
                ),
                display(
                    ScreenId::new("DEL", "DELL U2419H", "ABC1234"),
                    "DP-1",
                    "Dell Inc. 24\"",
                    0,
                    false,
                ),
            ],
        }
    }

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
