//! The DeskCheck window.
//!
//! It targets Ubuntu 22.04: GTK 4.6 and libadwaita 1.1. The `v4_6` and
//! `v1_1` features of the bindings keep to what those versions have, so a
//! newer call does not compile.
//!
//! The look of the design is in `resources/style.css`, with the icons of
//! the mockups, built into the app by `build.rs`.
//!
//! The interface thread only shows things. The system work runs in the
//! [`backend`] thread, which sends its results on a channel the interface
//! awaits.

use adw::prelude::*;
use deskcheck_core::{APP_ID, APP_NAME, VERSION};
use gtk::{gio, glib};

mod audio_model;
mod audio_page;
mod backend;
mod diag_model;
mod diag_page;
mod identify;
mod notify;
mod now_model;
mod now_page;
mod place_watch;
mod places_model;
mod places_page;
mod plug_watch;
mod quick_panel;
mod screens_model;
mod screens_page;
mod sections;
mod status;
mod tray;
mod tray_model;
mod ui;
mod window;

fn about(window: &adw::ApplicationWindow) {
    // `adw::AboutWindow` is libadwaita 1.2; Ubuntu 22.04 has 1.1.
    gtk::AboutDialog::builder()
        .transient_for(window)
        .modal(true)
        .program_name(APP_NAME)
        .version(VERSION)
        .comments("See and fix your sound devices and screens.")
        .logo_icon_name("dc-logo-symbolic")
        .authors(["Godwin"])
        .license_type(gtk::License::MitX11)
        .website("https://github.com/geekabel/DeskCheck")
        .build()
        .present();
}

/// Adds the actions of the menu and their shortcuts.
fn add_actions(app: &adw::Application, window: &window::MainWindow, backend: &backend::Backend) {
    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate({
        let app = app.clone();
        move |_, _| app.quit()
    });
    let refresh = gio::SimpleAction::new("refresh", None);
    refresh.connect_activate({
        let backend = backend.clone();
        move |_, _| backend.request(backend::Request::Refresh)
    });
    let about_action = gio::SimpleAction::new("about", None);
    about_action.connect_activate({
        let window = window.window.clone();
        move |_, _| about(&window)
    });
    // A number: 0 for every screen, else the screen to identify alone.
    let identify = gio::SimpleAction::new("identify", Some(glib::VariantTy::INT32));
    identify.connect_activate({
        let window = window.clone();
        move |_, parameter| {
            let only = parameter
                .and_then(glib::Variant::get::<i32>)
                .and_then(|number| usize::try_from(number).ok())
                .filter(|number| *number > 0);
            window.identify(only);
        }
    });
    // The buttons of the line and of the notification of a place (DC-28).
    let place_undo = gio::SimpleAction::new("place-undo", None);
    place_undo.connect_activate({
        let window = window.clone();
        move |_, _| window.place_undo()
    });
    let place_apply = gio::SimpleAction::new("place-apply", None);
    place_apply.connect_activate({
        let window = window.clone();
        move |_, _| window.place_apply()
    });
    app.connect_shutdown({
        let window = window.clone();
        move |_| {
            window.withdraw_place();
            window.stop_top_bar();
        }
    });
    // No button: run with gdbus, to check that notifications reach GNOME.
    let test_notification = gio::SimpleAction::new("test-notification", None);
    test_notification.connect_activate({
        let window = window.clone();
        move |_, _| window.test_notification()
    });
    app.add_action(&test_notification);
    // The buttons of the quick panel (DC-30).
    let open_window = gio::SimpleAction::new("open-window", None);
    open_window.connect_activate({
        let window = window.clone();
        move |_, _| window.tray_event(tray::TrayEvent::OpenWindow)
    });
    let diagnostic = gio::SimpleAction::new("diagnostic", None);
    diagnostic.connect_activate({
        let window = window.clone();
        move |_, _| window.tray_event(tray::TrayEvent::Diagnostic)
    });
    // The buttons of the notification of a headset plugged in (DC-31).
    let plug_undo = gio::SimpleAction::new("plug-undo", None);
    plug_undo.connect_activate({
        let window = window.clone();
        move |_, _| window.plug_undo()
    });
    let plug_ok = gio::SimpleAction::new("plug-ok", None);
    plug_ok.connect_activate({
        let window = window.clone();
        move |_, _| window.plug_ok()
    });
    app.add_action(&plug_undo);
    app.add_action(&plug_ok);
    app.add_action(&open_window);
    app.add_action(&diagnostic);
    app.add_action(&place_undo);
    app.add_action(&place_apply);
    app.add_action(&identify);
    app.add_action(&quit);
    app.add_action(&refresh);
    app.add_action(&about_action);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    app.set_accels_for_action("app.refresh", &["F5"]);
}

fn activate(app: &adw::Application) {
    // A second launch only shows the window that is already open.
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    notify::log_environment();
    let (updates, received) = async_channel::unbounded();
    let backend = backend::start(updates);
    let window = window::MainWindow::new(app, backend.clone());
    add_actions(app, &window, &backend);
    {
        let window = window.clone();
        glib::spawn_future_local(async move {
            while let Ok(update) = received.recv().await {
                window.apply(update);
            }
        });
    }
    start_top_bar(app, &window);
    window.window.present();
}

/// Puts the icon of DeskCheck in the top bar (DC-30). With it, closing
/// the window only hides it: DeskCheck stays in the top bar until "Quit".
fn start_top_bar(app: &adw::Application, window: &window::MainWindow) {
    let (events, received) = async_channel::unbounded();
    match tray::TopBar::start(events) {
        Ok(top_bar) => {
            glib::g_debug!("deskcheck", "top bar: icon shown");
            window.set_top_bar(top_bar);
            window.window.set_hide_on_close(true);
            // `quit` still ends DeskCheck, whatever holds it.
            std::mem::forget(app.hold());
        }
        Err(error) => {
            glib::g_debug!("deskcheck", "top bar: no icon: {error}");
            return;
        }
    }
    let window = window.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = received.recv().await {
            window.tray_event(event);
        }
    });
}

fn main() -> glib::ExitCode {
    // The styles and icons of the design (see build.rs). Registered before
    // the app starts: libadwaita then loads `style.css` and
    // `style-dark.css`, and GTK finds the icons in `icons/`, both under the
    // resource path of the app id.
    gio::resources_register_include!("deskcheck.gresource")
        .expect("the resources are built into the app");
    // `adw::Application` also starts libadwaita (styles, dark mode).
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(activate);
    app.run()
}
