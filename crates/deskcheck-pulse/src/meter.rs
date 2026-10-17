//! Live level meters for microphones.
//!
//! [`Meters::start`] opens one *peak detect* stream per input, like the
//! level bar of GNOME Settings (`cc-level-bar.c`): the server sends only the
//! loudest sample of each slice of time, [`UPDATES_PER_SECOND`] times per
//! second, one float at a time. So the work stays small.
//!
//! The streams live in a thread of their own, with their own connection,
//! because the main loop of the library cannot move between threads. The
//! thread sends [`Event`]s on a channel and stops when [`Meters`] is
//! dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use deskcheck_core::audio::DeviceId;
use libpulse_binding as pulse;
use pulse::context::State as ContextState;
use pulse::def::BufferAttr;
use pulse::sample::{Format, Spec};
use pulse::stream::{FlagSet, PeekResult, State, Stream};
use pulse::time::MicroSeconds;

use crate::client::{Connection, Error, connect, describe};

/// How many peaks each meter sends per second.
pub const UPDATES_PER_SECOND: u32 = 25;

/// Longest wait of the thread before it checks whether it must stop.
const WAKE_UP: MicroSeconds = MicroSeconds(100_000);

/// Size of one peak: a 32-bit float.
const PEAK_SIZE: usize = std::mem::size_of::<f32>();

/// What the meter thread sends.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A new peak for an input, from 0.0 (silence) to 1.0.
    Peak {
        /// The input.
        input: DeviceId,
        /// The loudest sample since the last peak.
        peak: f32,
    },
    /// The meter of this input stopped, for example because it was
    /// unplugged.
    Lost(DeviceId),
    /// The connection to the sound server failed: no more events will come.
    Failed(Error),
}

/// Running level meters. Dropping it stops them.
#[derive(Debug)]
pub struct Meters {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    events: Receiver<Event>,
}

impl Meters {
    /// Starts one meter per input.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] when no sound server answers or a meter cannot
    /// be created. A meter that fails later is reported as [`Event::Lost`].
    pub fn start(inputs: &[DeviceId]) -> Result<Self, Error> {
        let (events_tx, events) = mpsc::channel();
        let (ready_tx, ready) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            let inputs = inputs.to_vec();
            thread::Builder::new()
                .name("deskcheck-meters".to_owned())
                .spawn(move || run(&inputs, &stop, &events_tx, &ready_tx))
                .map_err(|_| Error::Setup("cannot start the meter thread"))?
        };
        match ready.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                thread: Some(thread),
                events,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(Error::Mainloop("the meter thread stopped".to_owned()))
            }
        }
    }

    /// The events of the meters.
    #[must_use]
    pub fn events(&self) -> &Receiver<Event> {
        &self.events
    }
}

impl Drop for Meters {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Opens a peak detect stream on an input.
fn open(connection: &mut Connection, input: &DeviceId) -> Result<Stream, Error> {
    let spec = Spec {
        format: Format::F32le,
        rate: UPDATES_PER_SECOND,
        channels: 1,
    };
    // As GNOME does: a fragment of one peak, so each one comes at once.
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: u32::try_from(PEAK_SIZE).unwrap_or(u32::MAX),
    };
    let mut stream = Stream::new(&mut connection.context, "Peak detect", &spec, None)
        .ok_or(Error::Setup("cannot create a level meter stream"))?;
    stream
        .connect_record(
            Some(input.as_str()),
            Some(&attr),
            FlagSet::PEAK_DETECT | FlagSet::DONT_MOVE | FlagSet::ADJUST_LATENCY,
        )
        .map_err(|error| Error::Refused {
            what: format!("measure {input}"),
            reason: describe(error),
        })?;
    Ok(stream)
}

/// Reads every peak waiting in the stream and returns the loudest.
fn drain(stream: &mut Stream) -> Option<f32> {
    let mut loudest: Option<f32> = None;
    while stream.readable_size().is_some_and(|size| size > 0) {
        match stream.peek() {
            Ok(PeekResult::Data(bytes)) => {
                for chunk in bytes.chunks_exact(PEAK_SIZE) {
                    let mut raw = [0; PEAK_SIZE];
                    raw.copy_from_slice(chunk);
                    let peak = f32::from_le_bytes(raw).abs();
                    loudest = Some(loudest.map_or(peak, |l| l.max(peak)));
                }
                let _ = stream.discard();
            }
            Ok(PeekResult::Hole(_)) => {
                let _ = stream.discard();
            }
            Ok(PeekResult::Empty) | Err(_) => break,
        }
    }
    loudest
}

/// Runs one turn of the main loop, waiting at most [`WAKE_UP`].
fn turn(connection: &mut Connection) -> Result<(), Error> {
    let mainloop = &mut connection.mainloop;
    mainloop
        .prepare(Some(WAKE_UP))
        .and_then(|()| mainloop.poll())
        .and_then(|_| mainloop.dispatch())
        .map(|_| ())
        .map_err(|error| Error::Mainloop(describe(error)))
}

/// The body of the meter thread.
fn run(
    inputs: &[DeviceId],
    stop: &AtomicBool,
    events: &Sender<Event>,
    ready: &Sender<Result<(), Error>>,
) {
    let mut connection = match connect() {
        Ok(connection) => connection,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    // Declared after the connection, so the streams are dropped first.
    let mut meters: Vec<(DeviceId, Stream, bool)> = Vec::new();
    for input in inputs {
        match open(&mut connection, input) {
            Ok(stream) => meters.push((input.clone(), stream, true)),
            Err(error) => {
                let _ = ready.send(Err(error));
                return;
            }
        }
    }
    let _ = ready.send(Ok(()));

    while !stop.load(Ordering::Relaxed) {
        if let Err(error) = turn(&mut connection) {
            let _ = events.send(Event::Failed(error));
            break;
        }
        if matches!(
            connection.context.get_state(),
            ContextState::Failed | ContextState::Terminated
        ) {
            let reason = describe(connection.context.errno());
            let _ = events.send(Event::Failed(Error::Disconnected(reason)));
            break;
        }
        for (input, stream, alive) in &mut meters {
            if !*alive {
                continue;
            }
            let sent = match stream.get_state() {
                State::Ready => match drain(stream) {
                    Some(peak) => events.send(Event::Peak {
                        input: input.clone(),
                        peak,
                    }),
                    None => Ok(()),
                },
                State::Failed | State::Terminated => {
                    *alive = false;
                    events.send(Event::Lost(input.clone()))
                }
                State::Unconnected | State::Creating => Ok(()),
            };
            // Nobody listens any more: stop.
            if sent.is_err() {
                return;
            }
        }
    }
    for (_, stream, alive) in &mut meters {
        if *alive {
            let _ = stream.disconnect();
        }
    }
    connection.context.disconnect();
}
