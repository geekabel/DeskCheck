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
use deskcheck_core::audio::level::LevelBar;
use deskcheck_core::audio::{AudioState, DeviceId, Target, Volume};
use deskcheck_core::display::DisplayState;
use gtk::glib;

use crate::backend::{Backend, Request, metered_sources};
use crate::now_model::{self, AppTag};

/// Highest volume of the slider, as GNOME with over-amplification.
const MAX_VOLUME: f64 = 150.0;

/// Wait after the last move of the slider before sending the volume.
const VOLUME_DELAY: Duration = Duration::from_millis(120);

/// The slider is not moved by new states for this long after the user
/// moved it, so it does not jump back while dragging.
const HANDS_OFF: Duration = Duration::from_millis(800);

/// Blocks of the level meters.
const OUTPUT_BLOCKS: f64 = 8.0;
const INPUT_BLOCKS: f64 = 16.0;

/// Size of the screen map, in pixels.
const MAP_SIZE: (i32, i32) = (360, 96);

/// A label, with its style classes.
fn label(text: &str, classes: &[&str]) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .css_classes(classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>())
        .build()
}

/// A card: a box with the `card` style of libadwaita, and inner padding.
fn card() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .css_classes(["card", "now-card"])
        .hexpand(true)
        .build()
}

/// A discrete level meter.
fn meter(blocks: f64) -> gtk::LevelBar {
    let bar = gtk::LevelBar::builder()
        .mode(gtk::LevelBarMode::Discrete)
        .min_value(0.0)
        .max_value(blocks)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    // Added, not set: setting `css-classes` would also remove the classes
    // GTK gives the bar itself (`horizontal`, `discrete`), and its blocks
    // would lose their style.
    bar.add_css_class("now-meter");
    // One color for the whole bar, instead of the low and high colors.
    for offset in ["low", "high", "full"] {
        bar.remove_offset_value(Some(offset));
    }
    bar
}

/// Shows a small tag: `ok`, `warn` or `error`.
fn set_tag(tag: &gtk::Label, text: &str, kind: &str) {
    tag.set_label(text);
    tag.set_css_classes(&["tag", &format!("tag-{kind}")]);
    tag.set_visible(true);
}

/// The top of a device card: caption, icon, name, technical name, tag.
struct DeviceHeader {
    icon: gtk::Image,
    name: gtk::Label,
    technical: gtk::Label,
    tag: gtk::Label,
}

impl DeviceHeader {
    fn new(card: &gtk::Box, caption: &str) -> Self {
        card.append(&label(caption, &["caption-heading", "dim-label"]));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        // The image is the tile: 44 × 44, the icon drawn in its middle.
        let icon = gtk::Image::builder()
            .pixel_size(24)
            .css_classes(["icon-tile"])
            .width_request(44)
            .height_request(44)
            .valign(gtk::Align::Center)
            .build();
        let texts = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let name = label("…", &["title-3"]);
        let technical = label("", &["monospace", "dim-label", "caption"]);
        technical.set_wrap(false);
        technical.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        texts.append(&name);
        texts.append(&technical);
        let tag = gtk::Label::builder()
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
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
        self.icon.set_icon_name(Some(icon));
        self.name.set_label(name);
        self.technical.set_label(technical);
        self.technical.set_tooltip_text(Some(technical));
    }
}

/// A "Change" button with a list of devices to choose from.
struct Chooser {
    button: gtk::MenuButton,
    list: gtk::Box,
    popover: gtk::Popover,
}

impl Chooser {
    fn new(tooltip: &str) -> Self {
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .build();
        let popover = gtk::Popover::builder().child(&list).build();
        let button = gtk::MenuButton::builder()
            .label("Change")
            .popover(&popover)
            .tooltip_text(tooltip)
            .css_classes(["flat"])
            .build();
        Self {
            button,
            list,
            popover,
        }
    }

    /// Fills the list: one button per device, the current one checked.
    fn fill(&self, backend: &Backend, choices: Vec<(Target, String, bool)>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.button.set_sensitive(choices.len() > 1);
        for (target, name, current) in choices {
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_opacity(if current { 1.0 } else { 0.0 });
            line.append(&check);
            line.append(&gtk::Label::new(Some(&name)));
            let choice = gtk::Button::builder()
                .child(&line)
                .css_classes(["flat"])
                .build();
            let backend = backend.clone();
            let popover = self.popover.clone();
            choice.connect_clicked(move |_| {
                popover.popdown();
                backend.request(Request::SetDefault(target.clone()));
            });
            self.list.append(&choice);
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
}

/// The Now page.
pub struct NowPage {
    /// The page, to put in the window.
    pub root: gtk::ScrolledWindow,
    backend: Backend,
    output: DeviceHeader,
    volume: gtk::Scale,
    output_meter: gtk::LevelBar,
    test_output: gtk::Button,
    output_chooser: Chooser,
    input: DeviceHeader,
    input_meter: gtk::LevelBar,
    input_hint: gtk::Label,
    test_input: gtk::Button,
    input_chooser: Chooser,
    apps: gtk::Box,
    map: gtk::Fixed,
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
            .spacing(16)
            .margin_top(18)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        content.append(&label("Now", &["title-1"]));
        content.append(&label(
            "Where the sound goes, which microphone hears you, and which screen you are on.",
            &["dim-label"],
        ));

        // The sound card.
        let output_card = card();
        let output = DeviceHeader::new(&output_card, "YOU HEAR THROUGH");
        let volume_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        volume_row.append(&label("Volume", &["dim-label"]));
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, MAX_VOLUME, 1.0);
        volume.set_hexpand(true);
        volume.set_draw_value(false);
        volume.add_mark(100.0, gtk::PositionType::Bottom, None);
        volume.set_tooltip_text(Some("Volume of the output"));
        let output_meter = meter(OUTPUT_BLOCKS);
        output_meter.set_width_request(80);
        output_meter.set_hexpand(false);
        volume_row.append(&volume);
        volume_row.append(&output_meter);
        output_card.append(&volume_row);
        let output_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let test_output = gtk::Button::builder()
            .label("Test the sound")
            .tooltip_text("Plays a short chime")
            .build();
        let output_chooser = Chooser::new("Choose another output");
        output_buttons.append(&test_output);
        output_buttons.append(&output_chooser.button);
        output_card.append(&output_buttons);

        // The microphone card.
        let input_card = card();
        let input = DeviceHeader::new(&input_card, "THEY HEAR YOU VIA");
        let level_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        level_row.append(&label("Level", &["dim-label"]));
        let input_meter = meter(INPUT_BLOCKS);
        let input_hint = label("Speak: the bar moves", &["success", "caption"]);
        input_hint.set_wrap(false);
        level_row.append(&input_meter);
        level_row.append(&input_hint);
        input_card.append(&level_row);
        let input_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let test_input = gtk::Button::builder()
            .label("Record 3 s and play back")
            .tooltip_text("Records 3 seconds, then plays them on the output")
            .build();
        let input_chooser = Chooser::new("Choose another microphone");
        input_buttons.append(&test_input);
        input_buttons.append(&input_chooser.button);
        input_card.append(&input_buttons);

        // The apps card.
        let apps_card = card();
        apps_card.append(&label(
            "APPS USING THE SOUND",
            &["caption-heading", "dim-label"],
        ));
        let apps = gtk::Box::new(gtk::Orientation::Vertical, 0);
        apps_card.append(&apps);

        // The screens card.
        let screens_card = card();
        let screens_top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let screens_caption = label("YOUR SCREENS", &["caption-heading", "dim-label"]);
        screens_caption.set_hexpand(true);
        let identify = gtk::Button::builder()
            .label("Identify")
            .sensitive(false)
            .tooltip_text("Shows a big number on each screen. Coming soon.")
            .build();
        screens_top.append(&screens_caption);
        screens_top.append(&identify);
        screens_card.append(&screens_top);
        let map_area = gtk::Box::builder()
            .css_classes(["screen-map"])
            .halign(gtk::Align::Fill)
            .build();
        let map = gtk::Fixed::builder()
            .width_request(MAP_SIZE.0)
            .height_request(MAP_SIZE.1)
            .halign(gtk::Align::Center)
            .hexpand(true)
            .build();
        map_area.append(&map);
        screens_card.append(&map_area);
        let screens_line = label("…", &["dim-label"]);
        screens_card.append(&screens_line);

        let grid = gtk::Grid::builder()
            .column_spacing(16)
            .row_spacing(16)
            .column_homogeneous(true)
            .build();
        grid.attach(&output_card, 0, 0, 1, 1);
        grid.attach(&input_card, 1, 0, 1, 1);
        grid.attach(&apps_card, 0, 1, 1, 1);
        grid.attach(&screens_card, 1, 1, 1, 1);
        content.append(&grid);

        let clamp = adw::Clamp::builder()
            .maximum_size(1100)
            .child(&content)
            .build();
        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&clamp)
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
            output_chooser,
            input,
            input_meter,
            input_hint,
            test_input,
            input_chooser,
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

    /// Connects the buttons and the slider to the backend.
    fn connect(self: &Rc<Self>) {
        let page = Rc::clone(self);
        self.test_output.connect_clicked(move |_| {
            if let Some(output) = page.current.borrow().output.clone() {
                page.backend.request(Request::TestOutput(output));
                page.toast_soon("Playing a test sound…");
            }
        });
        let page = Rc::clone(self);
        self.test_input.connect_clicked(move |_| {
            let current = page.current.borrow();
            if let (Some(input), Some(output)) = (current.input.clone(), current.output.clone()) {
                page.backend.request(Request::TestInput { input, output });
                page.toast_soon("Recording 3 seconds: speak now.");
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

    /// Shows a short message from the page, through the window.
    fn toast_soon(&self, text: &str) {
        if let Some(overlay) = self
            .root
            .ancestor(adw::ToastOverlay::static_type())
            .and_downcast::<adw::ToastOverlay>()
        {
            let toast = adw::Toast::new(text);
            toast.set_timeout(3);
            overlay.add_toast(toast);
        }
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
                &now_model::device_icon(output.hardware),
                config.output_label(output),
                output.id.as_str(),
            );
            if output.muted {
                set_tag(&self.output.tag, "Muted", "error");
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
            self.output.show("audio-speakers-symbolic", name, "");
            self.output.tag.set_visible(false);
        }
        self.volume.set_sensitive(output.is_some());
        self.test_output.set_sensitive(output.is_some());
        let choices = state.map_or_else(Vec::new, |state| {
            state
                .outputs
                .iter()
                .map(|o| {
                    (
                        Target::Output(o.id.clone()),
                        config.output_label(o).to_owned(),
                        state.default_output.as_ref() == Some(&o.id),
                    )
                })
                .collect()
        });
        self.output_chooser.fill(&self.backend, choices);
    }

    fn show_input(&self, config: &Config, state: Option<&AudioState>, waiting: &str) {
        let input = state
            .and_then(AudioState::default_input_device)
            .filter(|input| !input.is_monitor());
        self.current.borrow_mut().input = input.map(|i| i.id.clone());
        if let Some(input) = input {
            self.input.show(
                &now_model::device_icon(input.hardware),
                &config.input_label(input),
                input.id.as_str(),
            );
            if input.muted {
                set_tag(&self.input.tag, "Muted", "error");
                self.input_hint.set_label("Muted: nobody hears you");
                self.input_hint.set_css_classes(&["error", "caption"]);
            } else {
                set_tag(&self.input.tag, "Not muted", "ok");
                self.input_hint.set_label("Speak: the bar moves");
                self.input_hint.set_css_classes(&["success", "caption"]);
            }
        } else {
            let name = if !waiting.is_empty() {
                waiting
            } else if state.is_some_and(|s| s.microphones().next().is_some()) {
                "No microphone chosen"
            } else {
                "No microphone plugged in"
            };
            self.input.show("audio-input-microphone-symbolic", name, "");
            self.input.tag.set_visible(false);
            self.input_hint.set_label("");
            self.input_meter.set_value(0.0);
        }
        let output_known = self.current.borrow().output.is_some();
        self.test_input
            .set_sensitive(input.is_some() && output_known);
        let choices = state.map_or_else(Vec::new, |state| {
            state
                .microphones()
                .map(|i| {
                    (
                        Target::Input(i.id.clone()),
                        config.input_label(i).into_owned(),
                        state.default_input.as_ref() == Some(&i.id),
                    )
                })
                .collect()
        });
        self.input_chooser.fill(&self.backend, choices);
    }

    fn show_apps(&self, config: &Config, state: Option<&AudioState>) {
        while let Some(child) = self.apps.first_child() {
            self.apps.remove(&child);
        }
        let rows = state.map_or_else(Vec::new, |state| now_model::app_rows(config, state));
        if rows.is_empty() {
            self.apps
                .append(&label("No app uses the sound right now.", &["dim-label"]));
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
            line.append(&gtk::Image::from_icon_name("audio-x-generic-symbolic"));
            let texts = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .hexpand(true)
                .build();
            texts.append(&label(&row.name, &["heading"]));
            texts.append(&label(&row.detail, &["dim-label", "caption"]));
            line.append(&texts);
            let tag = gtk::Label::builder().valign(gtk::Align::Center).build();
            match row.tag {
                AppTag::Ok => set_tag(&tag, "OK", "ok"),
                AppTag::NotAMicrophone => set_tag(&tag, "Not a microphone", "warn"),
            }
            line.append(&tag);
            self.apps.append(&line);
            if index + 1 < count {
                self.apps
                    .append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
        }
    }

    /// Shows the screens: the map and the layout line.
    pub fn show_screens(&self, config: &Config, screens: Option<&Result<DisplayState, String>>) {
        while let Some(child) = self.map.first_child() {
            self.map.remove(&child);
        }
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
        let tiles = now_model::screen_map(config, state, MAP_SIZE.0, MAP_SIZE.1);
        let mut css = String::new();
        for tile in &tiles {
            let name = format!("screen-tile-{}", tile.number);
            let _ = writeln!(
                css,
                "#{name} {{ border-color: {color}; }} #{name} .screen-number {{ color: {color}; }}",
                color = tile.color
            );
            let box_ = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .valign(gtk::Align::Center)
                .css_classes(["screen-tile"])
                // Set, so the labels that center themselves inside do not
                // make the whole page grow.
                .vexpand(false)
                .width_request(tile.width)
                .height_request(tile.height)
                .tooltip_text(&tile.name)
                .build();
            box_.set_widget_name(&name);
            let number = gtk::Label::builder()
                .label(tile.number.to_string())
                .css_classes(["screen-number"])
                .vexpand(true)
                .valign(gtk::Align::End)
                .build();
            let title = gtk::Label::builder()
                .label(&tile.name)
                .css_classes(["caption"])
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(1)
                .hexpand(true)
                .vexpand(true)
                .valign(gtk::Align::Start)
                .margin_start(4)
                .margin_end(4)
                .build();
            box_.append(&number);
            box_.append(&title);
            self.map.put(&box_, f64::from(tile.x), f64::from(tile.y));
        }
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        self.screen_css.load_from_data(&css);
        self.screens_line.set_label(&now_model::screens_line(state));
    }

    /// Moves a level meter.
    pub fn peak(&self, source: &DeviceId, peak: f32) {
        let mut current = self.current.borrow_mut();
        if current.input.as_ref() == Some(source) {
            let fraction = current.input_bar.update(peak);
            self.input_meter
                .set_value(f64::from(fraction) * INPUT_BLOCKS);
        } else if current.output_monitor.as_ref() == Some(source) {
            let fraction = current.output_bar.update(peak);
            self.output_meter
                .set_value(f64::from(fraction) * OUTPUT_BLOCKS);
        }
    }
}
