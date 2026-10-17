//! Sound backend of DeskCheck.
//!
//! It reads the state of the sound server through the PulseAudio client
//! library, and changes it with [`apply`]. [`Meters`] shows the live level
//! of microphones. This works with PulseAudio and with PipeWire, whose
//! `pipewire-pulse` service speaks the same protocol.
//!
//! ```no_run
//! let state = deskcheck_pulse::read_state()?;
//! for mic in state.microphones() {
//!     println!("{}", mic.description);
//! }
//! # Ok::<(), deskcheck_pulse::Error>(())
//! ```

mod actions;
mod client;
mod convert;
mod meter;
mod raw;
mod sound_test;
mod watch;

pub use actions::{Action, apply};
pub use client::Error;
use deskcheck_core::audio::AudioState;
pub use meter::{Event, Meters, UPDATES_PER_SECOND};
pub use sound_test::{TEST_RATE, play, record};
pub use watch::SoundWatcher;

/// Which sound server answers: PulseAudio itself, or PipeWire through
/// `pipewire-pulse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerKind {
    /// PulseAudio.
    PulseAudio,
    /// PipeWire, which speaks the PulseAudio protocol.
    PipeWire,
}

impl ServerKind {
    /// Tells the server from the name it gives: PipeWire answers
    /// `PulseAudio (on PipeWire 0.3.48)`.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        if name.contains("PipeWire") {
            Self::PipeWire
        } else {
            Self::PulseAudio
        }
    }

    /// The name to show.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::PulseAudio => "PulseAudio",
            Self::PipeWire => "PipeWire",
        }
    }
}

/// Connects to the sound server and tells which one it is.
///
/// # Errors
///
/// Returns an [`Error`] when no sound server answers.
pub fn server_kind() -> Result<ServerKind, Error> {
    client::read_server_name().map(|name| ServerKind::from_name(&name))
}

/// Connects to the sound server and reads the outputs, inputs, defaults and
/// application streams.
///
/// # Errors
///
/// Returns an [`Error`] when no sound server answers, when the connection
/// is lost while reading, or when the server refuses a list.
pub fn read_state() -> Result<AudioState, Error> {
    client::read_snapshot().map(|snapshot| convert::audio_state(&snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_kind_comes_from_its_name() {
        assert_eq!(ServerKind::from_name("pulseaudio"), ServerKind::PulseAudio);
        assert_eq!(
            ServerKind::from_name("PulseAudio (on PipeWire 0.3.48)"),
            ServerKind::PipeWire
        );
        assert_eq!(ServerKind::PipeWire.label(), "PipeWire");
    }
}
