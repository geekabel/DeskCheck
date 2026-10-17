//! Finding the device the user means.
//!
//! The user says which device with a short text: `default`, a screen
//! number from `deskcheck displays`, a connector like `HDMI-1`, the
//! technical name, or any part of a name that matches only one device.
//! Everything here is pure, so it is tested without a sound server or GNOME.

use std::fmt;

use deskcheck_config::Config;
use deskcheck_core::audio::{AudioState, Input, Output};
use deskcheck_core::display::{DisplayState, Screen};

/// Why a command cannot find what the user means, or cannot do it.
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
                "the default input is a monitor (a copy of an output), not a microphone",
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

pub(crate) fn is_default(which: &str) -> bool {
    which.trim().eq_ignore_ascii_case("default")
}

pub(crate) fn find_output<'a>(
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

/// Only microphones can be chosen: a monitor is a copy of an output.
pub(crate) fn find_input<'a>(
    config: &Config,
    state: &'a AudioState,
    which: &str,
) -> Result<&'a Input, Error> {
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
pub(crate) fn find_screen<'a>(
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
