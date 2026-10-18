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

use std::cell::RefCell;

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
        if self.arrivals.borrow_mut().see(name) {
            if let Some((place, because)) = found {
                self.arrive(place, places_model::because(config, &because));
            }
        } else if name.is_none() && self.line.borrow().is_some() {
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

    fn arrive(&self, place: &Place, because: String) {
        self.forget();
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
                self.app
                    .send_notification(Some(NOTIFICATION), &notification);
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
        let mut line = self.line.borrow_mut();
        // The place may have changed meanwhile.
        let Some(line) = line.as_mut().filter(|line| line.name == result.name) else {
            return problems;
        };
        if result.undo.is_empty() {
            line.state = if problems.is_none() {
                LineState::InUse
            } else {
                LineState::Shown
            };
            return problems;
        }
        line.state = LineState::Applied { at: now_text() };
        let notification = gio::Notification::new(&format!("Place recognized: {}", result.name));
        notification.set_body(Some(&places_model::applied_body(
            config,
            sound,
            &result.sound,
            result.layout_changed,
        )));
        notification.add_button("Undo", UNDO_ACTION);
        self.app
            .send_notification(Some(NOTIFICATION), &notification);
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
