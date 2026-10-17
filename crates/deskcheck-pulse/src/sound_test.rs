//! Playing a sound on an output, and recording a few seconds from an input,
//! to test them.
//!
//! Samples are mono 32-bit floats at [`TEST_RATE`]; the sound server mixes
//! them to the channels of the device. A recording stays in memory and is
//! never written to disk.

use std::time::{Duration, Instant};

use deskcheck_core::audio::DeviceId;
use libpulse_binding as pulse;
use pulse::def::BufferAttr;
use pulse::sample::{Format, Spec};
use pulse::stream::{FlagSet, PeekResult, SeekMode, State, Stream};
use pulse::time::MicroSeconds;

use crate::client::{Connection, Error, connect, describe, run_until, turn};

/// Samples per second of the test sound and of test recordings.
pub const TEST_RATE: u32 = 48_000;

/// Size of one sample: a 32-bit float.
const SAMPLE_SIZE: usize = std::mem::size_of::<f32>();

/// Longest wait of one turn of the main loop.
const TURN: MicroSeconds = MicroSeconds(50_000);

/// Latency asked for playback. By default the server waits for about 2
/// seconds of sound before it starts (`prebuf` is `tlength`), longer than
/// the whole chime.
const PLAYBACK_LATENCY_MS: u32 = 100;

/// How long to wait for a device that sends or takes no sound at all.
const STALL: Duration = Duration::from_secs(3);

fn spec() -> Spec {
    Spec {
        format: Format::F32le,
        rate: TEST_RATE,
        channels: 1,
    }
}

/// Waits until a new stream is ready, or fails.
fn wait_ready(connection: &mut Connection, stream: &Stream, what: &str) -> Result<(), Error> {
    run_until(&mut connection.mainloop, &connection.context, || {
        stream.get_state() != State::Creating && stream.get_state() != State::Unconnected
    })?;
    if stream.get_state() == State::Ready {
        Ok(())
    } else {
        Err(Error::Refused {
            what: what.to_owned(),
            reason: describe(connection.context.errno()),
        })
    }
}

fn to_bytes(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

/// Plays `samples` on an output and returns once they have been heard.
///
/// # Errors
///
/// Returns an [`Error`] when no sound server answers, when the output is
/// not there, or when the playback stops.
pub fn play(output: &DeviceId, samples: &[f32]) -> Result<(), Error> {
    let what = format!("play a test sound on {output}");
    let mut connection = connect()?;
    let mut stream = Stream::new(
        &mut connection.context,
        "DeskCheck sound test",
        &spec(),
        None,
    )
    .ok_or(Error::Setup("cannot create a playback stream"))?;
    let bytes_per_ms = TEST_RATE / 1000 * u32::try_from(SAMPLE_SIZE).unwrap_or(4);
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: PLAYBACK_LATENCY_MS * bytes_per_ms,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: u32::MAX,
    };
    stream
        .connect_playback(
            Some(output.as_str()),
            Some(&attr),
            FlagSet::ADJUST_LATENCY,
            None,
            None,
        )
        .map_err(|error| Error::Refused {
            what: what.clone(),
            reason: describe(error),
        })?;
    wait_ready(&mut connection, &stream, &what)?;

    let bytes = to_bytes(samples);
    let mut written = 0;
    let mut last_progress = Instant::now();
    while written < bytes.len() {
        let room = stream.writable_size().unwrap_or(0);
        if room > 0 {
            let end = (written + room).min(bytes.len());
            stream
                .write_copy(&bytes[written..end], 0, SeekMode::Relative)
                .map_err(|error| Error::Refused {
                    what: what.clone(),
                    reason: describe(error),
                })?;
            written = end;
            last_progress = Instant::now();
        } else if last_progress.elapsed() > STALL {
            return Err(Error::Refused {
                what,
                reason: "the output takes no sound".to_owned(),
            });
        }
        turn(&mut connection, TURN)?;
    }

    // Waits until the server has played everything.
    let drained = stream.drain(None);
    run_until(&mut connection.mainloop, &connection.context, || {
        drained.get_state() != pulse::operation::State::Running
    })?;
    let _ = stream.disconnect();
    connection.context.disconnect();
    Ok(())
}

/// Records `duration` from an input. `progress` is called with the share
/// done, from 0.0 to 1.0, as the samples come in.
///
/// # Errors
///
/// Returns an [`Error`] when no sound server answers, when the input is
/// not there, or when it sends no sound at all.
pub fn record(
    input: &DeviceId,
    duration: Duration,
    mut progress: impl FnMut(f32),
) -> Result<Vec<f32>, Error> {
    let what = format!("record from {input}");
    let mut connection = connect()?;
    let mut stream = Stream::new(
        &mut connection.context,
        "DeskCheck sound test",
        &spec(),
        None,
    )
    .ok_or(Error::Setup("cannot create a recording stream"))?;
    stream
        .connect_record(Some(input.as_str()), None, FlagSet::NOFLAGS)
        .map_err(|error| Error::Refused {
            what: what.clone(),
            reason: describe(error),
        })?;
    wait_ready(&mut connection, &stream, &what)?;

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a test recording lasts a few seconds"
    )]
    let wanted = (duration.as_secs_f64() * f64::from(TEST_RATE)) as usize;
    let mut bytes: Vec<u8> = Vec::with_capacity(wanted * SAMPLE_SIZE);
    let mut last_progress = Instant::now();
    while bytes.len() < wanted * SAMPLE_SIZE {
        turn(&mut connection, TURN)?;
        let before = bytes.len();
        while stream.readable_size().is_some_and(|size| size > 0) {
            match stream.peek() {
                Ok(PeekResult::Data(data)) => {
                    bytes.extend_from_slice(data);
                    let _ = stream.discard();
                }
                Ok(PeekResult::Hole(size)) => {
                    // Missing sound counts as silence.
                    bytes.resize(bytes.len() + size, 0);
                    let _ = stream.discard();
                }
                Ok(PeekResult::Empty) | Err(_) => break,
            }
        }
        if bytes.len() > before {
            last_progress = Instant::now();
            #[allow(
                clippy::cast_precision_loss,
                reason = "a few seconds of samples, far below 2^24"
            )]
            progress((bytes.len() / SAMPLE_SIZE) as f32 / wanted as f32);
        } else if last_progress.elapsed() > STALL {
            return Err(Error::Refused {
                what,
                reason: "the input sends no sound".to_owned(),
            });
        }
    }
    let _ = stream.disconnect();
    connection.context.disconnect();

    let mut samples: Vec<f32> = bytes
        .chunks_exact(SAMPLE_SIZE)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    samples.truncate(wanted);
    progress(1.0);
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_become_little_endian_bytes() {
        assert_eq!(to_bytes(&[1.0]), 1.0_f32.to_le_bytes());
        assert_eq!(to_bytes(&[0.0, -0.5]).len(), 8);
    }
}
