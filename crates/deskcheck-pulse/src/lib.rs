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
