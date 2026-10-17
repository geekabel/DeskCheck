//! The DeskCheck window.
//!
//! It targets Ubuntu 22.04: GTK 4.6 and libadwaita 1.1. The `v4_6` and
//! `v1_1` features of the bindings keep to what those versions have, so a
//! newer call does not compile.
//!
//! The interface thread only shows things. The system work runs in the
//! [`backend`] thread, which sends its results on a channel the interface
//! awaits.

use adw::prelude::*;
use deskcheck_core::{APP_ID, APP_NAME, VERSION};
use gtk::{gio, glib};

mod backend;
mod now_model;
mod now_page;
mod sections;
mod status;
mod window;

/// Styles the design needs beyond libadwaita: the badge of the place.
const CSS: &str = "
.place-badge {
    background-color: alpha(@accent_bg_color, 0.15);
    color: @accent_color;
    border-radius: 999px;
    padding: 2px 10px;
    font-weight: bold;
}
.now-card {
    padding: 16px;
}
.icon-tile {
    min-width: 44px;
    min-height: 44px;
    border-radius: 10px;
    background-color: alpha(@accent_bg_color, 0.15);
    color: @accent_color;
}
.tag {
    border-radius: 999px;
    padding: 2px 10px;
    font-size: smaller;
    font-weight: bold;
}
.tag-ok {
    background-color: alpha(@success_color, 0.15);
    color: @success_color;
}
.tag-warn {
    background-color: alpha(@warning_color, 0.18);
    color: @warning_color;
}
.tag-error {
    background-color: alpha(@error_color, 0.15);
    color: @error_color;
}
.screen-map {
    border-radius: 8px;
    background-color: alpha(@window_fg_color, 0.05);
}
.screen-tile {
    border: 2px solid @accent_color;
    border-radius: 6px;
    background-color: @card_bg_color;
}
.screen-number {
    font-weight: bold;
    font-size: larger;
}
/* Adwaita gives each block 26 px at least: 16 blocks would make the card
   too wide to shrink. */
levelbar.now-meter.horizontal.discrete > trough > block {
    min-width: 6px;
}
";

fn load_css() {
    let provider = gtk::CssProvider::new();
    // `load_from_string` is GTK 4.12; Ubuntu 22.04 has 4.6.
    #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
    provider.load_from_data(CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn about(window: &adw::ApplicationWindow) {
    // `adw::AboutWindow` is libadwaita 1.2; Ubuntu 22.04 has 1.1.
    gtk::AboutDialog::builder()
        .transient_for(window)
        .modal(true)
        .program_name(APP_NAME)
        .version(VERSION)
        .comments("See and fix your sound devices and screens.")
        .logo_icon_name("audio-speakers-symbolic")
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
    load_css();
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
    window.window.present();
}

fn main() -> glib::ExitCode {
    // `adw::Application` also starts libadwaita (styles, dark mode).
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(activate);
    app.run()
}
