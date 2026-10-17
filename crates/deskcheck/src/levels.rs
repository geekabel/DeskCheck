//! `deskcheck levels`: a live level bar for each microphone.
//!
//! Speak, and the bar of the microphone that hears you moves. A frame is
//! drawn by a pure function, so it is tested without a sound server.

use std::fmt::Write as _;

use deskcheck_core::audio::level::LevelBar;

/// Width of a bar, in characters.
pub const BAR_WIDTH: usize = 30;

/// One microphone in the list.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Name to show.
    pub label: String,
    /// `true` for the default input.
    pub is_default: bool,
    /// Its bar.
    pub bar: LevelBar,
    /// `true` once its meter stopped, for example when it is unplugged.
    pub lost: bool,
}

impl Row {
    /// A row with an empty bar.
    #[must_use]
    pub fn new(label: String, is_default: bool) -> Self {
        Self {
            label,
            is_default,
            bar: LevelBar::default(),
            lost: false,
        }
    }

    fn title(&self) -> String {
        if self.is_default {
            format!("{} (default)", self.label)
        } else {
            self.label.clone()
        }
    }
}

/// The bar itself: `█` for the filled part, `░` for the rest.
fn bar(fraction: f32) -> String {
    let fraction = fraction.clamp(0.0, 1.0);
    // The bar is 30 characters: the cast cannot lose anything that shows.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a share from 0 to 1 of a 30-character bar"
    )]
    let filled = (fraction * BAR_WIDTH as f32).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(BAR_WIDTH - filled))
}

/// One frame: a line per microphone.
pub fn render_frame(rows: &[Row]) -> String {
    let width = rows
        .iter()
        .map(|row| row.title().chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for row in rows {
        let title = row.title();
        if row.lost {
            let _ = writeln!(out, "  {title:<width$}  (stopped: unplugged?)");
        } else {
            let _ = writeln!(
                out,
                "  {title:<width$}  {}  {:>4.0} dB",
                bar(row.bar.fraction()),
                row.bar.db()
            );
        }
    }
    out
}

/// What to print to draw a new frame over the last one, in a terminal:
/// move the cursor up over the old lines, and clear each line.
pub fn redraw(previous_lines: usize, frame: &str) -> String {
    let mut out = String::new();
    if previous_lines > 0 {
        let _ = write!(out, "\x1b[{previous_lines}A");
    }
    for line in frame.lines() {
        let _ = writeln!(out, "\x1b[2K{line}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_are_filled_on_the_decibel_scale() {
        assert_eq!(bar(0.0), "░".repeat(30));
        assert_eq!(bar(1.0), "█".repeat(30));
        assert_eq!(bar(0.5), format!("{}{}", "█".repeat(15), "░".repeat(15)));
        assert_eq!(bar(2.0), "█".repeat(30));
    }

    #[test]
    fn a_frame_has_a_line_per_microphone() {
        let mut laptop = Row::new("Built-in Audio Analog Stereo".to_owned(), true);
        laptop.bar.update(0.1);
        let mut headset = Row::new("Jabra".to_owned(), false);
        headset.lost = true;
        let frame = render_frame(&[laptop, headset]);
        assert_eq!(
            frame,
            format!(
                "  Built-in Audio Analog Stereo (default)  {}{}   -20 dB\n  \
                 Jabra                                   (stopped: unplugged?)\n",
                "█".repeat(20),
                "░".repeat(10)
            )
        );
    }

    #[test]
    fn redraw_goes_back_over_the_old_frame() {
        assert_eq!(redraw(0, "a\nb\n"), "\x1b[2Ka\n\x1b[2Kb\n");
        assert_eq!(redraw(2, "a\nb\n"), "\x1b[2A\x1b[2Ka\n\x1b[2Kb\n");
    }
}
