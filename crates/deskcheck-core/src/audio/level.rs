//! Sound levels, for live level bars.
//!
//! A level meter receives *peaks*: the loudest sample in a short slice of
//! time, from 0.0 (silence) to 1.0 (the loudest the device can give). Ears
//! hear loudness on a log scale, so a bar shows decibels: 0 dB is the top,
//! [`FLOOR_DB`] and below is an empty bar.
//!
//! [`LevelBar`] also makes the bar fall slowly after a loud sound, as in the
//! settings of GNOME, so short words stay visible.

/// Level shown as an empty bar, in dB below the loudest level.
pub const FLOOR_DB: f32 = -60.0;

/// How much a bar can fall at each update, as a share of the whole bar.
/// At 25 updates per second, a full bar empties in about 0.8 seconds.
pub const FALL_PER_UPDATE: f32 = 0.05;

/// Converts a peak (0.0 to 1.0) to decibels, from [`FLOOR_DB`] to 0 dB.
///
/// ```
/// use deskcheck_core::audio::level::to_db;
///
/// assert_eq!(to_db(1.0), 0.0);
/// assert!((to_db(0.1) + 20.0).abs() < 0.001);
/// assert_eq!(to_db(0.0), -60.0);
/// ```
#[must_use]
pub fn to_db(peak: f32) -> f32 {
    let peak = peak.abs();
    if !peak.is_finite() || peak <= 0.0 {
        return FLOOR_DB;
    }
    (20.0 * peak.log10()).clamp(FLOOR_DB, 0.0)
}

/// Converts a peak (0.0 to 1.0) to the filled share of a bar, from 0.0 to
/// 1.0, on the decibel scale.
#[must_use]
pub fn bar_fraction(peak: f32) -> f32 {
    (to_db(peak) - FLOOR_DB) / -FLOOR_DB
}

/// A level bar that rises at once and falls slowly.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LevelBar {
    fraction: f32,
    peak: f32,
}

impl LevelBar {
    /// Takes a new peak and returns the share of the bar to fill.
    pub fn update(&mut self, peak: f32) -> f32 {
        let target = bar_fraction(peak);
        self.fraction = target.max(self.fraction - FALL_PER_UPDATE).max(0.0);
        self.peak = peak.abs();
        self.fraction
    }

    /// Lets the bar fall without a new peak, when no value came in time.
    pub fn fall(&mut self) -> f32 {
        self.fraction = (self.fraction - FALL_PER_UPDATE).max(0.0);
        self.fraction
    }

    /// The filled share of the bar, from 0.0 to 1.0.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        self.fraction
    }

    /// The last peak, in dB.
    #[must_use]
    pub fn db(&self) -> f32 {
        to_db(self.peak)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn decibels_are_clamped() {
        assert!(close(to_db(0.5), -6.0206));
        assert!(close(to_db(2.0), 0.0));
        assert!(close(to_db(-1.0), 0.0));
        assert!(close(to_db(0.000_001), FLOOR_DB));
        assert!(close(to_db(f32::NAN), FLOOR_DB));
    }

    #[test]
    fn the_bar_follows_the_decibels() {
        assert!(close(bar_fraction(0.0), 0.0));
        assert!(close(bar_fraction(1.0), 1.0));
        // -20 dB is two thirds of a 60 dB bar.
        assert!(close(bar_fraction(0.1), 2.0 / 3.0));
    }

    #[test]
    fn the_bar_rises_at_once_and_falls_slowly() {
        let mut bar = LevelBar::default();
        assert!(close(bar.update(1.0), 1.0));
        assert!(close(bar.update(0.0), 0.95));
        assert!(close(bar.update(0.0), 0.90));
        assert!(close(bar.fall(), 0.85));
        assert!(close(bar.db(), FLOOR_DB));
        // A new loud sound lifts it again at once.
        assert!(close(bar.update(1.0), 1.0));
        assert!(close(bar.db(), 0.0));
        for _ in 0..30 {
            bar.fall();
        }
        assert!(close(bar.fraction(), 0.0));
    }
}
