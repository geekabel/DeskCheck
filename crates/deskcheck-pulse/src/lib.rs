//! Sound backend of DeskCheck.
//!
//! It reads the state of the sound server through the PulseAudio client
//! library. This works with PulseAudio and with PipeWire, whose
//! `pipewire-pulse` service speaks the same protocol.
//!
//! ```no_run
//! let state = deskcheck_pulse::read_state()?;
//! for mic in state.microphones() {
//!     println!("{}", mic.description);
//! }
//! # Ok::<(), deskcheck_pulse::Error>(())
//! ```

mod client;
mod convert;
mod raw;

pub use client::Error;
use deskcheck_core::audio::AudioState;

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
