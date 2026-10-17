//! The Now page: where the sound goes, which microphone hears you, which
//! apps use the sound, and your screens, as in the design.
//!
//! The page only shows things and sends requests to the backend: a change
//! comes back as a new state, which the page shows.

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use deskcheck_config::Config;
use deskcheck_core::audio::level::{LevelBar, to_db};
use deskcheck_core::audio::{AudioState, DeviceId, StreamId, Target, Volume};
use deskcheck_core::display::DisplayState;
use gtk::glib;

use crate::audio_model::{AudioTab, VOICE_DB};
use crate::backend::{Backend, Request, metered_sources};
use crate::now_model::{self, AppTag};
use crate::ui;

/// Highest volume of the slider, as GNOME with over-amplification.
const MAX_VOLUME: f64 = 150.0;

/// Wait after the last move of the slider before sending the volume.
const VOLUME_DELAY: Duration = Duration::from_millis(120);

/// The slider is not moved by new states for this long after the user
/// moved it, so it does not jump back while dragging.
const HANDS_OFF: Duration = Duration::from_millis(800);

/// Blocks of the level meters, as in the design.
const OUTPUT_BLOCKS: usize = 8;
const INPUT_BLOCKS: usize = 16;

/// Room for the screens in the map, inside its padding, and the gap
/// between two screens, in pixels.
const MAP_ROOM: (i32, i32) = (336, 72);
const MAP_GAP: i32 = 10;

/// The top of a device card: caption, icon, name, technical name, tag.
struct DeviceHeader {
    icon: gtk::Image,
    name: gtk::Label,
    technical: gtk::Label,
    tag: gtk::Label,
}

impl DeviceHeader {
    fn new(card: &gtk::Box, caption: &str) -> Self {
        card.append(&ui::caption(caption));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let icon = ui::tile(44);
        let texts = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let name = ui::label("…", &["dc-device-name"]);
        let technical = ui::label("", &["dc-mono"]);
        technical.set_wrap(false);
        technical.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texts.append(&name);
        texts.append(&technical);
        let tag = ui::tag("", ui::TagKind::Ok);
        tag.set_visible(false);
        row.append(&icon);
        row.append(&texts);
        row.append(&tag);
        card.append(&row);
        Self {
            icon,
            name,
            technical,
            tag,
        }
    }

    fn show(&self, icon: &str, name: &str, technical: &str) {
        ui::set_tile(&self.icon, icon, ui::TileKind::Accent);
        self.name.set_label(name);
        self.technical.set_label(technical);
        self.technical.set_tooltip_text(Some(technical));
    }
}

/// Where a button of the page sends the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NowLink {
    /// A tab of the Audio page, from a Change button.
    Audio(AudioTab),
    /// The Diagnostic page, from the alert.
    Diagnostic,
}

/// What the links of the page call.
type LinkHandler = Box<dyn Fn(NowLink)>;

/// How long a voice counts as just heard, for the alert.
const VOICE_FRESH: Duration = Duration::from_secs(10);

/// The alert of the design: an app listens to a monitor (DC-23).
struct AlertBanner {
    root: gtk::Box,
    title: gtk::Label,
    text: gtk::Label,
    switch: gtk::Button,
    diagnostic: gtk::Button,
    ignore: gtk::Button,
}

impl AlertBanner {
    fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(14)
            .css_classes(["dc-alert"])
            .visible(false)
            .build();
        let icon = ui::icon("dc-warning-symbolic", 24);
        icon.set_valign(gtk::Align::Start);
        icon.add_css_class("dc-warn-icon");
        root.append(&icon);
        let texts = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();
        let title = ui::label("", &["dc-alert-title"]);
        let text = ui::label("", &["dc-alert-text"]);
        texts.append(&title);
        texts.append(&text);
        let buttons = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_top(6)
            .build();
        let switch = gtk::Button::builder()
            .css_classes(["dc-btn", "dc-warn"])
            .build();
        let diagnostic = ui::button("Run the diagnostic", None, ui::ButtonKind::Normal);
        let ignore = ui::button("Ignore", None, ui::ButtonKind::Flat);
        ignore.set_tooltip_text(Some(
            "Hide this alert until another app listens to a monitor",
        ));
        buttons.append(&switch);
        buttons.append(&diagnostic);
        buttons.append(&ignore);
        texts.append(&buttons);
        root.append(&texts);
        Self {
            root,
            title,
            text,
            switch,
            diagnostic,
            ignore,
        }
    }
}

/// What the page remembers between updates.
#[derive(Default)]
struct Current {
    output: Option<DeviceId>,
    output_monitor: Option<DeviceId>,
    input: Option<DeviceId>,
    output_bar: LevelBar,
    input_bar: LevelBar,
    last_volume_move: Option<Instant>,
    pending_volume: Option<glib::SourceId>,
    /// When the microphone last heard a voice.
    heard_at: Option<Instant>,
    /// The streams whose alert the user ignored.
    ignored: Vec<StreamId>,
    /// What the alert is about now.
    alert: Option<now_model::Alert>,
    /// The last state, to show the alert again when a voice is heard.
    last: Option<(Config, AudioState)>,
}

/// The Now page.
pub struct NowPage {
    /// The page, to put in the window.
    pub root: gtk::ScrolledWindow,
    backend: Backend,
    output: DeviceHeader,
    volume: gtk::Scale,
    output_meter: ui::Meter,
    test_output: gtk::Button,
    output_change: gtk::Button,
    input: DeviceHeader,
    input_meter: ui::Meter,
    input_hint: gtk::Label,
    test_input: gtk::Button,
    input_change: gtk::Button,
    /// Called by the Change buttons and the alert, to show another page.
    on_link: RefCell<Option<LinkHandler>>,
    alert: AlertBanner,
    apps: gtk::Box,
    map: gtk::Box,
    screens_line: gtk::Label,
    screen_css: gtk::CssProvider,
    current: Rc<RefCell<Current>>,
    /// `true` while the page itself moves the slider.
    setting_volume: Rc<Cell<bool>>,
}

impl NowPage {
    /// Builds the page.
    #[allow(
        clippy::too_many_lines,
        reason = "builds the four cards of the design in order, easier to read in one place"
    )]
    pub fn new(backend: Backend) -> Rc<Self> {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(24)
            .margin_end(24)
            .build();
        let (title, _, _) = ui::page_title(
            "Now",
            "Where the sound goes, which microphone hears you, and which screen you are on.",
        );
        content.append(&title);
        let alert = AlertBanner::new();
        content.append(&alert.root);

        // The sound card.
        let output_card = ui::card();
        let output = DeviceHeader::new(&output_card, "You hear through");
        let volume_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        volume_row.append(&ui::row_label("Volume"));
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, MAX_VOLUME, 1.0);
        volume.set_hexpand(true);
        volume.set_draw_value(false);
        volume.add_mark(100.0, gtk::PositionType::Bottom, None);
        volume.set_tooltip_text(Some("Volume of the output"));
        let output_meter = ui::Meter::new(OUTPUT_BLOCKS);
        volume_row.append(&volume);
        volume_row.append(&output_meter.root);
        output_card.append(&volume_row);
        let output_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let test_output = ui::button(
            "Test the sound",
            Some("dc-play-symbolic"),
            ui::ButtonKind::Normal,
        );
        test_output.set_tooltip_text(Some("Plays a short chime"));
        let output_change = ui::button("Change", None, ui::ButtonKind::Flat);
        output_change.set_tooltip_text(Some("Choose another output"));
        output_buttons.append(&test_output);
        output_buttons.append(&output_change);
        output_card.append(&output_buttons);

        // The microphone card.
        let input_card = ui::card();
        let input = DeviceHeader::new(&input_card, "They hear you via");
        let level_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        level_row.append(&ui::row_label("Level"));
        let input_meter = ui::Meter::new(INPUT_BLOCKS);
        let input_hint = ui::label("Speak: the bar moves", &["dc-hint-ok"]);
        input_hint.set_wrap(false);
        level_row.append(&input_meter.root);
        level_row.append(&input_hint);
        input_card.append(&level_row);
        let input_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let test_input = ui::button(
            "Record 3 s and play back",
            Some("dc-record-symbolic"),
            ui::ButtonKind::Normal,
        );
        test_input.set_tooltip_text(Some("Records 3 seconds, then plays them on the output"));
        let input_change = ui::button("Change", None, ui::ButtonKind::Flat);
        input_change.set_tooltip_text(Some("Choose another microphone"));
        input_buttons.append(&test_input);
        input_buttons.append(&input_change);
        input_card.append(&input_buttons);

        // The apps card.
        let apps_card = ui::card();
        apps_card.append(&ui::caption("Apps using the sound"));
        let apps = gtk::Box::new(gtk::Orientation::Vertical, 0);
        apps_card.append(&apps);

        // The screens card.
        let screens_card = ui::card();
        let screens_top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let screens_caption = ui::caption("Your screens");
        screens_caption.set_hexpand(true);
        let identify = ui::button("Identify", None, ui::ButtonKind::Normal);
        identify.add_css_class("dc-compact");
        identify.set_sensitive(false);
        identify.set_tooltip_text(Some("Shows a big number on each screen. Coming soon."));
        screens_top.append(&screens_caption);
        screens_top.append(&identify);
        screens_card.append(&screens_top);
        let map_area = gtk::Box::builder()
            .css_classes(["dc-map"])
            .height_request(MAP_ROOM.1 + 24)
            .build();
        // The screens stand at the bottom, side by side, in the middle.
        let map = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(MAP_GAP)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .hexpand(true)
            .build();
        map_area.append(&map);
        screens_card.append(&map_area);
        let screens_line = ui::label("…", &["dc-small"]);
        screens_card.append(&screens_line);

        let grid = gtk::Grid::builder()
            .column_spacing(16)
            .row_spacing(14)
            .column_homogeneous(true)
            .build();
        grid.attach(&output_card, 0, 0, 1, 1);
        grid.attach(&input_card, 1, 0, 1, 1);
        grid.attach(&apps_card, 0, 1, 1, 1);
        grid.attach(&screens_card, 1, 1, 1, 1);
        content.append(&grid);

        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&content)
            .vexpand(true)
            .build();

        let screen_css = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &screen_css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let page = Rc::new(Self {
            root,
            backend,
            output,
            volume,
            output_meter,
            test_output,
            output_change,
            input,
            input_meter,
            input_hint,
            test_input,
            input_change,
            on_link: RefCell::default(),
            alert,
            apps,
            map,
            screens_line,
            screen_css,
            current: Rc::default(),
            setting_volume: Rc::default(),
        });
        page.connect();
        page.show_sound(&Config::default(), None);
        page
    }

    /// Calls `handler` when a button sends the user to another page.
    pub fn connect_link(&self, handler: impl Fn(NowLink) + 'static) {
        *self.on_link.borrow_mut() = Some(Box::new(handler));
    }

    fn link(&self, to: NowLink) {
        if let Some(handler) = self.on_link.borrow().as_ref() {
            handler(to);
        }
    }

    /// Connects the buttons and the slider to the backend.
    fn connect(self: &Rc<Self>) {
        for (button, tab) in [
            (&self.output_change, AudioTab::Outputs),
            (&self.input_change, AudioTab::Microphones),
        ] {
            let page = Rc::clone(self);
            button.connect_clicked(move |_| page.link(NowLink::Audio(tab)));
        }
        let page = Rc::clone(self);
        self.alert.switch.connect_clicked(move |_| {
            let fixes = page
                .current
                .borrow()
                .alert
                .as_ref()
                .and_then(|alert| alert.switch.as_ref())
                .map(|(_, fixes)| fixes.clone())
                .unwrap_or_default();
            for fix in fixes {
                page.backend.request(Request::Fix(fix));
            }
        });
        let page = Rc::clone(self);
        self.alert
            .diagnostic
            .connect_clicked(move |_| page.link(NowLink::Diagnostic));
        let page = Rc::clone(self);
        self.alert.ignore.connect_clicked(move |_| {
            let mut current = page.current.borrow_mut();
            if let Some(alert) = current.alert.take() {
                current.ignored.extend(alert.streams);
            }
            page.alert.root.set_visible(false);
        });
        let page = Rc::clone(self);
        self.test_output.connect_clicked(move |_| {
            if let Some(output) = page.current.borrow().output.clone() {
                page.backend.request(Request::TestOutput(output));
                ui::toast(&page.root, "Playing a test sound…");
            }
        });
        let page = Rc::clone(self);
        self.test_input.connect_clicked(move |_| {
            let current = page.current.borrow();
            if let (Some(input), Some(output)) = (current.input.clone(), current.output.clone()) {
                page.backend.request(Request::TestInput { input, output });
                ui::toast(&page.root, "Recording 3 seconds: speak now.");
            }
        });
        let page = Rc::clone(self);
        self.volume.connect_value_changed(move |scale| {
            if page.setting_volume.get() {
                return;
            }
            let mut current = page.current.borrow_mut();
            current.last_volume_move = Some(Instant::now());
            if let Some(pending) = current.pending_volume.take() {
                pending.remove();
            }
            let Some(output) = current.output.clone() else {
                return;
            };
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the slider goes from 0 to 150"
            )]
            let percent = scale.value().round() as u16;
            let page_later = Rc::clone(&page);
            current.pending_volume = Some(glib::timeout_add_local_once(VOLUME_DELAY, move || {
                page_later.current.borrow_mut().pending_volume = None;
                page_later.backend.request(Request::SetVolume(
                    Target::Output(output),
                    Volume::from_percent(percent),
                ));
            }));
        });
    }

    /// Shows the sound: the output and microphone cards, and the apps.
    pub fn show_sound(&self, config: &Config, sound: Option<&Result<AudioState, String>>) {
        let state = match sound {
            Some(Ok(state)) => Some(state),
            _ => None,
        };
        let waiting = match sound {
            None => "Reading the sound…",
            Some(Err(_)) => "No sound server",
            Some(Ok(_)) => "",
        };
        self.show_output(config, state, waiting);
        self.show_input(config, state, waiting);
        self.show_apps(config, state);
        self.current.borrow_mut().last = state.map(|state| (config.clone(), state.clone()));
        self.show_alert();
        let mut current = self.current.borrow_mut();
        let sources = state.map(metered_sources).unwrap_or_default();
        current.output_monitor = sources
            .iter()
            .find(|id| Some(*id) != current.input.as_ref())
            .cloned();
    }

    fn show_output(&self, config: &Config, state: Option<&AudioState>, waiting: &str) {
        let output = state.and_then(AudioState::default_output_device);
        self.current.borrow_mut().output = output.map(|o| o.id.clone());
        if let Some(output) = output {
            self.output.show(
                now_model::device_icon(output.hardware, false),
                config.output_label(output),
                output.id.as_str(),
            );
            if output.muted {
                ui::set_tag(&self.output.tag, "Muted", ui::TagKind::Error);
            } else {
                self.output.tag.set_visible(false);
            }
            let hands_off = self
                .current
                .borrow()
                .last_volume_move
                .is_some_and(|moved| moved.elapsed() < HANDS_OFF);
            if !hands_off {
                self.setting_volume.set(true);
                self.volume.set_value(f64::from(output.volume.percent()));
                self.setting_volume.set(false);
            }
        } else {
            let name = if waiting.is_empty() {
                "No output"
            } else {
                waiting
            };
            self.output.show("dc-speaker-symbolic", name, "");
            self.output.tag.set_visible(false);
        }
        self.volume.set_sensitive(output.is_some());
        self.test_output.set_sensitive(output.is_some());
    }

    fn show_input(&self, config: &Config, state: Option<&AudioState>, waiting: &str) {
        let input = state
            .and_then(AudioState::default_input_device)
            .filter(|input| !input.is_monitor());
        self.current.borrow_mut().input = input.map(|i| i.id.clone());
        if let Some(input) = input {
            self.input.show(
                // The design shows a microphone here, whatever the device.
                "dc-microphone-symbolic",
                &config.input_label(input),
                input.id.as_str(),
            );
            if input.muted {
                ui::set_tag(&self.input.tag, "Muted", ui::TagKind::Error);
                self.input_hint.set_label("Muted: nobody hears you");
                self.input_hint.set_css_classes(&["dc-hint-error"]);
            } else {
                ui::set_tag(&self.input.tag, "Not muted", ui::TagKind::Ok);
                self.input_hint.set_label("Speak: the bar moves");
                self.input_hint.set_css_classes(&["dc-hint-ok"]);
            }
        } else {
            let name = if !waiting.is_empty() {
                waiting
            } else if state.is_some_and(|s| s.microphones().next().is_some()) {
                "No microphone chosen"
            } else {
                "No microphone plugged in"
            };
            self.input.show("dc-microphone-symbolic", name, "");
            self.input.tag.set_visible(false);
            self.input_hint.set_label("");
            self.input_meter.set_fraction(0.0);
        }
        let output_known = self.current.borrow().output.is_some();
        self.test_input
            .set_sensitive(input.is_some() && output_known);
    }

    fn show_apps(&self, config: &Config, state: Option<&AudioState>) {
        ui::clear(&self.apps);
        let rows = state.map_or_else(Vec::new, |state| now_model::app_rows(config, state));
        if rows.is_empty() {
            self.apps.append(&ui::label(
                "No app uses the sound right now.",
                &["dc-small"],
            ));
            return;
        }
        let count = rows.len();
        for (index, row) in rows.into_iter().enumerate() {
            let line = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(10)
                .margin_top(8)
                .margin_bottom(8)
                .build();
            let icon = ui::icon(row.icon, 22);
            icon.add_css_class("dc-dim-icon");
            line.append(&icon);
            let texts = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .hexpand(true)
                .build();
            texts.append(&ui::label(&row.name, &["dc-name"]));
            texts.append(&ui::label(&row.detail, &["dc-small"]));
            line.append(&texts);
            let tag = match row.tag {
                AppTag::Ok => ui::tag("OK", ui::TagKind::Ok),
                AppTag::NotAMicrophone => ui::tag("Not a microphone", ui::TagKind::Warn),
            };
            line.append(&tag);
            self.apps.append(&line);
            if index + 1 < count {
                self.apps.append(&ui::line());
            }
        }
    }

    /// Shows the screens: the map and the layout line.
    pub fn show_screens(&self, config: &Config, screens: Option<&Result<DisplayState, String>>) {
        ui::clear(&self.map);
        let state = match screens {
            Some(Ok(state)) => state,
            Some(Err(_)) => {
                self.screens_line
                    .set_label("Screens can be read only in a GNOME session.");
                return;
            }
            None => {
                self.screens_line.set_label("Reading the screens…");
                return;
            }
        };
        let tiles = now_model::screen_map(config, state, MAP_ROOM.0, MAP_ROOM.1, MAP_GAP);
        let mut css = String::new();
        for tile in &tiles {
            let name = format!("screen-tile-{}", tile.number);
            let _ = writeln!(
                css,
                "#{name} {{ border-color: {color}; }} #{name} .dc-screen-number {{ color: {color}; }}",
                color = tile.color
            );
            self.map.append(&screen_tile(tile, &name));
        }
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        self.screen_css.load_from_data(&css);
        self.screens_line.set_label(&now_model::screens_line(state));
    }

    /// Shows the alert, or hides it, from the last state.
    fn show_alert(&self) {
        let mut current = self.current.borrow_mut();
        let voice_heard = current
            .heard_at
            .is_some_and(|at| at.elapsed() < VOICE_FRESH);
        let alert = current.last.as_ref().and_then(|(config, state)| {
            now_model::alert(config, state, &current.ignored, voice_heard)
        });
        // Streams that are gone cannot come back: forget them.
        if let Some((_, state)) = &current.last {
            let alive: Vec<StreamId> = state.recordings.iter().map(|r| r.id).collect();
            current.ignored.retain(|id| alive.contains(id));
        }
        match &alert {
            Some(alert) => {
                self.alert.title.set_label(&alert.title);
                self.alert.text.set_label(&alert.text);
                match &alert.switch {
                    Some((label, _)) => {
                        self.alert.switch.set_label(label);
                        self.alert.switch.set_visible(true);
                    }
                    None => self.alert.switch.set_visible(false),
                }
                self.alert.root.set_visible(true);
            }
            None => self.alert.root.set_visible(false),
        }
        current.alert = alert;
    }

    /// Moves a level meter.
    pub fn peak(&self, source: &DeviceId, peak: f32) {
        let mut current = self.current.borrow_mut();
        if current.input.as_ref() == Some(source) {
            let fraction = current.input_bar.update(peak);
            self.input_meter.set_fraction(fraction);
            if to_db(peak) > VOICE_DB {
                let fresh = current
                    .heard_at
                    .is_some_and(|at| at.elapsed() < VOICE_FRESH);
                current.heard_at = Some(Instant::now());
                // The alert says the microphone hears the voice.
                if !fresh && current.alert.is_some() {
                    drop(current);
                    self.show_alert();
                }
            }
        } else if current.output_monitor.as_ref() == Some(source) {
            let fraction = current.output_bar.update(peak);
            self.output_meter.set_fraction(fraction);
        }
    }
}

/// A screen of the map: its number, big and in its color, and its name.
fn screen_tile(tile: &now_model::ScreenTile, widget_name: &str) -> gtk::Box {
    let classes: &[&str] = if tile.primary {
        &["dc-screen-tile", "primary"]
    } else {
        &["dc-screen-tile"]
    };
    let screen = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .valign(gtk::Align::End)
        .css_classes(classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>())
        // Set, so the labels inside do not make the whole page grow.
        .vexpand(false)
        .width_request(tile.width)
        .height_request(tile.height)
        .tooltip_text(&tile.name)
        .build();
    screen.set_widget_name(widget_name);
    let middle = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .build();
    middle.append(
        &gtk::Label::builder()
            .label(tile.number.to_string())
            .css_classes(["dc-screen-number"])
            .build(),
    );
    middle.append(
        &gtk::Label::builder()
            .label(&tile.name)
            .css_classes(["dc-screen-name"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(1)
            .hexpand(true)
            .margin_start(4)
            .margin_end(4)
            .build(),
    );
    screen.append(&middle);
    screen
}
