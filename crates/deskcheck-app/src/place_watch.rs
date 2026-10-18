//! Acting on arrival at a place (DC-28).
//!
//! Each time the screens or the Wi-Fi change, the place is recognized
//! again ([`deskcheck_core::places::detect`]). On arrival, as the place
//! says: its choices are applied and a notification offers Undo, or the
//! notification asks first, or nothing is done. The line at the bottom of
//! the Now page says the same.
//!
//! The notification is a `gio::Notification`: its buttons run the actions
//! `app.place-undo` and `app.place-apply`, as the GIO documentation asks.
//! GNOME shows it only for an app with a `.desktop` file named after its
//! id (see `data/`).
//!
//! Each arrival is written in the GLib log, domain `deskcheck`: run the
//! app with `G_MESSAGES_DEBUG=deskcheck` to see why a place was, or was
//! not, recognized.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use deskcheck_config::Config;
use deskcheck_core::audio::AudioState;
use deskcheck_core::display::DisplayState;
use deskcheck_core::places::{self, Arrivals, Observed, OnArrival, Place, Snapshot};
use gtk::{gio, glib};

use crate::backend::{Backend, PlaceResult, Request};
use crate::places_model::{self, APPLY_ACTION, LineState, PlaceLine, UNDO_ACTION};

/// The id of the notification: a new one replaces the old one.
const NOTIFICATION: &str = "place";

/// The log domain of DeskCheck.
const LOG: &str = "deskcheck";

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

    /// Recognizes the place again, and acts on arrival. Called once both
    /// the screens and the Wi-Fi were read, so DeskCheck does not apply a
    /// place it would change its mind about a moment later.
    pub fn update(
        &self,
        config: &Config,
        screens: Option<&Result<DisplayState, String>>,
        wifi: Option<&str>,
    ) {
        let Some(screens) = screens else {
            return;
        };
        let observed = Observed::new(screens.as_ref().ok(), wifi.map(str::to_owned));
        let found = places::detect(config.places(), &observed)
            .and_then(|found| Some((config.places().get(found.index)?, found.because)));
        let name = found.as_ref().map(|(place, _)| place.name.as_str());
        let before = self.current();
        if self.arrivals.borrow_mut().see(name) {
            if let Some((place, because)) = found {
                let because = places_model::because(config, &because);
                glib::g_debug!(LOG, "place: arrived at {}, because {because}", place.name);
                self.arrive(place, because);
            }
        } else if name.is_none() && self.line.borrow().is_some() {
            glib::g_debug!(LOG, "place: left {}", before.unwrap_or_default());
            // Left the place: its Undo no longer makes sense.
            self.forget();
        }
    }

    fn forget(&self) {
        self.line.replace(None);
        self.undo.replace(None);
        self.asked.replace(None);
        self.app.withdraw_notification(NOTIFICATION);
    }

    /// Sends the notification of the place, replacing the one before.
    fn notify(&self, notification: &gio::Notification) {
        glib::g_debug!(LOG, "place: notification sent");
        self.app.send_notification(Some(NOTIFICATION), notification);
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
                let notification =
                    gio::Notification::new(&format!("Place recognized: {}", place.name));
                notification.set_body(Some(&format!("Because {because}. Apply its settings?")));
                notification.add_button("Apply", APPLY_ACTION);
                self.notify(&notification);
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
        let starting = self.starting.replace(false);
        let mut line = self.line.borrow_mut();
        // The place may have changed meanwhile.
        let Some(line) = line.as_mut().filter(|line| line.name == result.name) else {
            return problems;
        };
        let title = format!("Place recognized: {}", result.name);
        let notification = gio::Notification::new(&title);
        if result.undo.is_empty() {
            line.state = if problems.is_none() {
                LineState::InUse
            } else {
                LineState::Shown
            };
            // Each arrival is told, even when nothing had to change; but
            // not when DeskCheck just started and all is in place already.
            if !starting || problems.is_some() {
                notification.set_body(Some(
                    problems
                        .as_deref()
                        .unwrap_or("Its settings are already in use."),
                ));
                self.notify(&notification);
            }
            return problems;
        }
        line.state = LineState::Applied { at: now_text() };
        notification.set_body(Some(&places_model::applied_body(
            config,
            sound,
            &result.sound,
            result.layout_changed,
        )));
        notification.add_button("Undo", UNDO_ACTION);
        self.notify(&notification);
        self.undo.replace(Some(result.undo));
        problems
    }

    /// Applies the place DeskCheck asked about.
    pub fn apply_asked(&self) {
        let Some(place) = self.asked.take() else {
            return;
        };
        self.app.withdraw_notification(NOTIFICATION);
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
        self.app.withdraw_notification(NOTIFICATION);
        if let Some(line) = self.line.borrow_mut().as_mut() {
            line.state = LineState::Undone;
        }
        self.backend.request(Request::Undo(Box::new(snapshot)));
    }

    /// Takes the notification away, when DeskCheck quits.
    pub fn withdraw(&self) {
        self.app.withdraw_notification(NOTIFICATION);
    }
}
