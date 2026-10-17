//! Changes to the sound server: default devices, sound on or off, volume,
//! and moving an application stream to another device.
//!
//! Each change opens its own connection, sends one request and waits for
//! the answer of the server, like [`crate::read_state`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use deskcheck_core::audio::{DeviceId, Fix, StreamId, Target, Volume};
use libpulse_binding as pulse;
use pulse::callbacks::ListResult;
use pulse::operation::{Operation, State as OperationState};
use pulse::volume::{ChannelVolumes, Volume as PaVolume};

use crate::client::{Connection, Error, connect, describe, run_until};

/// One change to make on the sound server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Make this output the default one.
    SetDefaultOutput(DeviceId),
    /// Make this input the default one.
    SetDefaultInput(DeviceId),
    /// Move a playback stream to another output, without stopping it.
    MovePlayback {
        /// The stream.
        stream: StreamId,
        /// The output.
        to: DeviceId,
    },
    /// Move a recording stream to another input, without stopping it.
    MoveRecording {
        /// The stream.
        stream: StreamId,
        /// The input.
        to: DeviceId,
    },
    /// Turn the sound of a device off (`true`) or on (`false`).
    SetMute(Target, bool),
    /// Set the volume of a device. The balance between channels is kept.
    SetVolume(Target, Volume),
}

impl Action {
    /// The change that applies a fix of the diagnosis, or `None` for a fix
    /// only the user can do, like plugging in a microphone.
    #[must_use]
    pub fn from_fix(fix: &Fix) -> Option<Self> {
        match fix {
            Fix::MoveRecording { stream, to } => Some(Self::MoveRecording {
                stream: *stream,
                to: to.clone(),
            }),
            Fix::SetDefaultInput(id) => Some(Self::SetDefaultInput(id.clone())),
            Fix::Unmute(target) => Some(Self::SetMute(target.clone(), false)),
            Fix::SetVolume(target, volume) => Some(Self::SetVolume(target.clone(), *volume)),
            Fix::PlugInMicrophone => None,
        }
    }

    /// What the action does, for error messages.
    fn what(&self) -> String {
        match self {
            Self::SetDefaultOutput(id) => format!("make {id} the default output"),
            Self::SetDefaultInput(id) => format!("make {id} the default input"),
            Self::MovePlayback { stream, to } => format!("move playback stream {stream} to {to}"),
            Self::MoveRecording { stream, to } => {
                format!("move recording stream {stream} to {to}")
            }
            Self::SetMute(Target::Output(id) | Target::Input(id), true) => format!("mute {id}"),
            Self::SetMute(Target::Output(id) | Target::Input(id), false) => {
                format!("unmute {id}")
            }
            Self::SetVolume(Target::Output(id) | Target::Input(id), volume) => {
                format!("set {id} to {volume}")
            }
        }
    }
}

/// Converts a DeskCheck volume to a PulseAudio volume, capped at the
/// highest volume the server accepts.
pub(crate) fn volume_to_raw(volume: Volume) -> PaVolume {
    let norm = u64::from(PaVolume::NORMAL.0);
    let raw = (u64::from(volume.percent()) * norm + 50) / 100;
    PaVolume(u32::try_from(raw).unwrap_or(u32::MAX).min(PaVolume::MAX.0))
}

/// Runs the main loop until the operation is done, then checks that the
/// server said yes.
fn finish(
    connection: &mut Connection,
    operation: &Operation<dyn FnMut(bool)>,
    success: &Cell<Option<bool>>,
    what: &str,
) -> Result<(), Error> {
    wait(connection, operation)?;
    match success.get() {
        Some(true) => Ok(()),
        _ => Err(Error::Refused {
            what: what.to_owned(),
            reason: describe(connection.context.errno()),
        }),
    }
}

/// A callback that keeps the answer of the server.
fn keep(success: &Rc<Cell<Option<bool>>>) -> Box<dyn FnMut(bool)> {
    let success = Rc::clone(success);
    Box::new(move |ok| success.set(Some(ok)))
}

/// Runs the main loop until the operation is done.
fn wait<C: ?Sized>(connection: &mut Connection, operation: &Operation<C>) -> Result<(), Error> {
    run_until(&mut connection.mainloop, &connection.context, || {
        operation.get_state() != OperationState::Running
    })
}

/// Reads the current channel volumes of a device.
fn channel_volumes(connection: &mut Connection, target: &Target) -> Result<ChannelVolumes, Error> {
    let found: Rc<RefCell<Option<ChannelVolumes>>> = Rc::new(RefCell::new(None));
    let introspect = connection.context.introspect();
    let id = match target {
        Target::Output(id) => {
            let found = Rc::clone(&found);
            let operation = introspect.get_sink_info_by_name(id.as_str(), move |result| {
                if let ListResult::Item(info) = result {
                    *found.borrow_mut() = Some(info.volume);
                }
            });
            wait(connection, &operation)?;
            id
        }
        Target::Input(id) => {
            let found = Rc::clone(&found);
            let operation = introspect.get_source_info_by_name(id.as_str(), move |result| {
                if let ListResult::Item(info) = result {
                    *found.borrow_mut() = Some(info.volume);
                }
            });
            wait(connection, &operation)?;
            id
        }
    };
    let volumes = *found.borrow();
    volumes.ok_or_else(|| Error::NoDevice(id.to_string()))
}

/// Sends one action and waits for the answer.
fn send(connection: &mut Connection, action: &Action) -> Result<(), Error> {
    let success = Rc::new(Cell::new(None));
    let what = action.what();
    let operation = match action {
        Action::SetDefaultOutput(id) => {
            let success = Rc::clone(&success);
            connection
                .context
                .set_default_sink(id.as_str(), move |ok| success.set(Some(ok)))
        }
        Action::SetDefaultInput(id) => {
            let success = Rc::clone(&success);
            connection
                .context
                .set_default_source(id.as_str(), move |ok| success.set(Some(ok)))
        }
        Action::MovePlayback { stream, to } => connection
            .context
            .introspect()
            .move_sink_input_by_name(stream.index(), to.as_str(), Some(keep(&success))),
        Action::MoveRecording { stream, to } => connection
            .context
            .introspect()
            .move_source_output_by_name(stream.index(), to.as_str(), Some(keep(&success))),
        Action::SetMute(Target::Output(id), mute) => connection
            .context
            .introspect()
            .set_sink_mute_by_name(id.as_str(), *mute, Some(keep(&success))),
        Action::SetMute(Target::Input(id), mute) => connection
            .context
            .introspect()
            .set_source_mute_by_name(id.as_str(), *mute, Some(keep(&success))),
        Action::SetVolume(target, volume) => {
            let mut volumes = channel_volumes(connection, target)?;
            // Keeps the balance; a device at 0 on every channel gets the
            // same volume on each.
            if volumes.scale(volume_to_raw(*volume)).is_none() {
                return Err(Error::Refused {
                    what,
                    reason: "invalid channel volumes".to_owned(),
                });
            }
            let mut introspect = connection.context.introspect();
            match target {
                Target::Output(id) => {
                    introspect.set_sink_volume_by_name(id.as_str(), &volumes, Some(keep(&success)))
                }
                Target::Input(id) => introspect.set_source_volume_by_name(
                    id.as_str(),
                    &volumes,
                    Some(keep(&success)),
                ),
            }
        }
    };
    finish(connection, &operation, &success, &what)
}

/// Applies one change on the sound server.
///
/// # Errors
///
/// Returns an [`Error`] when no sound server answers, when the device is
/// gone ([`Error::NoDevice`]), or when the server refuses the change
/// ([`Error::Refused`]), for example because the stream has ended.
pub fn apply(action: &Action) -> Result<(), Error> {
    let mut connection = connect()?;
    let result = send(&mut connection, action);
    connection.context.disconnect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_convert_both_ways() {
        assert_eq!(volume_to_raw(Volume::NORMAL), PaVolume::NORMAL);
        assert_eq!(volume_to_raw(Volume::from_percent(0)), PaVolume::MUTED);
        for percent in [0, 1, 50, 100, 142, 150] {
            let volume = Volume::from_percent(percent);
            assert_eq!(
                crate::convert::volume_from_raw(volume_to_raw(volume).0),
                volume
            );
        }
        // Even the highest DeskCheck volume is a valid PulseAudio volume.
        assert!(volume_to_raw(Volume::from_percent(u16::MAX)).is_valid());
    }

    #[test]
    fn fixes_become_actions() {
        let mic = DeviceId::new("alsa_input.usb-Jabra-00.mono-fallback");
        let stream = StreamId::new(42);
        assert_eq!(
            Action::from_fix(&Fix::MoveRecording {
                stream,
                to: mic.clone()
            }),
            Some(Action::MoveRecording {
                stream,
                to: mic.clone()
            })
        );
        assert_eq!(
            Action::from_fix(&Fix::Unmute(Target::Input(mic.clone()))),
            Some(Action::SetMute(Target::Input(mic.clone()), false))
        );
        assert_eq!(
            Action::from_fix(&Fix::SetDefaultInput(mic.clone())),
            Some(Action::SetDefaultInput(mic))
        );
        assert_eq!(Action::from_fix(&Fix::PlugInMicrophone), None);
    }
}
