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
use deskcheck_mutter::setup::Setup;
use deskcheck_pulse::ServerKind;

use crate::audio_model::AudioTab;
use crate::audio_page::AudioPage;
use crate::backend::{Backend, MeterScope, Mouse, Request, Update};
use crate::diag_page::{DiagPage, Go};
use crate::identify::Identifier;
use crate::now_page::{NowLink, NowPage};
use crate::screens_page::ScreensPage;
use crate::sections::Section;
use crate::status;
use crate::ui;

/// Width of the sidebar, as in the design.
const SIDEBAR_WIDTH: i32 = 220;

/// The last state the backend sent.
#[derive(Default)]
struct Latest {
    server: Option<Result<ServerKind, String>>,
    sound: Option<Result<AudioState, String>>,
    screens: Option<Result<DisplayState, String>>,
    setup: Option<Setup>,
    mouse: Option<Mouse>,
}

/// The window and the widgets that change.
#[derive(Clone)]
pub struct MainWindow {
    /// The window itself.
    pub window: adw::ApplicationWindow,
    title: gtk::Label,
    /// The title, or the tabs of the Audio page.
    title_stack: gtk::Stack,
    stack: gtk::Stack,
    sidebar: gtk::ListBox,
    backend: Backend,
    now: Rc<NowPage>,
    audio: Rc<AudioPage>,
    screens: Rc<ScreensPage>,
    diagnostic: Rc<DiagPage>,
    toasts: adw::ToastOverlay,
    sound_footer: gtk::Label,
    screens_footer: gtk::Label,
    latest: Rc<RefCell<Latest>>,
    identifier: Rc<Identifier>,
}

/// A row of the sidebar: icon and title.
fn sidebar_row(section: Section) -> gtk::ListBoxRow {
    let line = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .build();
    line.append(&ui::icon(section.icon_name(), 18));
    line.append(&gtk::Label::new(Some(section.title())));
    gtk::ListBoxRow::builder().child(&line).build()
}

/// The name of the app with its icon, at the left of the header bar.
fn brand() -> gtk::Box {
    let brand = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .margin_start(8)
        .css_classes(["dc-brand"])
        .build();
    brand.append(&ui::icon("dc-logo-symbolic", 18));
    brand.append(&gtk::Label::new(Some(APP_NAME)));
    brand
}

/// The badge of the place, in the header bar.
fn place_badge() -> gtk::Box {
    let badge = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(5)
        .valign(gtk::Align::Center)
        .css_classes(["dc-tag", "accent"])
        .tooltip_text("Places come in a later version")
        .build();
    badge.append(&ui::icon("dc-place-symbolic", 14));
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
        .icon_name("dc-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text("Menu")
        .valign(gtk::Align::Center)
        .css_classes(["dc-round"])
        .build()
}

/// The sidebar: the sections, and two lines about the system at the
/// bottom.
fn sidebar() -> (gtk::Box, gtk::ListBox, gtk::Label, gtk::Label) {
    let list = gtk::ListBox::builder().vexpand(true).build();
    for section in Section::ALL {
        list.append(&sidebar_row(section));
    }
    let sound_footer = footer_label();
    let screens_footer = footer_label();
    let footer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .css_classes(["dc-sidebar-footer"])
        .build();
    footer.append(&sound_footer);
    footer.append(&screens_footer);
    let sidebar = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(SIDEBAR_WIDTH)
        .css_classes(["dc-sidebar"])
        .build();
    sidebar.append(&list);
    sidebar.append(&footer);

    (sidebar, list, sound_footer, screens_footer)
}

/// The page of a section that comes in a later version.
fn coming_soon(section: Section) -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name(section.icon_name())
        .title(section.title())
        .description(section.summary())
        .build()
}

/// A small line at the bottom of the sidebar.
fn footer_label() -> gtk::Label {
    gtk::Label::builder().label("…").xalign(0.0).build()
}

impl MainWindow {
    /// Builds the window, with the Now section shown.
    pub fn new(app: &adw::Application, backend: Backend) -> Self {
        let title = gtk::Label::builder()
            .label(Section::Now.title())
            .css_classes(["dc-header-title"])
            .build();
        let title_stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .hhomogeneous(false)
            .build();
        title_stack.add_named(&title, Some("title"));
        let header = adw::HeaderBar::builder().title_widget(&title_stack).build();
        header.pack_start(&brand());
        header.pack_end(&menu_button());
        header.pack_end(&place_badge());

        let stack = gtk::Stack::builder()
            .hexpand(true)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        let now = NowPage::new(backend.clone());
        let audio = AudioPage::new(backend.clone());
        title_stack.add_named(&audio.tabs, Some(Section::Audio.id()));
        let screens = ScreensPage::new(backend.clone());
        let diagnostic = DiagPage::new(backend.clone());
        title_stack.add_named(&screens.tabs, Some(Section::Screens.id()));
        for section in Section::ALL {
            let page: gtk::Widget = match section {
                Section::Now => now.root.clone().upcast(),
                Section::Audio => audio.root.clone().upcast(),
                Section::Screens => screens.root.clone().upcast(),
                Section::Diagnostic => diagnostic.root.clone().upcast(),
                Section::Places => coming_soon(section).upcast(),
            };
            stack.add_named(&page, Some(section.id()));
        }

        let (sidebar, list, sound_footer, screens_footer) = sidebar();

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.set_vexpand(true);
        body.append(&sidebar);
        body.append(&stack);

        // Short messages, like the result of a sound test, over the body.
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&body));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header);
        content.append(&toasts);

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
            title_stack,
            stack,
            sidebar: list.clone(),
            backend,
            now,
            audio,
            screens,
            diagnostic,
            toasts,
            sound_footer,
            screens_footer,
            latest: Rc::default(),
            identifier: Rc::default(),
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
        {
            let this = this.clone();
            this.now.clone().connect_link(move |link| match link {
                NowLink::Audio(tab) => this.show_audio(tab),
                NowLink::Diagnostic => this.select(Section::Diagnostic),
            });
        }
        {
            let this = this.clone();
            this.diagnostic.clone().connect_go(move |to| match to {
                Go::Now => this.select(Section::Now),
                Go::Audio(tab) => this.show_audio(tab),
            });
        }
        list.select_row(list.row_at_index(0).as_ref());
        this.render();
        this
    }

    /// Shows a section. The Audio page has its tabs in the header bar, and
    /// measures every source while it is shown.
    pub fn show(&self, section: Section) {
        self.stack.set_visible_child_name(section.id());
        self.title.set_label(section.title());
        // The Audio and Screens pages have tabs in the header bar.
        let (title, scope) = match section {
            Section::Audio => (section.id(), MeterScope::Everything),
            Section::Screens => (section.id(), MeterScope::Now),
            // The voice check listens to the best microphone, maybe not the
            // default one.
            Section::Diagnostic => ("title", MeterScope::Everything),
            _ => ("title", MeterScope::Now),
        };
        self.title_stack.set_visible_child_name(title);
        self.backend.request(Request::Meter(scope));
    }

    /// Shows a tab of the Audio page, and selects Audio in the sidebar.
    pub fn show_audio(&self, tab: AudioTab) {
        self.audio.show_tab(tab);
        self.select(Section::Audio);
    }

    /// Selects a section in the sidebar, which shows it.
    pub fn select(&self, section: Section) {
        let index = Section::ALL
            .iter()
            .position(|s| *s == section)
            .and_then(|at| i32::try_from(at).ok())
            .unwrap_or(0);
        self.sidebar
            .select_row(self.sidebar.row_at_index(index).as_ref());
    }

    /// Keeps what the backend sent, and shows it.
    pub fn apply(&self, update: Update) {
        {
            let mut latest = self.latest.borrow_mut();
            match update {
                Update::SoundServer(server) => latest.server = Some(server),
                Update::Sound(sound) => latest.sound = Some(sound),
                Update::Screens(screens) => latest.screens = Some(screens),
                Update::Setup(setup) => latest.setup = setup,
                // The mouse moved to another screen: only the tags change.
                Update::Mouse(mouse) => {
                    self.now.show_mouse(&mouse);
                    self.screens.show_mouse(&mouse);
                    latest.mouse = Some(mouse);
                    return;
                }
                // Peaks come 25 times a second: only the meter moves.
                Update::Peak { source, peak } => {
                    self.now.peak(&source, peak);
                    self.audio.peak(&source, peak);
                    self.diagnostic.peak(&source, peak);
                    return;
                }
                Update::Message(text) => {
                    let toast = adw::Toast::new(&text);
                    toast.set_timeout(6);
                    self.toasts.add_toast(toast);
                    return;
                }
            }
        }
        self.render();
    }

    /// Shows its number on every screen, or only on screen `only`
    /// (DC-24).
    pub fn identify(&self, only: Option<usize>) {
        let latest = self.latest.borrow();
        let Some(Ok(screens)) = latest.screens.as_ref() else {
            return;
        };
        let Some(app) = self.window.application().and_downcast::<adw::Application>() else {
            return;
        };
        let config = Store::new()
            .and_then(|store| store.load())
            .unwrap_or_else(|_| Config::default());
        let mouse = match &latest.mouse {
            Some(Mouse::On(id)) => Some(id),
            _ => None,
        };
        self.identifier.show(&app, &config, screens, only, mouse);
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
        self.now.show_sound(&config, latest.sound.as_ref());
        self.audio.show_sound(&config, latest.sound.as_ref());
        self.now.show_screens(&config, latest.screens.as_ref());
        self.screens.show(
            &config,
            latest.screens.as_ref(),
            latest.sound.as_ref(),
            latest.setup.as_ref(),
        );
        self.diagnostic
            .show(&config, latest.sound.as_ref(), latest.screens.as_ref());
    }
}
