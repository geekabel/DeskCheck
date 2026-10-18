//! The Places page (DC-29), as in the design: the list of places on the
//! left, the place chosen on the right, with what tells it ("Recognized
//! when"), the outputs and microphones by preference, the screens, and
//! what to do on arrival.
//!
//! Each change is saved at once in the settings file, then the window
//! shows everything again. The page is built again only when what it
//! shows changed, so an open menu stays open while the sound moves.
//!
//! The lists by preference are ordered by dragging a row onto another one
//! (`gtk::DragSource` and `gtk::DropTarget`, which carry the position of
//! the row).

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;

use adw::prelude::*;
use deskcheck_config::{Config, Store};
use deskcheck_core::audio::{AudioState, DeviceId};
use deskcheck_core::display::DisplayState;
use deskcheck_core::places::{OnArrival, Place, SavedLayout};
use gtk::gdk;

use crate::backend::{Backend, Request};
use crate::now_model::screen_color;
use crate::place_watch::PlaceWatch;
use crate::places_model;
use crate::ui::{self, ButtonKind, TagKind};

/// The width of the list of places, as in the design.
const LIST_WIDTH: i32 = 230;

/// The height of the tallest screen of the small map.
const MINI_HEIGHT: i32 = 58;

/// A change of the place shown, run by a choice of a menu.
type PlaceChange = Box<dyn Fn(&mut Place)>;

/// A choice of an "Add" menu: its text, and the change it makes.
type MenuItem = (String, PlaceChange);

/// A choice of the menu of a row: its text, and the change it makes to
/// the row at a position.
type RowChoice = (&'static str, fn(&mut Place, usize));

/// Which list of devices a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Outputs,
    Never,
    Inputs,
}

/// What the page shows. Built again only when it changes.
#[derive(Debug, Clone, PartialEq)]
struct View {
    config: Config,
    outputs: Vec<(DeviceId, String)>,
    inputs: Vec<(DeviceId, String)>,
    screens: Option<DisplayState>,
    wifi: Option<String>,
    current: Option<String>,
    selected: usize,
}

impl View {
    fn place(&self) -> Option<&Place> {
        self.config.places().get(self.selected)
    }

    /// A device by the name the user gave it, the name of the sound
    /// server, or its id when it is not plugged in.
    fn device_name(&self, id: &DeviceId) -> String {
        if let Some(name) = self.config.audio_name(id) {
            return name.to_owned();
        }
        self.outputs
            .iter()
            .chain(&self.inputs)
            .find(|(seen, _)| seen == id)
            .map_or_else(|| id.as_str().to_owned(), |(_, name)| name.clone())
    }

    fn plugged(&self, id: &DeviceId) -> bool {
        self.outputs
            .iter()
            .chain(&self.inputs)
            .any(|(seen, _)| seen == id)
    }
}

/// The Places page.
pub struct PlacesPage {
    /// The page, to put in the window.
    pub root: gtk::ScrolledWindow,
    backend: Backend,
    watch: Rc<PlaceWatch>,
    list: gtk::Box,
    detail: gtk::Box,
    selected: Cell<usize>,
    shown: RefCell<Option<View>>,
    /// The sound read last, for a new place: its default devices.
    sound: RefCell<Option<AudioState>>,
    css: gtk::CssProvider,
}

impl PlacesPage {
    /// Builds the page, empty until [`PlacesPage::show`].
    pub fn new(backend: Backend, watch: Rc<PlaceWatch>) -> Rc<Self> {
        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(16)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(24)
            .margin_end(24)
            .build();
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .width_request(LIST_WIDTH)
            .hexpand(false)
            .build();
        let detail = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .hexpand(true)
            .build();
        body.append(&list);
        body.append(&detail);
        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&body)
            .vexpand(true)
            .build();
        let css = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        Rc::new(Self {
            root,
            backend,
            watch,
            list,
            detail,
            selected: Cell::new(0),
            shown: RefCell::default(),
            sound: RefCell::default(),
            css,
        })
    }

    /// Shows the places, with what is plugged in now.
    pub fn show(
        self: &Rc<Self>,
        config: &Config,
        sound: Option<&Result<AudioState, String>>,
        screens: Option<&Result<DisplayState, String>>,
        wifi: Option<&str>,
    ) {
        let count = config.places().len();
        if self.selected.get() >= count {
            self.selected.set(count.saturating_sub(1));
        }
        let sound = sound.and_then(|s| s.as_ref().ok());
        self.sound.replace(sound.cloned());
        let view = View {
            config: config.clone(),
            outputs: sound
                .map(|state| {
                    state
                        .outputs
                        .iter()
                        .map(|o| (o.id.clone(), o.description.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            inputs: sound
                .map(|state| {
                    state
                        .inputs
                        .iter()
                        .filter(|i| !i.is_monitor())
                        .map(|i| (i.id.clone(), i.description.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            screens: screens.and_then(|s| s.as_ref().ok()).cloned(),
            wifi: wifi.map(str::to_owned),
            current: self.watch.current(),
            selected: self.selected.get(),
        };
        if self.shown.borrow().as_ref() == Some(&view) {
            return;
        }
        self.build_list(&view);
        self.build_detail(&view);
        self.shown.replace(Some(view));
    }

    /// Changes the settings file, then asks to show everything again.
    fn edit(&self, change: impl FnOnce(&mut Config)) {
        let saved = Store::new().and_then(|store| {
            let mut config = store.load()?;
            change(&mut config);
            store.save(&config)
        });
        match saved {
            Ok(()) => self.backend.request(Request::Refresh),
            Err(error) => ui::toast(
                &self.root,
                &format!("The place could not be saved: {error}"),
            ),
        }
    }

    /// Changes the place shown.
    fn edit_place(&self, change: impl FnOnce(&mut Place)) {
        let at = self.selected.get();
        self.edit(|config| {
            if let Some(place) = config.places_mut().get_mut(at) {
                change(place);
            }
        });
    }

    /// Shows another place.
    fn select(self: &Rc<Self>, at: usize) {
        self.selected.set(at);
        let view = self.shown.borrow().clone();
        if let Some(mut view) = view {
            view.selected = at;
            self.build_list(&view);
            self.build_detail(&view);
            self.shown.replace(Some(view));
        }
    }

    /// Adds a place made from what is in use now, and shows it.
    fn add_place(self: &Rc<Self>, view: &View) {
        let name = places_model::unique_name(view.config.places(), "New place");
        let place = places_model::new_place(
            name,
            self.sound.borrow().as_ref(),
            view.screens.as_ref(),
            view.wifi.as_deref(),
        );
        let at = view.config.places().len();
        self.selected.set(at);
        self.edit(|config| config.places_mut().push(place));
    }

    /// The list of places, on the left.
    fn build_list(self: &Rc<Self>, view: &View) {
        ui::clear(&self.list);
        let title = ui::label("Places", &["dc-page-title"]);
        title.set_margin_bottom(6);
        self.list.append(&title);
        for (at, place) in view.config.places().iter().enumerate() {
            let card = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(3)
                .build();
            let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let name = ui::label(&place.name, &["dc-place-card-name"]);
            name.set_hexpand(true);
            top.append(&name);
            if view.current.as_deref() == Some(place.name.as_str()) {
                top.append(&ui::tag("Now", TagKind::AccentStrong));
            }
            card.append(&top);
            card.append(&ui::label(
                &places_model::place_summary(&view.config, place),
                &["dc-small"],
            ));
            let button = gtk::Button::builder()
                .child(&card)
                .css_classes(["dc-place-card"])
                .build();
            if at == view.selected {
                button.add_css_class("selected");
            }
            let page = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(page) = page.upgrade() {
                    page.select(at);
                }
            });
            self.list.append(&button);
        }
        let add = ui::button("Add a place", Some("dc-add-symbolic"), ButtonKind::Flat);
        add.set_halign(gtk::Align::Start);
        add.set_tooltip_text(Some(
            "A new place, with the sound, microphone and screens in use now",
        ));
        {
            let page = Rc::downgrade(self);
            let view = view.clone();
            add.connect_clicked(move |_| {
                if let Some(page) = page.upgrade() {
                    page.add_place(&view);
                }
            });
        }
        self.list.append(&add);
        let spacer = gtk::Box::builder().vexpand(true).build();
        self.list.append(&spacer);
        let note = ui::label(
            "A place remembers your choices of sound, microphone and screens. DeskCheck \
             recognizes it on its own and applies it.",
            &["dc-small"],
        );
        note.set_margin_start(4);
        self.list.append(&note);
    }

    /// The place chosen, on the right.
    fn build_detail(self: &Rc<Self>, view: &View) {
        ui::clear(&self.detail);
        let Some(place) = view.place() else {
            self.build_empty(view);
            return;
        };
        self.detail.append(&self.title_row(place));
        self.detail.append(&self.conditions_card(view, place));
        self.detail.append(&self.sound_card(view, place));
        let bottom = gtk::Grid::builder()
            .column_spacing(12)
            .column_homogeneous(true)
            .build();
        bottom.attach(&self.screens_card(view, place), 0, 0, 1, 1);
        bottom.attach(&self.arrival_card(place), 1, 0, 1, 1);
        self.detail.append(&bottom);
    }

    /// Without any place: what a place is, and the button to add one.
    fn build_empty(self: &Rc<Self>, view: &View) {
        let card = ui::card();
        card.append(&ui::label("No place yet", &["dc-place-title"]));
        card.append(&ui::label(
            "Add a place where you work, like your office or home. DeskCheck takes the \
             sound, microphone and screens in use now, and how to recognize the place.",
            &["dc-small"],
        ));
        let add = ui::button("Add a place", Some("dc-add-symbolic"), ButtonKind::Primary);
        add.set_halign(gtk::Align::Start);
        let page = Rc::downgrade(self);
        let view = view.clone();
        add.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.add_place(&view);
            }
        });
        card.append(&add);
        self.detail.append(&card);
    }

    /// The name of the place, and the Rename button with its form.
    fn title_row(self: &Rc<Self>, place: &Place) -> gtk::Box {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .height_request(34)
            .build();
        let title = ui::label(&place.name, &["dc-place-title"]);
        title.set_hexpand(true);
        row.append(&title);

        let form = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .css_classes(["dc-rename"])
            .width_request(320)
            .build();
        form.append(&ui::label("Its name", &["dc-name"]));
        let entry = gtk::Entry::builder().text(&place.name).build();
        form.append(&entry);
        let buttons = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        let delete = ui::button("Delete this place", None, ButtonKind::Flat);
        delete.add_css_class("dc-danger");
        delete.set_hexpand(true);
        delete.set_halign(gtk::Align::Start);
        let save = ui::button("Save", None, ButtonKind::Primary);
        buttons.append(&delete);
        buttons.append(&save);
        form.append(&buttons);
        let popover = gtk::Popover::builder().child(&form).build();
        let rename = gtk::MenuButton::builder()
            .popover(&popover)
            .valign(gtk::Align::Center)
            .css_classes(["dc-btn", "dc-flat", "dc-compact"])
            .build();
        // A label as child: GTK then draws no arrow, as in the design.
        rename.set_child(Some(&gtk::Label::new(Some("Rename"))));
        row.append(&rename);

        let name = place.name.clone();
        let rename_to =
            {
                let page = Rc::downgrade(self);
                let popover = popover.clone();
                Rc::new(move |text: String| {
                    popover.popdown();
                    let Some(page) = page.upgrade() else {
                        return;
                    };
                    let text = text.trim().to_owned();
                    if text.is_empty() || text == name {
                        return;
                    }
                    if page.shown.borrow().as_ref().is_some_and(|view| {
                        view.config.places().iter().any(|place| place.name == text)
                    }) {
                        ui::toast(&page.root, "Another place has this name already.");
                        return;
                    }
                    // Still the same place: it is not a new arrival.
                    page.watch.renamed(&name, &text);
                    page.edit_place(|place| place.name = text);
                })
            };
        {
            let rename_to = Rc::clone(&rename_to);
            let entry = entry.clone();
            save.connect_clicked(move |_| rename_to(entry.text().to_string()));
        }
        entry.connect_activate(move |entry| rename_to(entry.text().to_string()));
        {
            let page = Rc::downgrade(self);
            delete.connect_clicked(move |_| {
                popover.popdown();
                if let Some(page) = page.upgrade() {
                    let at = page.selected.get();
                    page.selected.set(at.saturating_sub(1));
                    page.edit(|config| {
                        if at < config.places().len() {
                            config.places_mut().remove(at);
                        }
                    });
                }
            });
        }
        row
    }

    /// "Recognized when": the conditions, joined by "or".
    fn conditions_card(self: &Rc<Self>, view: &View, place: &Place) -> gtk::Box {
        let card = card("Recognized when");
        card.set_spacing(8);
        for (at, condition) in place.conditions.iter().enumerate() {
            if at > 0 {
                card.append(&ui::label("or", &["dc-or"]));
            }
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(10)
                .css_classes(["dc-cond"])
                .build();
            row.append(&ui::icon(places_model::condition_icon(condition), 18));
            let text = gtk::Label::builder()
                .use_markup(true)
                .label(places_model::condition_markup(&view.config, condition))
                .xalign(0.0)
                .wrap(true)
                .hexpand(true)
                .build();
            row.append(&text);
            let remove = icon_button("dc-close-symbolic", "Remove this condition");
            let page = Rc::downgrade(self);
            remove.connect_clicked(move |_| {
                if let Some(page) = page.upgrade() {
                    page.edit_place(|place| {
                        if at < place.conditions.len() {
                            place.conditions.remove(at);
                        }
                    });
                }
            });
            row.append(&remove);
            card.append(&row);
        }
        if place.conditions.is_empty() {
            card.append(&ui::label(
                "DeskCheck cannot recognize this place yet: add a condition.",
                &["dc-small"],
            ));
        }
        let choices =
            places_model::condition_choices(place, view.screens.as_ref(), view.wifi.as_deref());
        let items: Vec<MenuItem> = choices
            .into_iter()
            .map(|condition| {
                let label = places_model::condition_markup(&view.config, &condition);
                let add: PlaceChange =
                    Box::new(move |place: &mut Place| place.conditions.push(condition.clone()));
                (label, add)
            })
            .collect();
        card.append(&self.add_menu("Add a condition", items, true));
        card
    }

    /// A flat "+ text" button with a menu of choices. Each choice changes
    /// the place.
    fn add_menu(
        self: &Rc<Self>,
        text: &str,
        items: Vec<MenuItem>,
        markup: bool,
    ) -> gtk::MenuButton {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.append(&ui::icon("dc-add-symbolic", 18));
        content.append(&gtk::Label::new(Some(text)));
        let menu = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(["dc-menu-list"])
            .build();
        let popover = gtk::Popover::builder().child(&menu).build();
        let button = gtk::MenuButton::builder()
            .popover(&popover)
            .halign(gtk::Align::Start)
            .css_classes(["dc-btn", "dc-flat", "dc-compact"])
            .build();
        button.set_child(Some(&content));
        button.set_sensitive(!items.is_empty());
        if items.is_empty() {
            button.set_tooltip_text(Some("Nothing to add from what is plugged in now"));
        }
        for (label, change) in items {
            let text = gtk::Label::builder()
                .use_markup(markup)
                .label(&label)
                .xalign(0.0)
                .build();
            let item = gtk::Button::builder()
                .child(&text)
                .css_classes(["dc-btn", "dc-flat"])
                .build();
            let page = Rc::downgrade(self);
            let popover = popover.clone();
            item.connect_clicked(move |_| {
                popover.popdown();
                if let Some(page) = page.upgrade() {
                    page.edit_place(|place| change(place));
                }
            });
            menu.append(&item);
        }
        button
    }

    /// The outputs and the microphones, by preference.
    fn sound_card(self: &Rc<Self>, view: &View, place: &Place) -> gtk::Box {
        let card = ui::card();
        card.add_css_class("dc-place-card-body");
        let columns = gtk::Grid::builder()
            .column_spacing(16)
            .column_homogeneous(true)
            .build();
        let outputs = gtk::Box::new(gtk::Orientation::Vertical, 6);
        outputs.append(&caption("Output, by preference"));
        for (at, id) in place.outputs.iter().enumerate() {
            outputs.append(&self.device_row(view, Kind::Outputs, at, id));
        }
        for (at, id) in place.never_outputs.iter().enumerate() {
            outputs.append(&self.device_row(view, Kind::Never, at, id));
        }
        let plugged: Vec<DeviceId> = view.outputs.iter().map(|(id, _)| id.clone()).collect();
        let to_add = places_model::devices_to_add(&plugged, &place.outputs, &place.never_outputs);
        outputs.append(&self.add_menu(
            "Add an output",
            device_items(view, to_add, Kind::Outputs),
            false,
        ));
        let inputs = gtk::Box::new(gtk::Orientation::Vertical, 6);
        inputs.append(&caption("Microphone, by preference"));
        for (at, id) in place.inputs.iter().enumerate() {
            inputs.append(&self.device_row(view, Kind::Inputs, at, id));
        }
        let plugged: Vec<DeviceId> = view.inputs.iter().map(|(id, _)| id.clone()).collect();
        let to_add = places_model::devices_to_add(&plugged, &place.inputs, &[]);
        inputs.append(&self.add_menu(
            "Add a microphone",
            device_items(view, to_add, Kind::Inputs),
            false,
        ));
        columns.attach(&outputs, 0, 0, 1, 1);
        columns.attach(&inputs, 1, 0, 1, 1);
        card.append(&columns);
        card.append(&ui::label(
            "The first device plugged in is chosen. Drag a row to change the order. A \
             \"Monitor\" is never taken as a microphone.",
            &["dc-small"],
        ));
        card
    }

    /// A row of a list by preference: grip, number, name and its menu. A
    /// row is dragged onto another one to take its place.
    fn device_row(self: &Rc<Self>, view: &View, kind: Kind, at: usize, id: &DeviceId) -> gtk::Box {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .css_classes(["dc-pref"])
            .build();
        let mut name = view.device_name(id);
        if kind == Kind::Never {
            row.add_css_class("never");
            row.append(&ui::icon("dc-never-symbolic", 18));
            name = format!("Never: {name}");
        } else {
            let grip = ui::icon("dc-grip-symbolic", 18);
            grip.add_css_class("dc-grip");
            row.append(&grip);
            let number = gtk::Label::builder()
                .label((at + 1).to_string())
                .css_classes(["dc-num"])
                .valign(gtk::Align::Center)
                .build();
            row.append(&number);
        }
        let mut text = name;
        if !view.plugged(id) {
            let _ = write!(text, " · not plugged in");
        }
        let label = ui::label(&text, &[]);
        label.set_wrap(false);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_hexpand(true);
        label.set_tooltip_text(Some(id.as_str()));
        row.append(&label);
        row.append(&self.row_menu(kind, at));
        if kind != Kind::Never {
            self.make_draggable(&row, kind, at);
        }
        row
    }

    /// The small menu at the end of a row.
    fn row_menu(self: &Rc<Self>, kind: Kind, at: usize) -> gtk::MenuButton {
        let menu = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(["dc-menu-list"])
            .build();
        let popover = gtk::Popover::builder().child(&menu).build();
        let button = gtk::MenuButton::builder()
            .icon_name("dc-more-symbolic")
            .popover(&popover)
            .valign(gtk::Align::Center)
            .tooltip_text("More")
            .css_classes(["dc-btn", "dc-flat", "dc-mini"])
            .build();
        let choices: Vec<RowChoice> = match kind {
            Kind::Outputs => vec![
                ("Never use it here", |place, at| {
                    if at < place.outputs.len() {
                        let id = place.outputs.remove(at);
                        place.never_outputs.push(id);
                    }
                }),
                ("Remove from the list", |place, at| {
                    if at < place.outputs.len() {
                        place.outputs.remove(at);
                    }
                }),
            ],
            Kind::Never => vec![
                ("Allow it again", |place, at| {
                    if at < place.never_outputs.len() {
                        let id = place.never_outputs.remove(at);
                        place.outputs.push(id);
                    }
                }),
                ("Remove from the list", |place, at| {
                    if at < place.never_outputs.len() {
                        place.never_outputs.remove(at);
                    }
                }),
            ],
            Kind::Inputs => vec![("Remove from the list", |place, at| {
                if at < place.inputs.len() {
                    place.inputs.remove(at);
                }
            })],
        };
        for (text, change) in choices {
            let item = ui::button(text, None, ButtonKind::Flat);
            item.set_halign(gtk::Align::Fill);
            if let Some(label) = item
                .child()
                .and_then(|content| content.last_child())
                .and_downcast::<gtk::Label>()
            {
                label.set_xalign(0.0);
            }
            let page = Rc::downgrade(self);
            let popover = popover.clone();
            item.connect_clicked(move |_| {
                popover.popdown();
                if let Some(page) = page.upgrade() {
                    page.edit_place(|place| change(place, at));
                }
            });
            menu.append(&item);
        }
        button
    }

    /// Lets a row be dragged onto another row of the same list.
    fn make_draggable(self: &Rc<Self>, row: &gtk::Box, kind: Kind, at: usize) {
        let tag = list_tag(kind);
        let source = gtk::DragSource::builder()
            .actions(gdk::DragAction::MOVE)
            .build();
        let carried = format!("{tag}:{at}");
        source.connect_prepare(move |_, _, _| {
            Some(gdk::ContentProvider::for_value(&carried.to_value()))
        });
        {
            let row = row.clone();
            source.connect_drag_begin(move |source, _| {
                let picture = gtk::WidgetPaintable::new(Some(&row));
                source.set_icon(Some(&picture), 20, 18);
            });
        }
        row.add_controller(source);

        let target = gtk::DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
        {
            let row = row.clone();
            target.connect_enter(move |_, _, _| {
                row.add_css_class("drop");
                gdk::DragAction::MOVE
            });
        }
        {
            let row = row.clone();
            target.connect_leave(move |_| row.remove_css_class("drop"));
        }
        let page = Rc::downgrade(self);
        target.connect_drop(move |_, value, _, _| {
            let Some(from) = value
                .get::<String>()
                .ok()
                .and_then(|text| dropped_from(&text, tag))
            else {
                return false;
            };
            let Some(page) = page.upgrade() else {
                return false;
            };
            page.edit_place(|place| {
                let list = match kind {
                    Kind::Outputs => &mut place.outputs,
                    Kind::Never => &mut place.never_outputs,
                    Kind::Inputs => &mut place.inputs,
                };
                places_model::move_item(list, from, at);
            });
            true
        });
        row.add_controller(target);
    }

    /// The layout of the screens kept for the place.
    fn screens_card(self: &Rc<Self>, view: &View, place: &Place) -> gtk::Box {
        let card = card("Screens");
        card.set_spacing(8);
        let map = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .height_request(MINI_HEIGHT + 2)
            .build();
        let mut css = String::new();
        if let Some(layout) = &place.layout {
            for screen in places_model::mini_map(layout, MINI_HEIGHT) {
                let tile = gtk::Label::builder()
                    .label(screen.number.to_string())
                    .width_request(screen.width)
                    .height_request(screen.height)
                    .valign(gtk::Align::End)
                    .css_classes(["dc-mini-screen"])
                    .build();
                if screen.primary {
                    tile.add_css_class("primary");
                }
                tile.set_widget_name(&format!("place-screen-{}", screen.number));
                tile.set_tooltip_text(Some(&places_model::screen_name(&view.config, &screen.id)));
                let color = screen_color(&view.config, &screen.id, screen.number);
                let _ = writeln!(
                    css,
                    "#place-screen-{} {{ border-color: {color}; color: {color}; }}",
                    screen.number
                );
                map.append(&tile);
            }
        }
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        self.css.load_from_data(&css);
        let words = ui::label(
            &places_model::layout_words(place.layout.as_ref()),
            &["dc-layout-words"],
        );
        words.set_wrap(false);
        words.set_valign(gtk::Align::End);
        words.set_margin_start(6);
        map.append(&words);
        card.append(&map);
        let take = ui::button("Take the current layout", None, ButtonKind::Normal);
        take.add_css_class("dc-compact");
        take.set_halign(gtk::Align::Start);
        take.set_sensitive(view.screens.is_some());
        let layout = view.screens.as_ref().map(SavedLayout::from_state);
        let page = Rc::downgrade(self);
        take.connect_clicked(move |_| {
            if let (Some(page), Some(layout)) = (page.upgrade(), layout.clone()) {
                page.edit_place(|place| place.layout = Some(layout));
            }
        });
        card.append(&take);
        card
    }

    /// What DeskCheck does when it recognizes the place.
    fn arrival_card(self: &Rc<Self>, place: &Place) -> gtk::Box {
        let card = card("When this place is recognized");
        card.set_spacing(10);
        let leader = gtk::CheckButton::new();
        for (choice, text) in [
            (OnArrival::Apply, "Apply and tell me"),
            (OnArrival::Ask, "Ask me before"),
            (OnArrival::Nothing, "Do nothing"),
        ] {
            let radio = gtk::CheckButton::builder()
                .label(text)
                .active(place.on_arrival == choice)
                .css_classes(["dc-radio"])
                .build();
            // In a group, GTK draws a radio (see the Audio page).
            radio.set_group(Some(&leader));
            let page = Rc::downgrade(self);
            radio.connect_toggled(move |radio| {
                if radio.is_active() {
                    if let Some(page) = page.upgrade() {
                        page.edit_place(|place| place.on_arrival = choice);
                    }
                }
            });
            card.append(&radio);
        }
        card
    }
}

/// A card of the page with its title.
fn card(title: &str) -> gtk::Box {
    let card = ui::card();
    card.add_css_class("dc-place-card-body");
    card.append(&caption(title));
    card
}

/// The title of a card, a little stronger than on other pages, as in the
/// design of this page.
fn caption(text: &str) -> gtk::Label {
    let caption = ui::caption(text);
    caption.add_css_class("strong");
    caption
}

/// A small flat button with only an icon.
fn icon_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon_name)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .css_classes(["dc-btn", "dc-flat", "dc-mini"])
        .build()
}

/// The choices of an "Add" menu of devices.
fn device_items(view: &View, ids: Vec<DeviceId>, kind: Kind) -> Vec<MenuItem> {
    ids.into_iter()
        .map(|id| {
            let label = view.device_name(&id);
            let add: PlaceChange = Box::new(move |place: &mut Place| match kind {
                Kind::Inputs => place.inputs.push(id.clone()),
                Kind::Outputs | Kind::Never => place.outputs.push(id.clone()),
            });
            (label, add)
        })
        .collect()
}

/// The name of a list, carried with a dragged row: a row is dropped only
/// on its own list.
fn list_tag(kind: Kind) -> &'static str {
    match kind {
        Kind::Outputs => "outputs",
        Kind::Never => "never",
        Kind::Inputs => "inputs",
    }
}

/// The position of the dragged row, if it comes from the list `tag`.
fn dropped_from(text: &str, tag: &str) -> Option<usize> {
    let (list, at) = text.split_once(':')?;
    (list == tag).then(|| at.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_dropped_only_on_its_own_list() {
        assert_eq!(dropped_from("outputs:2", "outputs"), Some(2));
        assert_eq!(dropped_from("inputs:2", "outputs"), None);
        assert_eq!(dropped_from("outputs:x", "outputs"), None);
        assert_eq!(dropped_from("nothing", "outputs"), None);
    }

    #[test]
    fn unknown_devices_keep_their_id() {
        let view = View {
            config: Config::default(),
            outputs: vec![(DeviceId::new("usb"), "USB headset".to_owned())],
            inputs: Vec::new(),
            screens: None,
            wifi: None,
            current: None,
            selected: 0,
        };
        assert_eq!(view.device_name(&DeviceId::new("usb")), "USB headset");
        assert_eq!(view.device_name(&DeviceId::new("gone")), "gone");
        assert!(view.plugged(&DeviceId::new("usb")));
        assert!(!view.plugged(&DeviceId::new("gone")));
    }
}
