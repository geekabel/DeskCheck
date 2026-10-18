//! Acting on arrival at a place (DC-28).
//!
//! Each time the screens or the Wi-Fi change, the place is recognized
//! again ([`deskcheck_core::places::detect`]). DeskCheck acts only once
//! what it sees stays the same for a moment ([`Settle`]): plugging a
//! screen makes GNOME change the screens in a few steps, and the Wi-Fi can
//! drop for a moment. On arrival, as the place
//! says: its choices are applied and a notification offers Undo, or the
//! notification asks first, or nothing is done. The line at the bottom of
//! the Now page says the same.
//!
//! The notification is sent by [`crate::notify`]: its buttons run the
//! actions `app.place-undo` and `app.place-apply`, as the GIO
//! documentation asks.
//! GNOME shows it only for an app with a `.desktop` file named after its
//! id (see `data/`).
//!
//! Each arrival is written in the log of `GLib`, domain `deskcheck`: run the
//! app with `G_MESSAGES_DEBUG=deskcheck` to see why a place was, or was
//! not, recognized.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use deskcheck_config::Config;
use deskcheck_core::audio::AudioState;
use deskcheck_core::display::DisplayState;
use deskcheck_core::places::{
    self, Arrivals, Observed, OnArrival, Place, Settle, Settling, Snapshot,
};
use gtk::glib;

use crate::backend::{Backend, PlaceResult, Request};
use crate::notify::{self, Note};
use crate::places_model::{self, APPLY_ACTION, LineState, PlaceLine, UNDO_ACTION};

/// The id of the notification: a new one replaces the old one.
const NOTIFICATION: &str = "place";

/// The log domain of DeskCheck.
const LOG: &str = "deskcheck";

/// How long to wait before showing a notification: a screen just plugged
/// in makes GNOME lay out its panels again, and a banner shown meanwhile
/// can be lost.
const NOTIFY_DELAY: Duration = Duration::from_millis(1500);

/// The place recognized, and what DeskCheck did about it.
pub struct PlaceWatch {
    app: adw::Application,
    backend: Backend,
    arrivals: RefCell<Arrivals>,
    line: RefCell<Option<PlaceLine>>,
    /// What to put back on Undo.
    undo: RefCell<Option<Snapshot>>,
    /// The place to apply when the user says so.
    asked: RefCell<Option<Place>>,
    /// `true` until the first place seen after DeskCheck starts is
    /// applied: then a place already in use is not worth a notification.
    starting: Cell<bool>,
    /// What DeskCheck acted on last: the place and the screens plugged
    /// in (see [`settle_key`]).
    acted: RefCell<Option<String>>,
    /// `true` while the place is applied again because its screens
    /// changed: a result that changes nothing is then not told.
    refreshing: Cell<bool>,
    /// The screens plugged in, as written in the log last.
    screens_logged: RefCell<String>,
    /// The notification waiting for [`NOTIFY_DELAY`].
    pending: Rc<RefCell<Option<glib::SourceId>>>,
    /// The place seen, waiting to stay the same.
    settle: RefCell<Settle>,
    /// What DeskCheck will act on when the wait ends: the place seen and
    /// why, or nothing.
    seen: RefCell<Option<(Place, String)>>,
    /// The end of the wait.
    settle_timer: RefCell<Option<glib::SourceId>>,
}

/// The time of day, like "8:31".
fn now_text() -> String {
    glib::DateTime::now_local()
        .and_then(|now| now.format("%-H:%M"))
        .map_or_else(|_| String::new(), |text| text.to_string())
}

impl PlaceWatch {
    /// Nothing recognized yet.
    pub fn new(app: &adw::Application, backend: Backend) -> Self {
        Self {
            app: app.clone(),
            backend,
            arrivals: RefCell::default(),
            line: RefCell::default(),
            undo: RefCell::default(),
            asked: RefCell::default(),
            starting: Cell::new(true),
            acted: RefCell::default(),
            refreshing: Cell::new(false),
            screens_logged: RefCell::default(),
            pending: Rc::default(),
            settle: RefCell::default(),
            seen: RefCell::default(),
            settle_timer: RefCell::default(),
        }
    }

    /// The line for the Now page, if a place is recognized.
    pub fn line(&self) -> Option<PlaceLine> {
        self.line.borrow().clone()
    }

    /// A place was renamed on the Places page: still the same place.
    pub fn renamed(&self, from: &str, to: &str) {
        self.arrivals.borrow_mut().rename(from, to);
        if let Some(line) = self.line.borrow_mut().as_mut() {
            if line.name == from {
                to.clone_into(&mut line.name);
            }
        }
    }

    /// The name of the place recognized.
    pub fn current(&self) -> Option<String> {
        self.arrivals.borrow().current().map(str::to_owned)
    }

    /// Recognizes the place again. Called once both the screens and the
    /// Wi-Fi were read, so DeskCheck does not apply a place it would
    /// change its mind about a moment later. It acts when what it sees
    /// stays the same (see [`Settle`]).
    pub fn update(
        self: &Rc<Self>,
        config: &Config,
        screens: Option<&Result<DisplayState, String>>,
        wifi: Option<&str>,
    ) {
        let Some(screens) = screens else {
            return;
        };
        let observed = Observed::new(screens.as_ref().ok(), wifi.map(str::to_owned));
        let found = places::detect(config.places(), &observed).and_then(|found| {
            let place = config.places().get(found.index)?;
            Some((place.clone(), places_model::because(config, &found.because)))
        });
        let plugged = plugged_screens(config, &observed);
        if *self.screens_logged.borrow() != plugged {
            glib::g_debug!(LOG, "screens plugged in: {plugged}");
            self.screens_logged.replace(plugged.clone());
        }
        let name = found.as_ref().map(|(place, _)| place.name.clone());
        // The screens count too: plugging a screen back at the same place
        // applies the place again, for its layout.
        let key = name.as_deref().map(|name| settle_key(name, &plugged));
        let current = self.acted.borrow().clone();
        let settling = self
            .settle
            .borrow_mut()
            .see(key.as_deref(), current.as_deref());
        match settling {
            Settling::Keep => {
                // The same place, maybe with newer settings.
                self.seen.replace(found);
            }
            Settling::Cancel => {
                if let Some(timer) = self.settle_timer.take() {
                    timer.remove();
                    glib::g_debug!(
                        LOG,
                        "place: nothing changed, still at {}",
                        self.current().unwrap_or_default()
                    );
                }
                self.seen.replace(None);
            }
            Settling::Wait(delay) => {
                glib::g_debug!(
                    LOG,
                    "place: seen {}, acting in {} s if it stays",
                    found.as_ref().map_or_else(
                        || "no place".to_owned(),
                        |(place, because)| format!("{}, because {because}", place.name)
                    ),
                    delay.as_secs()
                );
                self.seen.replace(found);
                if let Some(timer) = self.settle_timer.take() {
                    timer.remove();
                }
                let watch = Rc::downgrade(self);
                let timer = glib::timeout_add_local_once(delay, move || {
                    if let Some(watch) = watch.upgrade() {
                        watch.settle_timer.replace(None);
                        watch.settle.borrow_mut().done();
                        let seen = watch.seen.take();
                        watch.acted.replace(key.clone());
                        watch.act(seen.as_ref());
                    }
                });
                self.settle_timer.replace(Some(timer));
            }
        }
    }

    /// Acts on the place seen once it stayed the same.
    fn act(&self, seen: Option<&(Place, String)>) {
        let name = seen.as_ref().map(|(place, _)| place.name.as_str());
        let before = self.current();
        if self.arrivals.borrow_mut().see(name) {
            if let Some((place, because)) = seen {
                glib::g_debug!(LOG, "place: arrived at {}, because {because}", place.name);
                self.arrive(place, because.clone());
            }
        } else if name.is_none() && self.line.borrow().is_some() {
            glib::g_debug!(LOG, "place: left {}", before.unwrap_or_default());
            // Left the place: its Undo no longer makes sense.
            self.forget();
        } else if let Some((place, because)) = seen {
            glib::g_debug!(
                LOG,
                "place: still at {}, now because {because}; its screens changed",
                place.name
            );
            self.refresh(place, because);
        }
    }

    /// The screens changed, but not the place: say why it is recognized
    /// now, and apply it again (a screen plugged back takes its place in
    /// the layout). Nothing is asked again.
    fn refresh(&self, place: &Place, because: &str) {
        if let Some(line) = self.line.borrow_mut().as_mut() {
            because.clone_into(&mut line.because);
        }
        if place.on_arrival == OnArrival::Apply && self.asked.borrow().is_none() {
            self.refreshing.set(true);
            if let Some(line) = self.line.borrow_mut().as_mut() {
                line.state = LineState::Applying;
            }
            self.backend
                .request(Request::ApplyPlace(Box::new(place.clone())));
        }
    }

    fn forget(&self) {
        self.line.replace(None);
        self.undo.replace(None);
        self.asked.replace(None);
        self.withdraw_now();
    }

    /// Sends the notification of the place, replacing the one before,
    /// after [`NOTIFY_DELAY`].
    fn notify(&self, note: Note) {
        self.cancel_pending();
        let app = self.app.clone();
        let pending = Rc::clone(&self.pending);
        let source = glib::timeout_add_local_once(NOTIFY_DELAY, move || {
            pending.replace(None);
            glib::g_debug!(LOG, "place: notification sent");
            notify::send(&app, NOTIFICATION, &note);
        });
        self.pending.replace(Some(source));
    }

    /// Forgets a notification not sent yet.
    fn cancel_pending(&self) {
        if let Some(source) = self.pending.take() {
            source.remove();
        }
    }

    /// Takes the notification of the place away, and the one waiting.
    fn withdraw_now(&self) {
        self.cancel_pending();
        glib::g_debug!(LOG, "place: notification withdrawn");
        notify::withdraw(&self.app, NOTIFICATION);
    }

    fn arrive(&self, place: &Place, because: String) {
        self.forget();
        if place.on_arrival != OnArrival::Apply {
            // Only an applied place waits for its result to say so.
            self.starting.set(false);
        }
        let state = match place.on_arrival {
            OnArrival::Apply => {
                self.backend
                    .request(Request::ApplyPlace(Box::new(place.clone())));
                LineState::Applying
            }
            OnArrival::Ask => {
                self.notify(
                    Note::new(
                        format!("Place recognized: {}", place.name),
                        format!("Because {because}. Apply its settings?"),
                    )
                    .button("Apply", APPLY_ACTION),
                );
                self.asked.replace(Some(place.clone()));
                LineState::Asking
            }
            OnArrival::Nothing => LineState::Shown,
        };
        self.line.replace(Some(PlaceLine {
            name: place.name.clone(),
            because,
            state,
        }));
    }

    /// The backend applied a place. Returns the problems to show, if any.
    pub fn applied(
        &self,
        result: PlaceResult,
        config: &Config,
        sound: Option<&AudioState>,
    ) -> Option<String> {
        let problems = (!result.problems.is_empty()).then(|| result.problems.join(" "));
        glib::g_debug!(
            LOG,
            "place: {} applied, changes kept for Undo: {:?}, problems: {:?}",
            result.name,
            result.undo,
            result.problems
        );
        let starting = self.starting.replace(false) || self.refreshing.replace(false);
        let mut line = self.line.borrow_mut();
        // The place may have changed meanwhile.
        let Some(line) = line.as_mut().filter(|line| line.name == result.name) else {
            return problems;
        };
        let title = format!("Place recognized: {}", result.name);
        if result.undo.is_empty() {
            line.state = if problems.is_none() {
                LineState::InUse
            } else {
                LineState::Shown
            };
            // Each arrival is told, even when nothing had to change; but
            // not when DeskCheck just started and all is in place already.
            if !starting || problems.is_some() {
                let body = problems
                    .as_deref()
                    .unwrap_or("Its settings are already in use.");
                self.notify(Note::new(title, body));
            }
            return problems;
        }
        line.state = LineState::Applied { at: now_text() };
        let body = places_model::applied_body(config, sound, &result.sound, result.layout_changed);
        self.notify(Note::new(title, body).button("Undo", UNDO_ACTION));
        self.undo.replace(Some(result.undo));
        problems
    }

    /// Applies the place DeskCheck asked about.
    pub fn apply_asked(&self) {
        let Some(place) = self.asked.take() else {
            return;
        };
        self.withdraw_now();
        if let Some(line) = self.line.borrow_mut().as_mut() {
            line.state = LineState::Applying;
        }
        self.backend.request(Request::ApplyPlace(Box::new(place)));
    }

    /// Puts back what was in use before the place.
    pub fn undo(&self) {
        let Some(snapshot) = self.undo.take() else {
            return;
        };
        self.withdraw_now();
        if let Some(line) = self.line.borrow_mut().as_mut() {
            line.state = LineState::Undone;
        }
        self.backend.request(Request::Undo(Box::new(snapshot)));
    }

    /// Takes the notification away, when DeskCheck quits.
    pub fn withdraw(&self) {
        self.withdraw_now();
    }

    /// Sends a test notification at once, the same way as the ones of the
    /// places: to tell a problem of GNOME from a problem of DeskCheck.
    pub fn test_notification(&self) {
        glib::g_debug!(LOG, "test notification sent");
        notify::send(
            &self.app,
            "test",
            &Note::new(
                "DeskCheck test",
                "If you see this, DeskCheck can show its notifications.",
            ),
        );
    }
}

/// The screens plugged in, by name, in a stable order.
fn plugged_screens(config: &Config, observed: &Observed) -> String {
    let mut names: Vec<String> = observed
        .screens
        .iter()
        .map(|id| places_model::screen_name(config, id))
        .collect();
    names.sort();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

/// What DeskCheck acts on: a place, and the screens plugged in.
fn settle_key(place: &str, screens: &str) -> String {
    format!("{place} with {screens}")
}
