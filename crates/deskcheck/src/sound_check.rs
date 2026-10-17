//! `deskcheck test`: play a sound on an output, or record a few seconds
//! from a microphone and play them back.
//!
//! The sentences are built by pure functions, so they are tested without a
//! sound server.

use deskcheck_core::audio::Volume;
use deskcheck_core::audio::tone::SILENCE_DB;

use crate::sound::Kind;

/// Default length of a test recording, in seconds.
pub const DEFAULT_SECONDS: u64 = 3;

/// Longest test recording, in seconds.
pub const MAX_SECONDS: u64 = 10;

/// Reads the length of a test recording, from 1 to [`MAX_SECONDS`].
///
/// # Errors
///
/// Returns a message when the text is not a whole number in that range.
pub fn parse_seconds(text: &str) -> Result<u64, String> {
    let seconds: u64 = text
        .trim()
        .parse()
        .map_err(|_| format!("`{text}` is not a number of seconds"))?;
    if (1..=MAX_SECONDS).contains(&seconds) {
        Ok(seconds)
    } else {
        Err(format!("choose from 1 to {MAX_SECONDS} seconds"))
    }
}

/// Quotes a device choice when it has spaces, so a command can be pasted.
fn shell_word(which: &str) -> String {
    if which.contains(char::is_whitespace) {
        format!("\"{which}\"")
    } else {
        which.to_owned()
    }
}

/// A warning when a device will not be heard: muted, or at 0 %.
pub fn off_warning(
    kind: Kind,
    label: &str,
    muted: bool,
    volume: Volume,
    which: &str,
) -> Option<String> {
    let word = match kind {
        Kind::Output => "output",
        Kind::Input => "input",
    };
    let which = shell_word(which);
    if muted {
        Some(format!(
            "Warning: \"{label}\" is muted. Run `deskcheck unmute {word} {which}` first."
        ))
    } else if volume.is_silent() {
        Some(format!(
            "Warning: \"{label}\" is at 0%. Run `deskcheck volume {word} {which} 80` first."
        ))
    } else {
        None
    }
}

/// What to say about a recording, from its loudest sound in dB.
pub fn recording_verdict(loudest_db: f32) -> String {
    if loudest_db < SILENCE_DB {
        format!(
            "Loudest sound: {loudest_db:.0} dB. That is almost silence: this microphone \
             does not hear you.\nIs it muted, too low, or not the one near you? \
             `deskcheck levels` shows all microphones live."
        )
    } else {
        format!("Loudest sound: {loudest_db:.0} dB. The microphone hears you.")
    }
}

/// A progress bar for the recording: `[██████░░░░]`.
pub fn progress_bar(done: f32) -> String {
    const WIDTH: usize = 20;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a share from 0 to 1 of a 20-character bar"
    )]
    let filled = (done.clamp(0.0, 1.0) * WIDTH as f32).round() as usize;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(WIDTH - filled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_are_checked() {
        assert_eq!(parse_seconds("3"), Ok(3));
        assert_eq!(parse_seconds("10"), Ok(10));
        assert!(parse_seconds("0").is_err());
        assert!(parse_seconds("11").is_err());
        assert!(parse_seconds("abc").is_err());
    }

    #[test]
    fn muted_or_silent_devices_get_a_warning() {
        assert_eq!(
            off_warning(
                Kind::Output,
                "Laptop speakers",
                true,
                Volume::NORMAL,
                "laptop speakers"
            ),
            Some(
                "Warning: \"Laptop speakers\" is muted. \
                 Run `deskcheck unmute output \"laptop speakers\"` first."
                    .to_owned()
            )
        );
        assert_eq!(
            off_warning(
                Kind::Input,
                "Jabra",
                false,
                Volume::from_percent(0),
                "default"
            ),
            Some(
                "Warning: \"Jabra\" is at 0%. Run `deskcheck volume input default 80` first."
                    .to_owned()
            )
        );
        assert_eq!(
            off_warning(Kind::Input, "Jabra", false, Volume::NORMAL, "default"),
            None
        );
    }

    #[test]
    fn the_verdict_tells_silence_from_a_voice() {
        assert_eq!(
            recording_verdict(-18.4),
            "Loudest sound: -18 dB. The microphone hears you."
        );
        let silent = recording_verdict(-60.0);
        assert!(silent.starts_with("Loudest sound: -60 dB. That is almost silence"));
        assert!(silent.contains("`deskcheck levels`"));
    }

    #[test]
    fn the_progress_bar_fills_up() {
        assert_eq!(progress_bar(0.0), format!("[{}]", "░".repeat(20)));
        assert_eq!(
            progress_bar(0.5),
            format!("[{}{}]", "█".repeat(10), "░".repeat(10))
        );
        assert_eq!(progress_bar(1.5), format!("[{}]", "█".repeat(20)));
    }
}
