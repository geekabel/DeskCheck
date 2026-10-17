//! The main window: a header bar, a sidebar with the five sections, and a
//! page for each section, as in the design.
//!
//! libadwaita 1.1 has no `NavigationSplitView` (1.4): the sidebar is a
//! `gtk::ListBox` with the `navigation-sidebar` style, next to a
//! `gtk::Stack`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use deskcheck_config::{Config, Store};
use deskcheck_core::APP_NAME;
use deskcheck_core::audio::AudioState;
use deskcheck_core::display::DisplayState;
use deskcheck_pulse::ServerKind;

use crate::backend::Update;
use crate::sections::Section;
use crate::status;

/// Width of the sidebar, as in the design.
const SIDEBAR_WIDTH: i32 = 220;

/// The last state the backend sent.
#[derive(Default)]
struct Latest {
    server: Option<Result<ServerKind, String>>,
    sound: Option<Result<AudioState, String>>,
    screens: Option<Result<DisplayState, String>>,
}

/// The window and the widgets that change.
#[derive(Clone)]
pub struct MainWindow {
    /// The window itself.
    pub window: adw::ApplicationWindow,
    title: adw::WindowTitle,
    stack: gtk::Stack,
    now_page: adw::StatusPage,
    sound_footer: gtk::Label,
    screens_footer: gtk::Label,
    latest: Rc<RefCell<Latest>>,
}

/// A row of the sidebar: icon and title.
fn sidebar_row(section: Section) -> gtk::ListBoxRow {
    let line = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(6)
        .margin_end(6)
        .build();
    line.append(&gtk::Image::from_icon_name(section.icon_name()));
    line.append(&gtk::Label::new(Some(section.title())));
    gtk::ListBoxRow::builder().child(&line).build()
}

/// The badge of the place, in the header bar.
fn place_badge() -> gtk::Box {
    let badge = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .valign(gtk::Align::Center)
        .css_classes(["place-badge"])
        .tooltip_text("Places come in a later version")
        .build();
    badge.append(&gtk::Image::from_icon_name("mark-location-symbolic"));
    badge.append(&gtk::Label::new(Some(status::NO_PLACE)));
    badge
}

/// The menu of the window.
fn menu_button() -> gtk::MenuButton {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Refresh"), Some("app.refresh"));
    menu.append(Some("About DeskCheck"), Some("app.about"));
    menu.append(Some("Quit"), Some("app.quit"));
    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text("Menu")
        .build()
}

/// A dim, small line at the bottom of the sidebar.
fn footer_label() -> gtk::Label {
    gtk::Label::builder()
        .label("…")
        .xalign(0.0)
        .css_classes(["dim-label", "caption"])
        .build()
}

impl MainWindow {
    /// Builds the window, with the Now section shown.
    pub fn new(app: &adw::Application) -> Self {
        let title = adw::WindowTitle::new(Section::Now.title(), "");
        let header = adw::HeaderBar::builder().title_widget(&title).build();
        header.pack_start(
            &gtk::Label::builder()
                .label(APP_NAME)
                .css_classes(["heading"])
                .margin_start(6)
                .build(),
        );
        header.pack_end(&menu_button());
        header.pack_end(&place_badge());

        let stack = gtk::Stack::builder()
            .hexpand(true)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        let mut now_page = None;
        for section in Section::ALL {
            let page = adw::StatusPage::builder()
                .icon_name(section.icon_name())
                .title(section.title())
                .description(section.summary())
                .build();
            stack.add_named(&page, Some(section.id()));
            if section == Section::Now {
                now_page = Some(page);
            }
        }
        let now_page = now_page.unwrap_or_default();

        let list = gtk::ListBox::builder()
            .css_classes(["navigation-sidebar"])
            .vexpand(true)
            .build();
        for section in Section::ALL {
            list.append(&sidebar_row(section));
        }
        let sound_footer = footer_label();
        let screens_footer = footer_label();
        let footer = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_start(18)
            .margin_end(12)
            .margin_bottom(12)
            .build();
        footer.append(&sound_footer);
        footer.append(&screens_footer);
        let sidebar = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(SIDEBAR_WIDTH)
            .build();
        sidebar.append(&list);
        sidebar.append(&footer);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.set_vexpand(true);
        body.append(&sidebar);
        body.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        body.append(&stack);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header);
        content.append(&body);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(APP_NAME)
            .default_width(1120)
            .default_height(780)
            .content(&content)
            .build();

        let this = Self {
            window,
            title,
            stack,
            now_page,
            sound_footer,
            screens_footer,
            latest: Rc::default(),
        };
        {
            let this = this.clone();
            list.connect_row_selected(move |_, row| {
                if let Some(section) = row
                    .and_then(|r| usize::try_from(r.index()).ok())
                    .and_then(Section::at)
                {
                    this.show(section);
                }
            });
        }
        list.select_row(list.row_at_index(0).as_ref());
        this.render();
        this
    }

    /// Shows a section.
    pub fn show(&self, section: Section) {
        self.stack.set_visible_child_name(section.id());
        self.title.set_title(section.title());
    }

    /// Keeps what the backend sent, and shows it.
    pub fn apply(&self, update: Update) {
        {
            let mut latest = self.latest.borrow_mut();
            match update {
                Update::SoundServer(server) => latest.server = Some(server),
                Update::Sound(sound) => latest.sound = Some(sound),
                Update::Screens(screens) => latest.screens = Some(screens),
            }
        }
        self.render();
    }

    /// Shows the latest state.
    fn render(&self) {
        let latest = self.latest.borrow();
        // Read at each update: names given with `deskcheck rename` show
        // at once. A broken file only loses the names.
        let config = Store::new()
            .and_then(|store| store.load())
            .unwrap_or_else(|_| Config::default());
        let session = std::env::var("XDG_SESSION_TYPE").ok();
        self.sound_footer
            .set_label(&status::sound_footer(latest.server.as_ref()));
        self.screens_footer.set_label(&status::screens_footer(
            latest.screens.as_ref(),
            session.as_deref(),
        ));
        self.now_page.set_description(Some(&status::now_summary(
            &config,
            latest.sound.as_ref(),
            latest.screens.as_ref(),
        )));
    }
}
