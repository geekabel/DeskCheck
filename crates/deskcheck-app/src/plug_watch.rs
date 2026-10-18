//! A headset plugged in (DC-31): DeskCheck makes it the output and its
//! microphone the default one ([`deskcheck_core::audio::plugging`]), then
//! says so in a notification with Undo and OK, as in the design.
//!
//! The output and the microphone of a headset can come in two readings
//! of the sound server, so DeskCheck gathers what is plugged in for a
//! moment before it acts.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use deskcheck_config::Config;
use deskcheck_core::audio::plugging::{self, Plugged, Switch};
use deskcheck_core::audio::{AudioState, DeviceId, Form, Target};
use gtk::glib;

use crate::backend::{Backend, Request};
use crate::notify::{self, Note};

/// The id of the notification.
const NOTIFICATION: &str = "plug";

/// The action of the Undo button.
pub const UNDO_ACTION: &str = "app.plug-undo";

/// The action of the OK button.
pub const OK_ACTION: &str = "app.plug-ok";

/// How long to gather the devices of one plug.
const GATHER: Duration = Duration::from_millis(1500);

/// The log domain of DeskCheck.
const LOG: &str = "deskcheck";

/// The words of the notification: its title and its body.
pub fn plug_note(config: &Config, state: &AudioState, switch: &Switch) -> (String, String) {
    let title = match switch.form {
        Form::Headphones => "Headphones plugged in",
        _ => "Headset plugged in",
    };
    let output = switch
        .output
        .as_ref()
        .and_then(|id| state.output(id))
        .map(|output| config.output_label(output).to_owned());
    let input = switch
        .input
        .as_ref()
        .and_then(|id| state.input(id))
        .map(|input| config.input_label(input).into_owned());
    let body = match (output, input) {
        (Some(output), Some(_)) => format!("The sound and the microphone now go to {output}."),
        (Some(output), None) => format!("The sound now goes to {output}."),
        (None, Some(input)) => format!("They now hear you via {input}."),
        (None, None) => String::new(),
    };
    (title.to_owned(), body)
}

/// What was in use before the plug, to put it back.
#[derive(Debug, Clone, Default)]
struct Undo {
    output: Option<DeviceId>,
    input: Option<DeviceId>,
}

/// Watches the sound for devices plugged in.
pub struct PlugWatch {
    app: adw::Application,
    backend: Backend,
    /// The sound read last, with the settings and the outputs never used
    /// at the place.
    last: RefCell<Option<(AudioState, Config, Vec<DeviceId>)>>,
    /// The sound before the first device of this plug.
    before: RefCell<Option<AudioState>>,
    plugged: RefCell<Plugged>,
    timer: RefCell<Option<glib::SourceId>>,
    undo: RefCell<Option<Undo>>,
}

impl PlugWatch {
    /// Nothing seen yet.
    pub fn new(app: &adw::Application, backend: Backend) -> Self {
        Self {
            app: app.clone(),
            backend,
            last: RefCell::default(),
            before: RefCell::default(),
            plugged: RefCell::default(),
            timer: RefCell::default(),
            undo: RefCell::default(),
        }
    }

    /// Takes a new reading of the sound. The first one is only kept: what
    /// is there when DeskCheck starts is not "plugged in".
    pub fn see(self: &Rc<Self>, state: &AudioState, config: &Config, never: Vec<DeviceId>) {
        let previous = self
            .last
            .replace(Some((state.clone(), config.clone(), never)))
            .map(|(previous, _, _)| previous);
        let Some(previous) = previous else {
            return;
        };
        let new = Plugged::between(&previous, state);
        if new.is_empty() {
            return;
        }
        glib::g_debug!(LOG, "sound: plugged in {:?}", new);
        if self.before.borrow().is_none() {
            self.before.replace(Some(previous));
        }
        self.plugged.borrow_mut().add(&new);
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        let watch = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(GATHER, move || {
            if let Some(watch) = watch.upgrade() {
                watch.timer.replace(None);
                watch.act();
            }
        });
        self.timer.replace(Some(timer));
    }

    /// Uses what was plugged in, and tells it.
    fn act(&self) {
        let plugged = self.plugged.take();
        let before = self.before.take();
        let last = self.last.borrow();
        let (Some((state, config, never)), Some(before)) = (last.as_ref(), before) else {
            return;
        };
        let switch = plugging::switch_for(state, &plugged, never);
        if switch.is_empty() {
            glib::g_debug!(LOG, "sound: nothing to switch to");
            return;
        }
        let mut undo = Undo::default();
        if let Some(output) = &switch.output {
            if state.default_output.as_ref() != Some(output) {
                self.backend
                    .request(Request::SetDefault(Target::Output(output.clone())));
            }
            undo.output = before.default_output.clone().filter(|id| id != output);
        }
        if let Some(input) = &switch.input {
            if state.default_input.as_ref() != Some(input) {
                self.backend
                    .request(Request::SetDefault(Target::Input(input.clone())));
            }
            undo.input = before.default_input.clone().filter(|id| id != input);
        }
        let (title, body) = plug_note(config, state, &switch);
        glib::g_debug!(LOG, "sound: {title}: {body}");
        let mut note = Note::new(title, body);
        if undo.output.is_some() || undo.input.is_some() {
            note = note.button("Undo", UNDO_ACTION);
        }
        notify::send(&self.app, NOTIFICATION, &note.button("OK", OK_ACTION));
        self.undo.replace(Some(undo));
    }

    /// Puts back the output and the microphone of before the plug.
    pub fn undo(&self) {
        notify::withdraw(&self.app, NOTIFICATION);
        let Some(undo) = self.undo.take() else {
            return;
        };
        if let Some(output) = undo.output {
            self.backend
                .request(Request::SetDefault(Target::Output(output)));
        }
        if let Some(input) = undo.input {
            self.backend
                .request(Request::SetDefault(Target::Input(input)));
        }
    }

    /// Keeps the change: the notification goes away.
    pub fn ok(&self) {
        self.undo.replace(None);
        notify::withdraw(&self.app, NOTIFICATION);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{Hardware, Input, InputKind, Output, Volume};

    #[test]
    fn the_notification_says_where_the_sound_goes() {
        let mut config = Config::default();
        config.set_audio_name(&DeviceId::new("usb"), Some("USB headset".to_owned()));
        let state = AudioState {
            outputs: vec![Output {
                id: DeviceId::new("usb"),
                description: "Jabra".to_owned(),
                hardware: Hardware::default(),
                volume: Volume::NORMAL,
                muted: false,
            }],
            inputs: vec![Input {
                id: DeviceId::new("mic"),
                description: "Jabra mic".to_owned(),
                hardware: Hardware::default(),
                kind: InputKind::Capture,
                volume: Volume::NORMAL,
                muted: false,
            }],
            ..AudioState::default()
        };
        let both = Switch {
            output: Some(DeviceId::new("usb")),
            input: Some(DeviceId::new("mic")),
            form: Form::Headset,
        };
        assert_eq!(
            plug_note(&config, &state, &both),
            (
                "Headset plugged in".to_owned(),
                "The sound and the microphone now go to USB headset.".to_owned()
            )
        );
        let headphones = Switch {
            input: None,
            form: Form::Headphones,
            ..both
        };
        assert_eq!(
            plug_note(&config, &state, &headphones),
            (
                "Headphones plugged in".to_owned(),
                "The sound now goes to USB headset.".to_owned()
            )
        );
    }
}
