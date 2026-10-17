//! Identify the screens (DC-24): each real screen shows its number, its
//! color and its name for a few seconds, the same as everywhere in
//! DeskCheck, as in the design.
//!
//! One window per screen, without decorations, made fullscreen on that
//! screen with `gtk::Window::fullscreen_on_monitor`. Its background is
//! clear, so the desktop stays visible around the number. Esc, a click or
//! the end of the countdown closes them all.
//!
//! Which `gdk::Monitor` is which screen: see
//! [`screens_model::match_monitors`].

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use deskcheck_config::Config;
use deskcheck_core::display::{DisplayState, ScreenId};
use gtk::{gdk, glib};

use crate::now_model::screen_color;
use crate::screens_model::{self, SeenMonitor};
use crate::ui::{self, TagKind};

/// How long the numbers stay, in seconds.
const SECONDS: u32 = 5;

/// The windows that show the numbers, while they are open.
#[derive(Default)]
pub struct Identifier {
    windows: RefCell<Vec<gtk::Window>>,
    countdowns: RefCell<Vec<gtk::Label>>,
    timer: RefCell<Option<glib::SourceId>>,
    left: Cell<u32>,
    css: RefCell<Option<gtk::CssProvider>>,
}

impl Identifier {
    /// Shows the number on every screen, or only on the screen `only`.
    pub fn show(
        self: &Rc<Self>,
        app: &adw::Application,
        config: &Config,
        state: &DisplayState,
        only: Option<usize>,
        mouse: Option<&ScreenId>,
    ) {
        self.close();
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let list = display.monitors();
        let monitors: Vec<gdk::Monitor> = (0..list.n_items())
            .filter_map(|at| list.item(at).and_downcast::<gdk::Monitor>())
            .collect();
        let seen: Vec<SeenMonitor> = monitors
            .iter()
            .map(|monitor| {
                let area = monitor.geometry();
                SeenMonitor {
                    connector: monitor.connector().map(|c| c.to_string()),
                    x: area.x(),
                    y: area.y(),
                }
            })
            .collect();
        let numbers = screens_model::match_monitors(state, &seen);
        let numbered = state.numbered();
        let mut css = String::new();
        for (monitor, number) in monitors.iter().zip(numbers) {
            let Some((number, screen)) = number
                .filter(|n| only.is_none_or(|only| only == *n))
                .and_then(|n| numbered.iter().find(|(m, _)| *m == n))
            else {
                continue;
            };
            let color = screen_color(config, &screen.id, *number);
            let _ = writeln!(
                css,
                "#identify-badge-{number} {{ background-color: {color}; }}"
            );
            let has_mouse = mouse == Some(&screen.id);
            let window = self.window(app, *number, config.screen_label(screen), screen, has_mouse);
            window.fullscreen_on_monitor(monitor);
            window.present();
            self.windows.borrow_mut().push(window);
        }
        let provider = gtk::CssProvider::new();
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        provider.load_from_data(&css);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        *self.css.borrow_mut() = Some(provider);
        if self.windows.borrow().is_empty() {
            return;
        }
        self.left.set(SECONDS);
        let identifier = Rc::downgrade(self);
        let timer = glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(identifier) = identifier.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let left = identifier.left.get().saturating_sub(1);
            identifier.left.set(left);
            if left == 0 {
                // The source ends here: forget it before closing.
                identifier.timer.borrow_mut().take();
                identifier.close();
                return glib::ControlFlow::Break;
            }
            for label in identifier.countdowns.borrow().iter() {
                label.set_label(&screens_model::countdown(left));
            }
            glib::ControlFlow::Continue
        });
        *self.timer.borrow_mut() = Some(timer);
    }

    /// The window of one screen: the badge, the name, the line, the tags
    /// and the countdown.
    fn window(
        self: &Rc<Self>,
        app: &adw::Application,
        number: usize,
        name: &str,
        screen: &deskcheck_core::display::Screen,
        has_mouse: bool,
    ) -> gtk::Window {
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .css_classes(["dc-identify-card"])
            .build();
        let badge = gtk::Label::builder()
            .label(number.to_string())
            .css_classes(["dc-identify-badge"])
            .halign(gtk::Align::Center)
            .build();
        badge.set_widget_name(&format!("identify-badge-{number}"));
        card.append(&badge);
        let title = gtk::Label::builder()
            .label(name)
            .css_classes(["dc-identify-name"])
            .build();
        card.append(&title);
        let line = gtk::Label::builder()
            .label(screens_model::identify_line(screen))
            .css_classes(["dc-identify-line"])
            .build();
        card.append(&line);
        if screen.is_primary() || has_mouse {
            let tags = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(6)
                .halign(gtk::Align::Center)
                .build();
            if screen.is_primary() {
                let primary = ui::tag("Primary", TagKind::WarnStrong);
                primary.add_css_class("dc-identify-tag");
                tags.append(&primary);
            }
            if has_mouse {
                let here = ui::mouse_tag("Your mouse is here");
                here.add_css_class("dc-identify-mouse");
                here.set_visible(true);
                tags.append(&here);
            }
            card.append(&tags);
        }
        let countdown = gtk::Label::builder()
            .label(screens_model::countdown(SECONDS))
            .css_classes(["dc-identify-countdown"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .margin_bottom(28)
            .build();
        self.countdowns.borrow_mut().push(countdown.clone());
        let overlay = gtk::Overlay::builder().child(&card).build();
        overlay.add_overlay(&countdown);

        let window = gtk::Window::builder()
            .application(app)
            .decorated(false)
            .title(format!("Screen {number}"))
            .css_classes(["dc-identify-window"])
            .child(&overlay)
            .build();
        let keys = gtk::EventControllerKey::new();
        let identifier = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                if let Some(identifier) = identifier.upgrade() {
                    identifier.close();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(keys);
        let click = gtk::GestureClick::new();
        let identifier = Rc::downgrade(self);
        click.connect_released(move |_, _, _, _| {
            if let Some(identifier) = identifier.upgrade() {
                identifier.close();
            }
        });
        window.add_controller(click);
        window
    }

    /// Closes every window of the numbers.
    pub fn close(&self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        self.countdowns.borrow_mut().clear();
        let windows: Vec<gtk::Window> = self.windows.borrow_mut().drain(..).collect();
        for window in windows {
            window.close();
        }
        if let (Some(provider), Some(display)) =
            (self.css.borrow_mut().take(), gdk::Display::default())
        {
            gtk::style_context_remove_provider_for_display(&display, &provider);
        }
    }
}
