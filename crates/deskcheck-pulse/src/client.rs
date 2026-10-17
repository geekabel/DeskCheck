//! Connection to the sound server and one full read of its state.
//!
//! We use the standard main loop of `libpulse-binding` and run it ourselves
//! with `iterate()` until each request is done. This needs no `unsafe` code
//! and no background thread, which is all a one-shot read needs.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use deskcheck_core::{APP_ID, APP_NAME};
use libpulse_binding as pulse;
use pulse::callbacks::ListResult;
use pulse::context::introspect::{SinkInfo, SinkInputInfo, SourceInfo, SourceOutputInfo};
use pulse::context::{Context, FlagSet, State};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::operation::State as OperationState;
use pulse::proplist::{Proplist, properties};

use crate::raw::{DeviceProps, Sink, SinkInput, Snapshot, Source, SourceOutput, StreamProps};

/// What can go wrong while reading the sound server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The PulseAudio client library could not set up its objects.
    Setup(&'static str),
    /// No sound server answered. The message comes from the library.
    Connect(String),
    /// The connection was lost while reading.
    Disconnected(String),
    /// The main loop stopped with an error.
    Mainloop(String),
    /// The server refused to give one of the lists.
    ListFailed(&'static str),
    /// The device to change is not there (any more).
    NoDevice(String),
    /// The server refused a change.
    Refused {
        /// The change, for example `unmute alsa_input.usb-Jabra-00.mono`.
        what: String,
        /// The reason the library gives.
        reason: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(what) => write!(f, "cannot set up the sound client: {what}"),
            Self::Connect(reason) => write!(
                f,
                "cannot connect to the sound server (PulseAudio or pipewire-pulse): {reason}"
            ),
            Self::Disconnected(reason) => {
                write!(f, "the sound server closed the connection: {reason}")
            }
            Self::Mainloop(reason) => write!(f, "the sound client stopped: {reason}"),
            Self::ListFailed(what) => write!(f, "the sound server did not send the {what}"),
            Self::NoDevice(id) => write!(f, "the sound device {id} is not there"),
            Self::Refused { what, reason } => {
                write!(f, "the sound server refused to {what}: {reason}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Text of a library error, from its `Display` implementation.
///
/// `PAErr` also has its own `to_string()` method, which returns an
/// `Option`; formatting goes through `Display` instead.
pub(crate) fn describe(error: pulse::error::PAErr) -> String {
    format!("{error}")
}

/// Runs the main loop until `done` returns `true`.
///
/// Fails as soon as the connection fails or the main loop stops.
pub(crate) fn run_until(
    mainloop: &mut Mainloop,
    context: &Context,
    mut done: impl FnMut() -> bool,
) -> Result<(), Error> {
    loop {
        match context.get_state() {
            State::Failed => return Err(Error::Disconnected(describe(context.errno()))),
            State::Terminated => return Err(Error::Disconnected("terminated".to_owned())),
            _ => {}
        }
        if done() {
            return Ok(());
        }
        match mainloop.iterate(true) {
            IterateResult::Success(_) => {}
            IterateResult::Quit(_) => return Err(Error::Mainloop("quit was called".to_owned())),
            IterateResult::Err(error) => return Err(Error::Mainloop(describe(error))),
        }
    }
}

/// Copies the properties that name the application of a stream.
fn stream_props(proplist: &Proplist, stream_name: Option<&str>) -> StreamProps {
    StreamProps {
        app_name: proplist.get_str(properties::APPLICATION_NAME),
        binary: proplist.get_str(properties::APPLICATION_PROCESS_BINARY),
        icon_name: proplist.get_str(properties::APPLICATION_ICON_NAME),
        app_id: proplist.get_str(properties::APPLICATION_ID),
        media_name: proplist.get_str(properties::MEDIA_NAME),
        stream_name: stream_name.map(str::to_owned),
    }
}

fn sink_input(info: &SinkInputInfo) -> SinkInput {
    SinkInput {
        index: info.index,
        sink: info.sink,
        props: stream_props(&info.proplist, info.name.as_deref()),
        volume: info.volume.avg().0,
        mute: info.mute,
    }
}

fn source_output(info: &SourceOutputInfo) -> SourceOutput {
    SourceOutput {
        index: info.index,
        source: info.source,
        props: stream_props(&info.proplist, info.name.as_deref()),
        volume: info.volume.avg().0,
        mute: info.mute,
        resample_method: info.resample_method.as_deref().map(str::to_owned),
    }
}

/// Copies the properties that tell what the hardware is.
fn device_props(proplist: &Proplist, port: Option<&str>) -> DeviceProps {
    DeviceProps {
        form_factor: proplist.get_str(properties::DEVICE_FORM_FACTOR),
        bus: proplist.get_str(properties::DEVICE_BUS),
        port: port.map(str::to_owned),
    }
}

fn sink(info: &SinkInfo) -> Option<Sink> {
    let name = info.name.as_deref()?;
    Some(Sink {
        index: info.index,
        name: name.to_owned(),
        description: info.description.as_deref().unwrap_or(name).to_owned(),
        props: device_props(
            &info.proplist,
            info.active_port
                .as_ref()
                .and_then(|port| port.name.as_deref()),
        ),
        volume: info.volume.avg().0,
        mute: info.mute,
    })
}

fn source(info: &SourceInfo) -> Option<Source> {
    let name = info.name.as_deref()?;
    Some(Source {
        index: info.index,
        name: name.to_owned(),
        description: info.description.as_deref().unwrap_or(name).to_owned(),
        props: device_props(
            &info.proplist,
            info.active_port
                .as_ref()
                .and_then(|port| port.name.as_deref()),
        ),
        monitor_of_sink: info.monitor_of_sink,
        monitor_of_sink_name: info.monitor_of_sink_name.as_deref().map(str::to_owned),
        volume: info.volume.avg().0,
        mute: info.mute,
    })
}

/// A ready connection to the sound server.
///
/// The fields drop in order: the context first, then its main loop.
pub(crate) struct Connection {
    pub(crate) context: Context,
    pub(crate) mainloop: Mainloop,
}

/// Connects to the sound server and waits until the connection is ready.
///
/// The connection never starts a sound server (no autospawn): DeskCheck
/// reports a missing server instead of hiding it.
pub(crate) fn connect() -> Result<Connection, Error> {
    let mut mainloop = Mainloop::new().ok_or(Error::Setup("cannot create the main loop"))?;
    let mut proplist = Proplist::new().ok_or(Error::Setup("cannot create a property list"))?;
    proplist
        .set_str(properties::APPLICATION_NAME, APP_NAME)
        .map_err(|()| Error::Setup("cannot set the application name"))?;
    proplist
        .set_str(properties::APPLICATION_ID, APP_ID)
        .map_err(|()| Error::Setup("cannot set the application ID"))?;
    let mut context = Context::new_with_proplist(&mainloop, APP_NAME, &proplist)
        .ok_or(Error::Setup("cannot create the context"))?;

    context
        .connect(None, FlagSet::NOAUTOSPAWN, None)
        .map_err(|error| Error::Connect(describe(error)))?;
    run_until(&mut mainloop, &context, || {
        context.get_state() == State::Ready
    })
    .map_err(|error| match error {
        Error::Disconnected(reason) => Error::Connect(reason),
        other => other,
    })?;
    Ok(Connection { context, mainloop })
}

/// Connects to the sound server and reads everything once.
pub(crate) fn read_snapshot() -> Result<Snapshot, Error> {
    let Connection {
        mut context,
        mut mainloop,
    } = connect()?;

    let snapshot = Rc::new(RefCell::new(Snapshot::default()));
    let failed: Rc<Cell<Option<&'static str>>> = Rc::new(Cell::new(None));
    let introspect = context.introspect();

    let server = {
        let snapshot = Rc::clone(&snapshot);
        introspect.get_server_info(move |info| {
            let mut snapshot = snapshot.borrow_mut();
            snapshot.default_sink = info.default_sink_name.as_deref().map(str::to_owned);
            snapshot.default_source = info.default_source_name.as_deref().map(str::to_owned);
        })
    };
    let sinks = {
        let (snapshot, failed) = (Rc::clone(&snapshot), Rc::clone(&failed));
        introspect.get_sink_info_list(move |result| match result {
            ListResult::Item(info) => snapshot.borrow_mut().sinks.extend(sink(info)),
            ListResult::End => {}
            ListResult::Error => failed.set(Some("list of outputs")),
        })
    };
    let sources = {
        let (snapshot, failed) = (Rc::clone(&snapshot), Rc::clone(&failed));
        introspect.get_source_info_list(move |result| match result {
            ListResult::Item(info) => snapshot.borrow_mut().sources.extend(source(info)),
            ListResult::End => {}
            ListResult::Error => failed.set(Some("list of inputs")),
        })
    };
    let sink_inputs = {
        let (snapshot, failed) = (Rc::clone(&snapshot), Rc::clone(&failed));
        introspect.get_sink_input_info_list(move |result| match result {
            ListResult::Item(info) => snapshot.borrow_mut().sink_inputs.push(sink_input(info)),
            ListResult::End => {}
            ListResult::Error => failed.set(Some("list of playback streams")),
        })
    };
    let source_outputs = {
        let (snapshot, failed) = (Rc::clone(&snapshot), Rc::clone(&failed));
        introspect.get_source_output_info_list(move |result| match result {
            ListResult::Item(info) => {
                snapshot
                    .borrow_mut()
                    .source_outputs
                    .push(source_output(info));
            }
            ListResult::End => {}
            ListResult::Error => failed.set(Some("list of recording streams")),
        })
    };

    run_until(&mut mainloop, &context, || {
        [
            server.get_state(),
            sinks.get_state(),
            sources.get_state(),
            sink_inputs.get_state(),
            source_outputs.get_state(),
        ]
        .iter()
        .all(|state| *state != OperationState::Running)
    })?;

    if let Some(what) = failed.get() {
        return Err(Error::ListFailed(what));
    }

    context.disconnect();
    let result = snapshot.borrow().clone();
    Ok(result)
}
