//! The DeskCheck window.
//!
//! It targets Ubuntu 22.04: GTK 4.6 and libadwaita 1.1. The `v4_6` and
//! `v1_1` features of the bindings keep to what those versions have, so a
//! newer call does not compile. For now the window only shows a header bar
//! and a welcome page; the pages come with the next tasks.

use adw::prelude::*;
use deskcheck_core::{APP_ID, APP_NAME};
use gtk::glib;

/// Default size of the window, in logical pixels.
const DEFAULT_SIZE: (i32, i32) = (900, 600);

/// Builds the main window of the app.
fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    // `adw::ApplicationWindow` has no header bar of its own: the content
    // brings one (libadwaita 1.1 has no `ToolbarView` yet).
    let header = adw::HeaderBar::builder()
        .title_widget(&adw::WindowTitle::new(APP_NAME, "Sound and screens"))
        .build();
    let welcome = adw::StatusPage::builder()
        .icon_name("audio-speakers-symbolic")
        .title(APP_NAME)
        .description(
            "See which device you hear, which microphone hears you, and which screen is which.",
        )
        .vexpand(true)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&welcome);

    adw::ApplicationWindow::builder()
        .application(app)
        .title(APP_NAME)
        .default_width(DEFAULT_SIZE.0)
        .default_height(DEFAULT_SIZE.1)
        .content(&content)
        .build()
}

fn main() -> glib::ExitCode {
    // `adw::Application` also starts libadwaita (styles, dark mode).
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| build_window(app).present());
    app.run()
}
