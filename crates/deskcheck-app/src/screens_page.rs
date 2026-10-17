//! The Screens page: a map of the screens at their real place, each with
//! its number and color, and the settings of the screen chosen in the map,
//! as in the design.
//!
//! The name and the color are saved at once in the settings file. The
//! layout (tabs of the header bar), the primary screen, the resolution,
//! the scale and the place (drag a screen in the map) go to GNOME through
//! the backend (DC-25). GNOME then asks to keep the change, and goes back
//! after 20 seconds without an answer.

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;

use adw::prelude::*;
use deskcheck_config::{Color, Config, Store};
use deskcheck_core::audio::{AudioState, Target};
use deskcheck_core::display::{DisplayState, Screen, ScreenId};

use crate::backend::{Backend, Mouse, Request};
use crate::now_model::screen_color;
use deskcheck_mutter::setup::{Arrangement, Change, Setup};

use crate::screens_model::{self, Layout, MapGeometry, MapTile, SWATCHES, TileSize};
use crate::ui::{self, ButtonKind, TagKind};

/// Room for the screens in the map, in pixels.
const MAP_ROOM: (i32, i32) = (800, 200);

/// Pixels between two screens of the map.
const MAP_GAP: i32 = 20;

/// The line under the map.
const HINT: &str = "Drag a screen to place it · Double-click to identify it alone";

/// A drag shorter than this, in pixels, is a click.
const DRAG_THRESHOLD: f64 = 6.0;

/// Where the list of makers of the system is (package `hwdata`).
const PNP_IDS: &str = "/usr/share/hwdata/pnp.ids";

/// A line of a settings card: a label of fixed width, then a control.
fn field(card: &gtk::Box, label: &str, control: &impl IsA<gtk::Widget>, first: bool) {
    if !first {
        card.append(&ui::line());
    }
    let line = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["dc-field"])
        .build();
    let name = ui::label(label, &["dc-field-label"]);
    name.set_wrap(false);
    name.set_width_request(120);
    line.append(&name);
    line.append(control);
    card.append(&line);
}

/// A list to choose a value, with its texts.
fn choice_list() -> (gtk::DropDown, gtk::StringList) {
    let values = gtk::StringList::new(&[]);
    let list = gtk::DropDown::builder()
        .model(&values)
        .hexpand(true)
        .css_classes(["dc-input"])
        .build();
    (list, values)
}

/// Fills a list made by [`choice_list`] and selects the value in use.
fn fill_list<T>(
    list: &gtk::DropDown,
    values: &gtk::StringList,
    choices: &[screens_model::Choice<T>],
) {
    let labels: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
    values.splice(0, values.n_items(), &labels);
    let at = choices.iter().position(|c| c.current).unwrap_or(0);
    list.set_selected(u32::try_from(at).unwrap_or(0));
    list.set_sensitive(choices.len() > 1);
}

/// The arrangement GNOME applies for a tab of the header bar.
fn arrangement(layout: Layout) -> Arrangement {
    match layout {
        Layout::Extended => Arrangement::Extend,
        Layout::Mirrored => Arrangement::Mirror,
        Layout::Single => Arrangement::Single,
    }
}

/// The value of a line of "How DeskCheck recognizes it".
fn fact(grid: &gtk::Grid, row: i32, label: &str) -> gtk::Label {
    let name = ui::label(label, &["dc-field-label"]);
    name.set_wrap(false);
    name.set_width_request(120);
    let value = ui::label("", &["dc-value"]);
    value.set_selectable(true);
    grid.attach(&name, 0, row, 1, 1);
    grid.attach(&value, 1, row, 1, 1);
    value
}

/// The widgets of the settings, kept to show another screen in place:
/// building them again would lose what is being typed.
struct Settings {
    badge: gtk::Label,
    name: gtk::Entry,
    swatches: Vec<(Color, gtk::Button)>,
    primary: gtk::CheckButton,
    resolution: gtk::DropDown,
    resolutions: gtk::StringList,
    scale: gtk::DropDown,
    scales: gtk::StringList,
    speaker_name: gtk::Label,
    speaker_system: gtk::Label,
    speaker_use: gtk::Button,
    speaker_used: gtk::Label,
    speaker_row: gtk::Box,
    no_speaker: gtk::Label,
    maker: gtk::Label,
    model: gtk::Label,
    serial: gtk::Label,
    connector: gtk::Label,
}

/// What the page remembers between updates.
#[derive(Default)]
struct Current {
    /// The screen chosen in the map.
    chosen: Option<ScreenId>,
    /// The screen shown in the settings, with its system name.
    shown: Option<(ScreenId, String)>,
    /// The output of the screen, to use it.
    speakers: Option<deskcheck_core::audio::DeviceId>,
    config: Config,
    screens: Option<DisplayState>,
    sound: Option<AudioState>,
    setup: Option<Setup>,
    /// The connector of the shown screen, and the values of its lists.
    connector: Option<String>,
    resolution_values: Vec<String>,
    scale_values: Vec<f64>,
    /// How the map is drawn, to turn a drop into a place on the desktop.
    geometry: Option<MapGeometry>,
}

/// The Screens page.
pub struct ScreensPage {
    /// The page, to put in the window.
    pub root: gtk::ScrolledWindow,
    /// The layout tabs, to put in the header bar.
    pub tabs: gtk::Box,
    layout_buttons: Vec<(Layout, gtk::ToggleButton)>,
    map: gtk::Fixed,
    hint: gtk::Label,
    details: gtk::Box,
    settings: Settings,
    css: gtk::CssProvider,
    chosen_css: gtk::CssProvider,
    tiles: RefCell<Vec<(ScreenId, gtk::Box)>>,
    /// The "Your mouse is here" tag of each screen of the map (DC-26).
    mouse_tags: RefCell<Vec<(ScreenId, gtk::Box)>>,
    /// Where the mouse is.
    mouse: RefCell<Option<Mouse>>,
    backend: Backend,
    pnp_ids: Option<String>,
    current: RefCell<Current>,
    /// `true` while the page itself sets its controls.
    updating: Cell<bool>,
}

impl ScreensPage {
    /// Builds the page.
    #[allow(
        clippy::too_many_lines,
        reason = "builds the map and the two cards of the design in order"
    )]
    pub fn new(backend: Backend) -> Rc<Self> {
        let tabs = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(2)
            .css_classes(["dc-seg"])
            .valign(gtk::Align::Center)
            .build();
        let mut layout_buttons: Vec<(Layout, gtk::ToggleButton)> = Vec::new();
        for layout in Layout::ALL {
            let button = gtk::ToggleButton::builder().label(layout.tab()).build();
            if let Some((_, first)) = layout_buttons.first() {
                button.set_group(Some(first));
            }
            tabs.append(&button);
            layout_buttons.push((layout, button));
        }

        let page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(24)
            .margin_end(24)
            .build();
        let top = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .build();
        let (title, _, _) = ui::page_title(
            "Screens",
            "Your screens as they are on your desk. Click a screen to set it up.",
        );
        top.append(&title);
        let identify = ui::button(
            "Identify screens",
            Some("dc-identify-symbolic"),
            ButtonKind::Primary,
        );
        identify.set_valign(gtk::Align::End);
        identify.set_tooltip_text(Some("Shows its number on each screen"));
        identify.set_action_name(Some("app.identify"));
        identify.set_action_target_value(Some(&0_i32.to_variant()));
        let lost = ui::button("Bring back lost windows", None, ButtonKind::Normal);
        lost.set_valign(gtk::Align::End);
        lost.set_sensitive(false);
        lost.set_tooltip_text(Some("Comes in a later version"));
        top.append(&identify);
        top.append(&lost);
        page.append(&top);

        // The map, on a dotted desk.
        let desk = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["dc-card", "dc-dots"])
            .height_request(262)
            // Kept at the height of the design, though the hint at its
            // bottom expands.
            .vexpand(false)
            .build();
        let map = gtk::Fixed::builder()
            .width_request(MAP_ROOM.0)
            .height_request(MAP_ROOM.1)
            .halign(gtk::Align::Center)
            .margin_top(18)
            .build();
        desk.append(&map);
        let hint = ui::label(HINT, &["dc-small"]);
        hint.set_margin_start(16);
        hint.set_margin_top(12);
        hint.set_margin_bottom(12);
        hint.set_valign(gtk::Align::End);
        hint.set_vexpand(true);
        desk.append(&hint);
        page.append(&desk);

        // The settings of the chosen screen.
        let left = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["dc-card", "dc-form"])
            .hexpand(true)
            .build();
        let heading = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["dc-form-heading"])
            .build();
        let badge = gtk::Label::builder()
            .css_classes(["dc-number-badge"])
            .build();
        badge.set_widget_name("screen-badge");
        heading.append(&badge);
        heading.append(&ui::label("Settings of this screen", &["dc-section-title"]));
        left.append(&heading);
        let name = gtk::Entry::builder()
            .hexpand(true)
            .css_classes(["dc-input"])
            .placeholder_text("Name of this screen")
            .build();
        field(&left, "Name", &name, true);
        let swatch_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let swatches: Vec<(Color, gtk::Button)> = SWATCHES
            .iter()
            .enumerate()
            .map(|(index, (color, label))| {
                let swatch = gtk::Button::builder()
                    .css_classes(["dc-swatch"])
                    .tooltip_text(*label)
                    .valign(gtk::Align::Center)
                    .build();
                swatch.set_widget_name(&format!("swatch-{index}"));
                swatch_row.append(&swatch);
                (*color, swatch)
            })
            .collect();
        field(&left, "Marker color", &swatch_row, false);
        let primary = gtk::CheckButton::builder()
            .tooltip_text("The primary screen has the top bar")
            .build();
        field(&left, "Primary screen", &primary, false);
        let (resolution, resolutions) = choice_list();
        field(&left, "Resolution", &resolution, false);
        let (scale, scales) = choice_list();
        field(&left, "Scale", &scale, false);

        let right = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["dc-card", "dc-form"])
            .hexpand(true)
            .build();
        let sound_title = ui::label("Sound of this screen", &["dc-section-title"]);
        sound_title.add_css_class("dc-form-heading");
        right.append(&sound_title);
        let speaker_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .margin_bottom(10)
            .build();
        let speaker_icon = ui::icon("dc-speaker-symbolic", 18);
        speaker_icon.add_css_class("dc-warn-icon");
        speaker_row.append(&speaker_icon);
        let speaker_texts = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        let speaker_name = ui::label("", &["dc-name"]);
        let speaker_system = ui::label("", &["dc-mono"]);
        speaker_system.set_wrap(false);
        speaker_system.set_ellipsize(gtk::pango::EllipsizeMode::End);
        speaker_texts.append(&speaker_name);
        speaker_texts.append(&speaker_system);
        speaker_row.append(&speaker_texts);
        let speaker_use = ui::button("Use", None, ButtonKind::Normal);
        speaker_use.add_css_class("dc-compact");
        speaker_use.set_tooltip_text(Some("Play the sound on the speakers of this screen"));
        let speaker_used = ui::tag("In use", TagKind::Ok);
        speaker_row.append(&speaker_use);
        speaker_row.append(&speaker_used);
        right.append(&speaker_row);
        let no_speaker = ui::label("", &["dc-small"]);
        no_speaker.set_margin_bottom(10);
        right.append(&no_speaker);
        right.append(&ui::line());
        let known_title = ui::label("How DeskCheck recognizes it", &["dc-section-title"]);
        known_title.add_css_class("dc-form-heading");
        right.append(&known_title);
        let facts = gtk::Grid::builder()
            .row_spacing(6)
            .column_spacing(12)
            .margin_bottom(12)
            .build();
        let maker = fact(&facts, 0, "Maker");
        let model = fact(&facts, 1, "Model");
        let serial = fact(&facts, 2, "Serial number");
        serial.add_css_class("dc-mono-value");
        let connector = fact(&facts, 3, "Connected to");
        right.append(&facts);

        let details = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(14)
            .homogeneous(true)
            .build();
        details.append(&left);
        details.append(&right);
        page.append(&details);

        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&page)
            .vexpand(true)
            .build();

        let css = gtk::CssProvider::new();
        let chosen_css = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            for provider in [&css, &chosen_css] {
                gtk::style_context_add_provider_for_display(
                    &display,
                    provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        }

        let page = Rc::new(Self {
            root,
            tabs,
            layout_buttons,
            map,
            hint,
            details,
            settings: Settings {
                badge,
                name,
                swatches,
                primary,
                resolution,
                resolutions,
                scale,
                scales,
                speaker_name,
                speaker_system,
                speaker_use,
                speaker_used,
                speaker_row,
                no_speaker,
                maker,
                model,
                serial,
                connector,
            },
            css,
            chosen_css,
            tiles: RefCell::default(),
            mouse_tags: RefCell::default(),
            mouse: RefCell::default(),
            backend,
            pnp_ids: std::fs::read_to_string(PNP_IDS).ok(),
            current: RefCell::default(),
            updating: Cell::new(false),
        });
        page.connect();
        page
    }

    /// Connects the controls of the settings.
    fn connect(self: &Rc<Self>) {
        let page = Rc::clone(self);
        self.settings
            .name
            .connect_activate(move |_| page.save_name());
        let focus = gtk::EventControllerFocus::new();
        let page = Rc::clone(self);
        focus.connect_leave(move |_| page.save_name());
        self.settings.name.add_controller(focus);
        for (color, swatch) in &self.settings.swatches {
            let page = Rc::clone(self);
            let color = *color;
            swatch.connect_clicked(move |_| page.save_color(color));
        }
        for (layout, button) in &self.layout_buttons {
            let page = Rc::clone(self);
            let layout = *layout;
            button.connect_toggled(move |button| {
                if button.is_active() && !page.updating.get() {
                    page.ask(Change::Arrange(arrangement(layout)));
                }
            });
        }
        let page = Rc::clone(self);
        self.settings.primary.connect_toggled(move |check| {
            if page.updating.get() || !check.is_active() {
                return;
            }
            if let Some(connector) = page.current.borrow().connector.clone() {
                page.ask(Change::Primary(connector));
            }
        });
        let page = Rc::clone(self);
        self.settings
            .resolution
            .connect_selected_notify(move |list| {
                if page.updating.get() {
                    return;
                }
                let change = {
                    let current = page.current.borrow();
                    let mode = usize::try_from(list.selected())
                        .ok()
                        .and_then(|at| current.resolution_values.get(at).cloned());
                    current
                        .connector
                        .clone()
                        .zip(mode)
                        .map(|(connector, mode)| Change::Mode { connector, mode })
                };
                if let Some(change) = change {
                    page.ask(change);
                }
            });
        let page = Rc::clone(self);
        self.settings.scale.connect_selected_notify(move |list| {
            if page.updating.get() {
                return;
            }
            let change = {
                let current = page.current.borrow();
                let scale = usize::try_from(list.selected())
                    .ok()
                    .and_then(|at| current.scale_values.get(at).copied());
                current
                    .connector
                    .clone()
                    .zip(scale)
                    .map(|(connector, scale)| Change::Scale { connector, scale })
            };
            if let Some(change) = change {
                page.ask(change);
            }
        });
        let page = Rc::clone(self);
        self.settings.speaker_use.connect_clicked(move |_| {
            if let Some(output) = page.current.borrow().speakers.clone() {
                page.backend
                    .request(Request::SetDefault(Target::Output(output)));
            }
        });
    }

    /// Shows which screen the mouse is on.
    pub fn show_mouse(&self, mouse: &Mouse) {
        *self.mouse.borrow_mut() = Some(mouse.clone());
        self.show_mouse_tags();
        if self.details.is_visible() {
            self.hint.set_label(&self.hint_text());
        }
    }

    fn show_mouse_tags(&self) {
        let mouse = self.mouse.borrow();
        for (id, tag) in self.mouse_tags.borrow().iter() {
            tag.set_visible(matches!(mouse.as_ref(), Some(Mouse::On(on)) if on == id));
        }
    }

    /// The line under the map, with a word on the mouse when it is not known.
    fn hint_text(&self) -> String {
        match self.mouse.borrow().as_ref() {
            Some(Mouse::NeedsExtension) => {
                format!("{HINT} · To see your mouse on Wayland, turn on the DeskCheck extension")
            }
            _ => HINT.to_owned(),
        }
    }

    /// Asks GNOME for a change of the screens.
    fn ask(&self, change: Change) {
        self.backend.request(Request::Screens(change));
    }

    /// Shows the last state again: after a drag, the screens go back to
    /// their place until GNOME says where they are.
    fn show_again(self: &Rc<Self>) {
        let (config, screens, sound, setup) = {
            let current = self.current.borrow();
            (
                current.config.clone(),
                current.screens.clone(),
                current.sound.clone(),
                current.setup.clone(),
            )
        };
        self.show(
            &config,
            screens.map(Ok).as_ref(),
            sound.map(Ok).as_ref(),
            setup.as_ref(),
        );
    }

    /// Saves the name typed, when it changed. A blank name gives the
    /// screen its system name again.
    fn save_name(&self) {
        let Some((id, label)) = self.current.borrow().shown.clone() else {
            return;
        };
        let text = self.settings.name.text().trim().to_owned();
        let unchanged = {
            let current = self.current.borrow();
            current
                .screens
                .as_ref()
                .and_then(|state| state.screen(&id))
                .is_some_and(|screen| current.config.screen_label(screen) == text)
        };
        if unchanged {
            return;
        }
        let name = (!text.is_empty() && text != label).then_some(text);
        self.save(|config| config.set_screen_name(&id, name));
    }

    /// Saves the color chosen for the shown screen.
    fn save_color(&self, color: Color) {
        let Some((id, _)) = self.current.borrow().shown.clone() else {
            return;
        };
        self.save(|config| config.set_screen_color(&id, Some(color)));
    }

    /// Changes the settings file, then asks to show everything again.
    fn save(&self, change: impl FnOnce(&mut Config)) {
        let saved = Store::new().and_then(|store| {
            let mut config = store.load()?;
            change(&mut config);
            store.save(&config)
        });
        match saved {
            Ok(()) => self.backend.request(Request::Refresh),
            Err(error) => ui::toast(
                &self.root,
                &format!("The setting could not be saved: {error}"),
            ),
        }
    }

    /// Shows the screens, and the sound of the chosen one.
    pub fn show(
        self: &Rc<Self>,
        config: &Config,
        screens: Option<&Result<DisplayState, String>>,
        sound: Option<&Result<AudioState, String>>,
        setup: Option<&Setup>,
    ) {
        {
            let mut current = self.current.borrow_mut();
            current.config = config.clone();
            current.screens = screens.and_then(|s| s.as_ref().ok()).cloned();
            current.sound = sound.and_then(|s| s.as_ref().ok()).cloned();
            current.setup = setup.cloned();
        }
        while let Some(child) = self.map.first_child() {
            self.map.remove(&child);
        }
        self.mouse_tags.borrow_mut().clear();
        let state = match screens {
            Some(Ok(state)) => state,
            Some(Err(_)) => {
                self.empty("Screens can be read only in a GNOME session.");
                return;
            }
            None => {
                self.empty("Reading the screens…");
                return;
            }
        };
        let current_layout = screens_model::layout(state);
        self.updating.set(true);
        for (layout, button) in &self.layout_buttons {
            if Some(*layout) == current_layout {
                button.set_active(true);
            }
        }
        self.updating.set(false);
        // Only GNOME can change the screens, and only when it said how.
        self.tabs.set_sensitive(setup.is_some());
        let chosen = self.current.borrow().chosen.clone();
        let Some((_, screen)) = screens_model::selected(state, chosen.as_ref()) else {
            self.empty("No screen is on.");
            return;
        };
        self.hint.set_label(&self.hint_text());
        self.details.set_visible(true);

        let tiles = screens_model::layout_map(config, state, MAP_ROOM.0, MAP_ROOM.1, MAP_GAP);
        self.current.borrow_mut().geometry =
            screens_model::map_geometry(state, MAP_ROOM.0, MAP_ROOM.1, MAP_GAP);
        let mut css = String::new();
        let mut widgets = Vec::new();
        for tile in &tiles {
            let name = format!("map-screen-{}", tile.number);
            let _ = writeln!(
                css,
                "#{name} {{ border-color: {color}; }} #{name} .dc-big-number {{ color: {color}; }}",
                color = tile.color
            );
            let widget = self.map_tile(tile, &name, tile.id == screen.id);
            self.map.put(&widget, f64::from(tile.x), f64::from(tile.y));
            widgets.push((tile.id.clone(), widget));
        }
        *self.tiles.borrow_mut() = widgets;
        self.show_mouse_tags();
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        self.css.load_from_data(&css);
        self.show_chosen();
    }

    /// Shows the chosen screen: its ring in the map, and its settings.
    /// The map is not built again, so a double click reaches the screen.
    fn show_chosen(&self) {
        let (config, state, chosen) = {
            let current = self.current.borrow();
            (
                current.config.clone(),
                current.screens.clone(),
                current.chosen.clone(),
            )
        };
        let Some(state) = state else {
            return;
        };
        let Some((number, screen)) = screens_model::selected(&state, chosen.as_ref()) else {
            return;
        };
        for (id, widget) in self.tiles.borrow().iter() {
            if *id == screen.id {
                widget.add_css_class("selected");
            } else {
                widget.remove_css_class("selected");
            }
        }
        let mut css = String::new();
        let color = screen_color(&config, &screen.id, number);
        let _ = writeln!(css, "#screen-badge {{ background-color: {color}; }}");
        for (index, (swatch_color, swatch)) in self.settings.swatches.iter().enumerate() {
            let _ = writeln!(
                css,
                "#swatch-{index} {{ background-color: {swatch_color}; }}"
            );
            if *swatch_color == color {
                swatch.add_css_class("on");
            } else {
                swatch.remove_css_class("on");
            }
        }
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        self.chosen_css.load_from_data(&css);
        self.show_settings(&config, &state, number, screen);
    }

    /// The page without screens: a sentence, no settings.
    fn empty(&self, text: &str) {
        self.hint.set_label(text);
        self.details.set_visible(false);
        self.current.borrow_mut().shown = None;
    }

    /// A screen of the map: number, name, line, tags. A click chooses it.
    fn map_tile(self: &Rc<Self>, tile: &MapTile, widget_name: &str, chosen: bool) -> gtk::Box {
        let size = match tile.size {
            TileSize::Large => "large",
            TileSize::Medium => "medium",
            TileSize::Small => "small",
        };
        let mut classes = vec!["dc-map-screen", size];
        if chosen {
            classes.push("selected");
        }
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>())
            .width_request(tile.width)
            .height_request(tile.height)
            .overflow(gtk::Overflow::Hidden)
            .tooltip_text(&tile.name)
            .build();
        widget.set_widget_name(widget_name);
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let number = gtk::Label::builder()
            .label(tile.number.to_string())
            .css_classes(["dc-big-number"])
            .build();
        head.append(&number);
        let name = ui::label(&tile.name, &["dc-map-name"]);
        name.set_wrap(false);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_valign(gtk::Align::Baseline);
        number.set_valign(gtk::Align::Baseline);
        head.append(&name);
        widget.append(&head);
        if tile.size != TileSize::Small {
            let details = ui::label(&tile.details, &["dc-mono"]);
            details.set_wrap(false);
            details.set_ellipsize(gtk::pango::EllipsizeMode::End);
            widget.append(&details);
            let tags = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(6)
                .margin_top(8)
                .build();
            if tile.primary {
                tags.append(&ui::tag("Primary", TagKind::WarnStrong));
                if tile.size == TileSize::Large {
                    tags.append(&ui::tag("Top bar here", TagKind::Neutral));
                }
            }
            if tile.builtin {
                tags.append(&ui::tag("Built-in", TagKind::Neutral));
            }
            // The tags of a small screen go to the bottom, as in the design.
            if !tile.primary {
                tags.set_vexpand(true);
                tags.set_valign(gtk::Align::End);
            }
            widget.append(&tags);
        }
        // At the bottom, as in the design.
        let mouse = ui::mouse_tag("Your mouse is here");
        mouse.set_halign(gtk::Align::Start);
        mouse.set_valign(gtk::Align::End);
        mouse.set_vexpand(true);
        widget.append(&mouse);
        self.mouse_tags.borrow_mut().push((tile.id.clone(), mouse));
        let click = gtk::GestureClick::new();
        let page = Rc::clone(self);
        let id = tile.id.clone();
        let number = tile.number;
        click.connect_pressed(move |gesture, presses, _, _| {
            if presses == 2 {
                let number = i32::try_from(number).unwrap_or(0);
                if let Some(widget) = gesture.widget() {
                    let _ = widget.activate_action("app.identify", Some(&number.to_variant()));
                }
                return;
            }
            page.current.borrow_mut().chosen = Some(id.clone());
            page.show_chosen();
        });
        widget.add_controller(click);
        self.add_drag(&widget, tile);
        widget
    }

    /// Lets a screen be dragged in the map. On the drop, GNOME is asked to
    /// put it there; it touches the nearest screen without covering one.
    fn add_drag(self: &Rc<Self>, widget: &gtk::Box, tile: &MapTile) {
        let drag = gtk::GestureDrag::new();
        let start = (f64::from(tile.x), f64::from(tile.y));
        let connector = self
            .current
            .borrow()
            .screens
            .as_ref()
            .and_then(|state| state.screen(&tile.id))
            .map(|screen| screen.connector.clone());
        let page = Rc::clone(self);
        let moving = widget.clone();
        drag.connect_drag_update(move |_, dx, dy| {
            if dx.hypot(dy) >= DRAG_THRESHOLD {
                page.map.move_(&moving, start.0 + dx, start.1 + dy);
            }
        });
        let page = Rc::clone(self);
        drag.connect_drag_end(move |_, dx, dy| {
            if dx.hypot(dy) < DRAG_THRESHOLD {
                return;
            }
            let geometry = page.current.borrow().geometry;
            if let (Some(geometry), Some(connector)) = (geometry, connector.clone()) {
                let (x, y) = geometry.to_desktop(start.0 + dx, start.1 + dy);
                page.ask(Change::Move { connector, x, y });
            }
            // Back to its place until GNOME says where it is: not now, the
            // screen being dragged is still handling this signal.
            let page = Rc::clone(&page);
            gtk::glib::idle_add_local_once(move || page.show_again());
        });
        widget.add_controller(drag);
    }

    /// Fills the primary box, the resolutions and the scales of a screen.
    fn show_display_settings(&self, state: &DisplayState, screen: &Screen) {
        let settings = &self.settings;
        let setup = self.current.borrow().setup.clone();
        let monitor = setup.as_ref().and_then(|s| s.monitor(&screen.connector));
        let resolutions = monitor
            .map(screens_model::resolution_choices)
            .unwrap_or_default();
        let scale = screen.placement.map_or(1.0, |p| p.scale);
        let scales = monitor
            .map(|m| screens_model::scale_choices(m, scale))
            .unwrap_or_default();
        self.updating.set(true);
        settings.primary.set_active(screen.is_primary());
        // The primary screen stays primary until another one is chosen.
        settings
            .primary
            .set_sensitive(setup.is_some() && !screen.is_primary() && state.enabled().len() > 1);
        fill_list(&settings.resolution, &settings.resolutions, &resolutions);
        fill_list(&settings.scale, &settings.scales, &scales);
        self.updating.set(false);
        let mut current = self.current.borrow_mut();
        current.connector = Some(screen.connector.clone());
        current.resolution_values = resolutions.into_iter().map(|c| c.value).collect();
        current.scale_values = scales.into_iter().map(|c| c.value).collect();
    }

    /// Fills the two cards with the shown screen.
    fn show_settings(&self, config: &Config, state: &DisplayState, number: usize, screen: &Screen) {
        let settings = &self.settings;
        settings.badge.set_label(&number.to_string());
        let label = config.screen_label(screen);
        let same_screen = self
            .current
            .borrow()
            .shown
            .as_ref()
            .is_some_and(|(id, _)| *id == screen.id);
        // The text is left alone while the user types in it.
        if !(same_screen && settings.name.has_focus()) {
            settings.name.set_text(label);
        }
        self.current.borrow_mut().shown = Some((screen.id.clone(), screen.name().to_owned()));
        self.show_display_settings(state, screen);

        let sound = self.current.borrow().sound.clone();
        let speakers = sound.as_ref().and_then(|sound| {
            screens_model::screen_speakers(sound, state, screen).map(|o| (sound, o))
        });
        if let Some((sound, output)) = speakers {
            settings.speaker_row.set_visible(true);
            settings.no_speaker.set_visible(false);
            settings.speaker_name.set_label(config.output_label(output));
            settings
                .speaker_system
                .set_label(&format!("System name: \"{}\"", output.description));
            let in_use = sound.default_output.as_ref() == Some(&output.id);
            settings.speaker_use.set_visible(!in_use);
            settings.speaker_used.set_visible(in_use);
            self.current.borrow_mut().speakers = Some(output.id.clone());
        } else {
            settings.speaker_row.set_visible(false);
            settings.no_speaker.set_visible(true);
            settings.no_speaker.set_label(if screen.builtin {
                "The sound of the laptop is on the Audio page."
            } else {
                "DeskCheck sees no speakers for this screen."
            });
            self.current.borrow_mut().speakers = None;
        }

        settings.maker.set_label(&screens_model::maker(
            &screen.id.vendor,
            self.pnp_ids.as_deref(),
        ));
        settings.model.set_label(&screen.id.product);
        settings
            .serial
            .set_label(&screens_model::serial_label(&screen.id));
        settings.connector.set_label(&screen.connector);
    }
}
