//! Sending notifications to GNOME Shell.
//!
//! `GLib`'s `Application::send_notification` picks a way to send them by
//! itself: the D-Bus interface `org.gtk.Notifications` of GNOME Shell,
//! the desktop portal, or `org.freedesktop.Notifications`. Inside some
//! environments, a terminal of an app installed as a snap for example,
//! `GTK_USE_PORTAL=1` is set and `GLib` goes through the portal, which may
//! drop the notifications of an app that is not sandboxed.
//!
//! So DeskCheck calls `org.gtk.Notifications` itself, the same call `GLib`
//! makes (`AddNotification` and `RemoveNotification`, with the id of the
//! app), and sees the answer of GNOME. Only when nobody owns that name,
//! outside GNOME, it falls back to `send_notification`. The buttons still
//! run the `app.` actions: GNOME Shell activates them on the app, through
//! `org.freedesktop.Application`.

use adw::prelude::*;
use gtk::{gio, glib};

/// The log domain of DeskCheck.
const LOG: &str = "deskcheck";

const SERVICE: &str = "org.gtk.Notifications";
const PATH: &str = "/org/gtk/Notifications";

/// A notification: a title, a body, and buttons that run app actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The title, in bold.
    pub title: String,
    /// The text under the title.
    pub body: String,
    /// Each button: its text and its action, like `app.place-undo`.
    pub buttons: Vec<(String, String)>,
}

impl Note {
    /// A notification without buttons.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            buttons: Vec::new(),
        }
    }

    /// Adds a button that runs an action of the app.
    #[must_use]
    pub fn button(mut self, label: &str, action: &str) -> Self {
        self.buttons.push((label.to_owned(), action.to_owned()));
        self
    }

    /// The notification as `org.gtk.Notifications` takes it: the
    /// dictionary `GLib` builds in `g_notification_serialize`.
    pub fn to_variant(&self) -> glib::Variant {
        let dict = glib::VariantDict::new(None);
        dict.insert_value("title", &self.title.to_variant());
        if !self.body.is_empty() {
            dict.insert_value("body", &self.body.to_variant());
        }
        dict.insert_value("priority", &"normal".to_variant());
        if !self.buttons.is_empty() {
            let buttons = self.buttons.iter().map(|(label, action)| {
                let button = glib::VariantDict::new(None);
                button.insert_value("label", &label.to_variant());
                button.insert_value("action", &action.to_variant());
                button.end()
            });
            dict.insert_value(
                "buttons",
                &glib::Variant::array_from_iter_with_type(glib::VariantTy::VARDICT, buttons),
            );
        }
        dict.end()
    }

    /// The same, for `Application::send_notification`.
    fn to_notification(&self) -> gio::Notification {
        let notification = gio::Notification::new(&self.title);
        if !self.body.is_empty() {
            notification.set_body(Some(&self.body));
        }
        for (label, action) in &self.buttons {
            notification.add_button(label, action);
        }
        notification
    }
}

/// `true` when the error says that nobody owns the name.
fn no_service(error: &glib::Error) -> bool {
    gio::DBusError::remote_error(error).is_some_and(|name| {
        name == "org.freedesktop.DBus.Error.ServiceUnknown"
            || name == "org.freedesktop.DBus.Error.NameHasNoOwner"
    })
}

/// Shows a notification, replacing the one with the same `id`.
pub fn send(app: &adw::Application, id: &str, note: &Note) {
    let (Some(connection), Some(app_id)) = (app.dbus_connection(), app.application_id()) else {
        app.send_notification(Some(id), &note.to_notification());
        return;
    };
    let params =
        glib::Variant::tuple_from_iter([app_id.to_variant(), id.to_variant(), note.to_variant()]);
    let app = app.clone();
    let id = id.to_owned();
    let note = note.clone();
    connection.call(
        Some(SERVICE),
        PATH,
        SERVICE,
        "AddNotification",
        Some(&params),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        move |result| match result {
            Ok(_) => glib::g_debug!(LOG, "notification {id} accepted by GNOME"),
            Err(error) if no_service(&error) => {
                glib::g_debug!(LOG, "notification {id}: no GNOME Shell, sent by GLib");
                app.send_notification(Some(&id), &note.to_notification());
            }
            Err(error) => glib::g_warning!(LOG, "notification {id} refused: {error}"),
        },
    );
}

/// Takes a notification away.
pub fn withdraw(app: &adw::Application, id: &str) {
    let (Some(connection), Some(app_id)) = (app.dbus_connection(), app.application_id()) else {
        app.withdraw_notification(id);
        return;
    };
    let params = (app_id.to_string(), id.to_owned()).to_variant();
    let app = app.clone();
    let id = id.to_owned();
    connection.call(
        Some(SERVICE),
        PATH,
        SERVICE,
        "RemoveNotification",
        Some(&params),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        move |result| {
            if let Err(error) = result {
                if no_service(&error) {
                    app.withdraw_notification(&id);
                }
            }
        },
    );
}

/// Writes in the log the settings that change how `GLib` sends
/// notifications, to explain a notification that GNOME never shows.
pub fn log_environment() {
    for name in ["GTK_USE_PORTAL", "GNOTIFICATION_BACKEND", "SNAP"] {
        if let Ok(value) = std::env::var(name) {
            glib::g_debug!(LOG, "environment: {name}={value}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_serialized_as_glib_does() {
        let note = Note::new("Place recognized: Home", "Sound: USB headset.")
            .button("Undo", "app.place-undo");
        let variant = note.to_variant();
        assert_eq!(variant.type_().as_str(), "a{sv}");
        let dict = glib::VariantDict::new(Some(&variant));
        assert_eq!(
            dict.lookup::<String>("title").unwrap().as_deref(),
            Some("Place recognized: Home")
        );
        assert_eq!(
            dict.lookup::<String>("priority").unwrap().as_deref(),
            Some("normal")
        );
        let buttons = dict.lookup_value("buttons", None).unwrap();
        assert_eq!(buttons.type_().as_str(), "aa{sv}");
        let first = glib::VariantDict::new(Some(&buttons.child_value(0)));
        assert_eq!(
            first.lookup::<String>("action").unwrap().as_deref(),
            Some("app.place-undo")
        );
        // Without buttons, there is no "buttons" key, as with `GLib`.
        let plain = glib::VariantDict::new(Some(&Note::new("Test", "").to_variant()));
        assert!(plain.lookup_value("buttons", None).is_none());
        assert!(plain.lookup_value("body", None).is_none());
    }
}
