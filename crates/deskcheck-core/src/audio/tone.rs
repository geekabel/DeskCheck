//! The test sound, and checks on a test recording.
//!
//! Samples are mono 32-bit floats, from -1.0 to 1.0.

use std::f32::consts::TAU;

use super::level::to_db;

/// Loudness of the chime: well below the top, so it never hurts.
const CHIME_AMPLITUDE: f32 = 0.3;

/// The two notes of the chime (E5 then A5), with their length in seconds.
const NOTES: [(f32, f32); 2] = [(659.25, 0.18), (880.0, 0.42)];

/// A recording whose loudest sound is below this level is taken as
/// silence: a voice near a working microphone is far louder.
pub const SILENCE_DB: f32 = -50.0;

/// A short two-note chime, at `rate` samples per second. Each note starts
/// softly and fades out, so it does not click.
///
/// ```
/// use deskcheck_core::audio::tone::chime;
///
/// let samples = chime(48_000);
/// assert_eq!(samples.len(), 28_800); // 0.6 seconds
/// assert!(samples.iter().all(|s| s.abs() <= 0.3));
/// ```
#[must_use]
pub fn chime(rate: u32) -> Vec<f32> {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sample counts and rates are small, far below 2^24"
    )]
    let rate = rate as f32;
    let mut samples = Vec::new();
    for (frequency, seconds) in NOTES {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a note lasts less than a second"
        )]
        let count = (rate * seconds).round() as usize;
        samples.extend((0..count).map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "i is below 2^24")]
            let t = i as f32 / rate;
            // 5 ms fade in, then an exponential fade out.
            let attack = (t / 0.005).min(1.0);
            let decay = (-t * 6.0 / seconds).exp();
            CHIME_AMPLITUDE * attack * decay * (TAU * frequency * t).sin()
        }));
    }
    samples
}

/// The loudest sample of a recording, in dB (see [`to_db`]).
#[must_use]
pub fn loudest_db(samples: &[f32]) -> f32 {
    to_db(
        samples
            .iter()
            .fold(0.0_f32, |loudest, s| loudest.max(s.abs())),
    )
}

/// `true` when a recording is so quiet that the microphone heard nothing.
#[must_use]
pub fn is_silence(samples: &[f32]) -> bool {
    loudest_db(samples) < SILENCE_DB
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chime_starts_and_ends_softly() {
        let samples = chime(48_000);
        assert!(samples[0].abs() < 1e-3);
        assert!(samples.last().unwrap().abs() < 0.01);
        assert!(loudest_db(&samples) > -12.0);
    }

    #[test]
    fn silence_is_told_from_a_voice() {
        assert!(is_silence(&[]));
        assert!(is_silence(&[0.0; 100]));
        assert!(is_silence(&[0.001, -0.002]));
        assert!(!is_silence(&[0.0, -0.1, 0.05]));
        assert!(!is_silence(&chime(8_000)));
    }
}
