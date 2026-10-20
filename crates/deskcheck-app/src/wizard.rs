//! The first-run assistant (DC-32), as in the design "Nommage": a window
//! with the four steps on the left, the devices of the step, and the name
//! and the icon (or the color, for a screen) of the one chosen.
//!
//! It opens at the first start of DeskCheck, and from the menu ("Learn my
//! devices"). Each name is saved at once, like on the other pages.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use deskcheck_config::{Color, Config, DeviceIcon, Store};
use deskcheck_core::audio::level::{LevelBar, to_db};
use deskcheck_core::audio::{AudioState, DeviceId};
use deskcheck_core::display::{DisplayState, ScreenId};
use gtk::{gdk, glib};

use crate::audio_model::VOICE_DB;
use crate::backend::{Backend, Request};
use crate::now_model;
use crate::places_model;
use crate::screens_model::SWATCHES;
use crate::ui::{self, ButtonKind};
use crate::wizard_model::{self, Step, WizardDevice};

/// How long a voice heard keeps a microphone "hearing you".
const VOICE_FRESH: Duration = Duration::from_millis(1500);

/// The blocks of the level meters.
const METER_BLOCKS: usize = 10;

/// What is chosen in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Chosen {
    Device(DeviceId),
    Screen(ScreenId),
}

/// What the assistant knows of the system.
#[derive(Default)]
struct Latest {
    config: Config,
    sound: Option<AudioState>,
    screens: Option<DisplayState>,
    wifi: Option<String>,
}

/// A microphone row: its meter and how loud it is.
struct MicRow {
    row: gtk::Box,
    title: gtk::Label,
    /// Its title when it does not hear the voice.
    name: String,
    meter: ui::Meter,
    level: LevelBar,
    heard_at: Option<Instant>,
}

/// The assistant.
pub struct Wizard {
    window: adw::Window,
    backend: Backend,
    step: Cell<Step>,
    step_rows: Vec<(gtk::Box, gtk::Label, gtk::Label)>,
    counter: gtk::Label,
    heading: gtk::Label,
    subtitle: gtk::Label,
    list: gtk::Box,
    form: gtk::Box,
    entry: gtk::Entry,
    chips: gtk::FlowBox,
    picker: gtk::Box,
    follows: gtk::CheckButton,
    place_box: gtk::Box,
    place_summary: gtk::Label,
    add_place: gtk::Button,
    skip: gtk::Button,
    back: gtk::Button,
    next: gtk::Button,
    chosen: RefCell<Option<Chosen>>,
    /// `true` once the user clicked a microphone: the voice no longer
    /// chooses.
    manual: Cell<bool>,
    mics: RefCell<HashMap<DeviceId, MicRow>>,
    latest: RefCell<Latest>,
    /// What the list shows, to build it again only when it changes.
    shown: RefCell<Option<String>>,
    /// Called when the assistant closes.
    on_close: RefCell<Option<Box<dyn Fn()>>>,
    css: gtk::CssProvider,
    swatch_css: gtk::CssProvider,
}

impl Wizard {
    /// Builds the assistant, hidden.
    #[allow(
        clippy::too_many_lines,
        reason = "builds the parts of the design in order, easier to read in one place"
    )]
    pub fn new(
        app: &adw::Application,
        parent: &adw::ApplicationWindow,
        backend: Backend,
    ) -> Rc<Self> {
        // The header: title, step, close.
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["dc-wizard-header"])
            .build();
        let title = ui::label("Let's learn your devices", &["dc-wizard-title"]);
        title.set_hexpand(true);
        header.append(&title);
        let counter = ui::label("", &["dc-small"]);
        counter.set_wrap(false);
        header.append(&counter);
        let close = gtk::Button::builder()
            .icon_name("dc-close-symbolic")
            .tooltip_text("Close")
            .css_classes(["dc-btn", "dc-flat", "dc-icon-only"])
            .build();
        header.append(&close);

        // The steps.
        let steps = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .width_request(210)
            .css_classes(["dc-wizard-steps"])
            .build();
        let mut step_rows = Vec::new();
        for (at, step) in Step::ALL.iter().enumerate() {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(10)
                .css_classes(["dc-wizard-step"])
                .build();
            let dot = gtk::Label::builder()
                .label((at + 1).to_string())
                .css_classes(["dc-wizard-dot"])
                .valign(gtk::Align::Center)
                .build();
            row.append(&dot);
            let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
            texts.set_valign(gtk::Align::Center);
            texts.append(&ui::label(step.title(), &["dc-wizard-step-title"]));
            let note = ui::label("", &["dc-wizard-step-note"]);
            texts.append(&note);
            row.append(&texts);
            steps.append(&row);
            step_rows.push((row, dot, note));
        }

        // The page of the step.
        let main = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .hexpand(true)
            .css_classes(["dc-wizard-main"])
            .build();
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let heading = ui::label("", &["dc-wizard-heading"]);
        let subtitle = ui::label("", &["dc-page-subtitle"]);
        titles.append(&heading);
        titles.append(&subtitle);
        main.append(&titles);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
        main.append(&list);

        // The name, the suggestions, the icon or the color.
        let form = gtk::Box::new(gtk::Orientation::Vertical, 8);
        form.append(&ui::label("Its name", &["dc-name", "dc-bold"]));
        let entry = gtk::Entry::builder()
            .css_classes(["dc-wizard-entry"])
            .build();
        form.append(&entry);
        let chips = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(6)
            .row_spacing(6)
            .max_children_per_line(6)
            .build();
        form.append(&chips);
        let picker = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_top(6)
            .build();
        form.append(&picker);
        let follows = gtk::CheckButton::builder()
            .label("It follows me everywhere: the same name at every place")
            .active(true)
            .sensitive(false)
            .tooltip_text("DeskCheck keeps one name for each device, at every place")
            .build();
        form.append(&follows);
        main.append(&form);

        // The step of the places: a name and what is kept.
        let place_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let place_summary = ui::label("", &["dc-panel-text"]);
        place_box.append(&place_summary);
        let add_place = ui::button(
            "Add this place",
            Some("dc-add-symbolic"),
            ButtonKind::Primary,
        );
        add_place.set_halign(gtk::Align::Start);
        place_box.append(&add_place);
        main.append(&place_box);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.set_vexpand(true);
        body.append(&steps);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&main)
            .hexpand(true)
            .build();
        body.append(&scroller);

        // The footer.
        let footer = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["dc-wizard-footer"])
            .build();
        let skip = ui::button("Skip this step", None, ButtonKind::Flat);
        footer.append(&skip);
        let spacer = gtk::Box::builder().hexpand(true).build();
        footer.append(&spacer);
        let back = ui::button("Back", None, ButtonKind::Normal);
        footer.append(&back);
        let next = ui::button("", None, ButtonKind::Primary);
        footer.append(&next);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header);
        content.append(&body);
        content.append(&footer);
        let window = adw::Window::builder()
            .transient_for(parent)
            .application(app)
            .modal(true)
            .title("Let's learn your devices")
            .default_width(880)
            .default_height(720)
            .content(&content)
            .hide_on_close(true)
            .build();
        let css = gtk::CssProvider::new();
        let swatch_css = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            for provider in [&css, &swatch_css] {
                gtk::style_context_add_provider_for_display(
                    &display,
                    provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        }

        let wizard = Rc::new(Self {
            window,
            backend,
            step: Cell::new(Step::Outputs),
            step_rows,
            counter,
            heading,
            subtitle,
            list,
            form,
            entry,
            chips,
            picker,
            follows,
            place_box,
            place_summary,
            add_place,
            skip,
            back,
            next,
            chosen: RefCell::default(),
            manual: Cell::new(false),
            mics: RefCell::default(),
            latest: RefCell::default(),
            shown: RefCell::default(),
            on_close: RefCell::default(),
            css,
            swatch_css,
        });
        wizard.connect(&close);
        wizard
    }

    fn connect(self: &Rc<Self>, close: &gtk::Button) {
        let weak = Rc::downgrade(self);
        let on = move |action: fn(&Rc<Self>)| {
            let weak = weak.clone();
            move |_: &gtk::Button| {
                if let Some(wizard) = weak.upgrade() {
                    action(&wizard);
                }
            }
        };
        close.connect_clicked(on(|w| w.finish()));
        self.skip.connect_clicked(on(|w| w.go(w.step.get().next())));
        self.back.connect_clicked(on(|w| {
            w.save();
            w.go(w.step.get().previous());
        }));
        self.next.connect_clicked(on(|w| {
            w.save();
            w.go(w.step.get().next());
        }));
        self.add_place.connect_clicked(on(Self::add_place));
        let weak = Rc::downgrade(self);
        self.entry.connect_activate(move |_| {
            if let Some(wizard) = weak.upgrade() {
                wizard.save();
            }
        });
        // A name typed is kept when the focus leaves the field too.
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus.connect_leave(move |_| {
            if let Some(wizard) = weak.upgrade() {
                wizard.save();
            }
        });
        self.entry.add_controller(focus);
        let weak = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            if let Some(wizard) = weak.upgrade() {
                wizard.done();
            }
            glib::Propagation::Proceed
        });
    }

    /// Runs `action` when the assistant closes.
    pub fn connect_close(&self, action: impl Fn() + 'static) {
        self.on_close.replace(Some(Box::new(action)));
    }

    /// Opens the assistant at its first step.
    pub fn open(self: &Rc<Self>) {
        self.manual.set(false);
        self.shown.replace(None);
        self.go(Some(Step::Outputs));
        self.window.present();
    }

    /// Shows a step, or closes the assistant after the last one.
    fn go(self: &Rc<Self>, step: Option<Step>) {
        let Some(step) = step else {
            self.finish();
            return;
        };
        self.step.set(step);
        self.chosen.replace(None);
        self.manual.set(false);
        self.shown.replace(None);
        self.refresh();
    }

    /// Closes the assistant: it is not shown at start any more.
    fn finish(&self) {
        self.save();
        self.window.close();
    }

    /// Marks the assistant as seen, and tells the window.
    fn done(&self) {
        edit(&self.backend, |config| config.set_setup_done(true));
        if let Some(action) = self.on_close.borrow().as_ref() {
            action();
        }
    }

    /// Takes the latest state of the system, and shows it.
    pub fn show(
        self: &Rc<Self>,
        config: &Config,
        sound: Option<&AudioState>,
        screens: Option<&DisplayState>,
        wifi: Option<&str>,
    ) {
        self.latest.replace(Latest {
            config: config.clone(),
            sound: sound.cloned(),
            screens: screens.cloned(),
            wifi: wifi.map(str::to_owned),
        });
        if self.window.is_visible() {
            self.refresh();
        }
    }

    /// Shows the current step with the latest state.
    fn refresh(self: &Rc<Self>) {
        let step = self.step.get();
        let latest = self.latest.borrow();
        self.counter.set_label(&step.counter());
        self.heading.set_label(step.heading());
        self.subtitle.set_label(step.subtitle());
        for (at, (row, dot, note)) in self.step_rows.iter().enumerate() {
            let this = Step::ALL[at];
            let text = wizard_model::step_note(
                this,
                &latest.config,
                latest.sound.as_ref(),
                latest.screens.as_ref(),
            );
            note.set_label(text.as_deref().unwrap_or(""));
            note.set_visible(text.is_some());
            let done = at < step.index() || (this != step && text.is_some());
            if this == step {
                row.add_css_class("current");
            } else {
                row.remove_css_class("current");
            }
            dot.set_css_classes(&[
                "dc-wizard-dot",
                if this == step {
                    "current"
                } else if done {
                    "done"
                } else {
                    "todo"
                },
            ]);
            dot.set_label(&if done && this != step {
                "✓".to_owned()
            } else {
                (at + 1).to_string()
            });
        }
        if let Some(label) = self
            .next
            .child()
            .and_then(|content| content.last_child())
            .and_downcast::<gtk::Label>()
        {
            label.set_label(step.next_label());
        }
        self.back.set_sensitive(step.previous().is_some());
        self.skip.set_visible(step.next().is_some());
        drop(latest);
        match step {
            Step::Outputs | Step::Microphones => self.show_devices(step),
            Step::Screens => self.show_screens(),
            Step::Places => self.show_place(),
        }
    }

    /// The outputs or the microphones of the step.
    fn show_devices(self: &Rc<Self>, step: Step) {
        self.form.set_visible(true);
        self.place_box.set_visible(false);
        self.follows.set_visible(step == Step::Microphones);
        let latest = self.latest.borrow();
        let devices = latest.sound.as_ref().map_or_else(Vec::new, |state| {
            if step == Step::Outputs {
                wizard_model::outputs(&latest.config, state)
            } else {
                wizard_model::microphones(&latest.config, state)
            }
        });
        drop(latest);
        if self.chosen.borrow().is_none() {
            if let Some(first) = devices.first() {
                self.chosen.replace(Some(Chosen::Device(first.id.clone())));
                self.fill_form(step, Some(first));
            }
        }
        let key = format!("{step:?}{devices:?}{:?}", self.chosen.borrow());
        if self.shown.borrow().as_deref() == Some(key.as_str()) {
            return;
        }
        self.shown.replace(Some(key));
        ui::clear(&self.list);
        let mut mics = self.mics.borrow_mut();
        let old: HashMap<DeviceId, MicRow> = mics.drain().collect();
        if devices.is_empty() {
            self.list
                .append(&ui::label("DeskCheck sees no device here.", &["dc-small"]));
        }
        let chosen = self.chosen.borrow().clone();
        for device in &devices {
            let selected = chosen == Some(Chosen::Device(device.id.clone()));
            let row = self.device_row(step, device, selected);
            if step == Step::Microphones {
                let meter = ui::Meter::new(METER_BLOCKS);
                row.0.append(&meter.root);
                let previous = old.get(&device.id);
                mics.insert(
                    device.id.clone(),
                    MicRow {
                        row: row.0.clone(),
                        title: row.1.clone(),
                        name: device.title().to_owned(),
                        meter,
                        level: previous.map(|p| p.level).unwrap_or_default(),
                        heard_at: previous.and_then(|p| p.heard_at),
                    },
                );
            }
            self.list.append(&row.0);
        }
    }

    /// A row of a device: icon, name, technical name, and a Play button
    /// for an output. A click chooses it.
    fn device_row(
        self: &Rc<Self>,
        step: Step,
        device: &WizardDevice,
        selected: bool,
    ) -> (gtk::Box, gtk::Label) {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["dc-wizard-device"])
            .build();
        if selected {
            row.add_css_class("selected");
        }
        let icon = ui::icon(device.icon, 20);
        icon.add_css_class("dc-dim-icon");
        row.append(&icon);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
        texts.set_hexpand(true);
        let title = ui::label(device.title(), &["dc-wizard-device-title"]);
        if device.name.is_some() {
            title.add_css_class("named");
        }
        texts.append(&title);
        let technical = ui::label(&device.technical, &["dc-mono"]);
        technical.set_wrap(false);
        technical.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texts.append(&technical);
        row.append(&texts);
        if step == Step::Outputs {
            let play = ui::button("Play", Some("dc-play-symbolic"), ButtonKind::Normal);
            play.add_css_class("dc-compact");
            play.set_tooltip_text(Some("Plays a short chime on this output"));
            let backend = self.backend.clone();
            let id = device.id.clone();
            let weak = Rc::downgrade(self);
            play.connect_clicked(move |_| {
                backend.request(Request::TestOutput(id.clone()));
                if let Some(wizard) = weak.upgrade() {
                    wizard.choose(&Chosen::Device(id.clone()));
                }
            });
            row.append(&play);
        }
        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(self);
        let id = device.id.clone();
        click.connect_released(move |_, _, _, _| {
            if let Some(wizard) = weak.upgrade() {
                wizard.manual.set(true);
                wizard.choose(&Chosen::Device(id.clone()));
            }
        });
        row.add_controller(click);
        (row, title)
    }

    /// Chooses a device or a screen: the one before is saved first.
    fn choose(self: &Rc<Self>, chosen: &Chosen) {
        if self.chosen.borrow().as_ref() == Some(chosen) {
            return;
        }
        self.save();
        self.chosen.replace(Some(chosen.clone()));
        let step = self.step.get();
        let latest = self.latest.borrow();
        match chosen {
            Chosen::Device(id) => {
                let devices = latest.sound.as_ref().map_or_else(Vec::new, |state| {
                    if step == Step::Outputs {
                        wizard_model::outputs(&latest.config, state)
                    } else {
                        wizard_model::microphones(&latest.config, state)
                    }
                });
                drop(latest);
                self.fill_form(step, devices.iter().find(|d| &d.id == id));
            }
            Chosen::Screen(id) => {
                let name = latest.config.screen_name(id).map(str::to_owned);
                let color = latest.screens.as_ref().and_then(|state| {
                    state
                        .numbered()
                        .iter()
                        .find(|(_, s)| &s.id == id)
                        .map(|(n, s)| now_model::screen_color(&latest.config, &s.id, *n))
                });
                drop(latest);
                self.fill_screen_form(name.as_deref(), color);
            }
        }
        self.shown.replace(None);
        self.refresh();
    }

    /// Fills the name, the suggestions and the icons for a device.
    fn fill_form(self: &Rc<Self>, step: Step, device: Option<&WizardDevice>) {
        self.entry
            .set_text(device.and_then(|d| d.name.as_deref()).unwrap_or(""));
        self.fill_chips(step);
        ui::clear(&self.picker);
        let label = ui::label("Icon", &["dc-name", "dc-bold"]);
        label.set_width_request(70);
        self.picker.append(&label);
        let leader: Option<gtk::ToggleButton> = None;
        let mut leader = leader;
        for (icon, text) in step.icons() {
            let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
            content.append(&ui::icon(now_model::chosen_icon(*icon), 20));
            content.append(&gtk::Label::new(Some(text)));
            let button = gtk::ToggleButton::builder()
                .child(&content)
                .active(device.is_some_and(|d| d.choice == *icon))
                .css_classes(["dc-icon-pick"])
                .build();
            if let Some(first) = &leader {
                button.set_group(Some(first));
            } else {
                leader = Some(button.clone());
            }
            let weak = Rc::downgrade(self);
            let icon = *icon;
            button.connect_toggled(move |button| {
                if button.is_active() {
                    if let Some(wizard) = weak.upgrade() {
                        wizard.save_icon(icon);
                    }
                }
            });
            self.picker.append(&button);
        }
    }

    /// The suggested names of the step.
    fn fill_chips(self: &Rc<Self>, step: Step) {
        while let Some(child) = self.chips.first_child() {
            self.chips.remove(&child);
        }
        for name in step.suggestions() {
            let chip = ui::button(name, None, ButtonKind::Normal);
            chip.add_css_class("dc-chip");
            let weak = Rc::downgrade(self);
            chip.connect_clicked(move |_| {
                if let Some(wizard) = weak.upgrade() {
                    wizard.entry.set_text(name);
                    wizard.save();
                }
            });
            self.chips.insert(&chip, -1);
        }
    }

    /// The screens, numbered: "Show the numbers" puts each number on its
    /// screen.
    #[allow(
        clippy::too_many_lines,
        reason = "the rows of the screens, in the order of the design"
    )]
    fn show_screens(self: &Rc<Self>) {
        self.form.set_visible(true);
        self.place_box.set_visible(false);
        self.follows.set_visible(false);
        let latest = self.latest.borrow();
        let screens: Vec<(usize, ScreenId, String, String, String)> = latest
            .screens
            .as_ref()
            .map(|state| {
                state
                    .numbered()
                    .iter()
                    .map(|(number, screen)| {
                        (
                            *number,
                            screen.id.clone(),
                            latest
                                .config
                                .screen_name(&screen.id)
                                .unwrap_or("Not named yet")
                                .to_owned(),
                            format!("{} · {}", screen.name(), screen.connector),
                            now_model::screen_color(&latest.config, &screen.id, *number)
                                .to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let first = screens.first().map(|(_, id, ..)| id.clone());
        let config = latest.config.clone();
        drop(latest);
        if self.chosen.borrow().is_none() {
            if let Some(id) = first {
                let name = config.screen_name(&id).map(str::to_owned);
                let color = screens
                    .first()
                    .and_then(|(_, _, _, _, color)| color.parse().ok());
                self.chosen.replace(Some(Chosen::Screen(id)));
                self.fill_screen_form(name.as_deref(), color);
            }
        }
        let key = format!("screens{screens:?}{:?}", self.chosen.borrow());
        if self.shown.borrow().as_deref() == Some(key.as_str()) {
            return;
        }
        self.shown.replace(Some(key));
        ui::clear(&self.list);
        let identify = ui::button(
            "Show the numbers",
            Some("dc-identify-symbolic"),
            ButtonKind::Normal,
        );
        identify.set_halign(gtk::Align::Start);
        identify.set_action_name(Some("app.identify"));
        identify.set_action_target_value(Some(&0_i32.to_variant()));
        self.list.append(&identify);
        if screens.is_empty() {
            self.list.append(&ui::label(
                "DeskCheck cannot read the screens: is this a GNOME session?",
                &["dc-small"],
            ));
        }
        let mut css = String::new();
        let chosen = self.chosen.borrow().clone();
        for (number, id, name, technical, color) in screens {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .css_classes(["dc-wizard-device"])
                .build();
            if chosen == Some(Chosen::Screen(id.clone())) {
                row.add_css_class("selected");
            }
            let badge = gtk::Label::builder()
                .label(number.to_string())
                .css_classes(["dc-panel-badge"])
                .width_request(26)
                .height_request(26)
                .valign(gtk::Align::Center)
                .build();
            badge.set_widget_name(&format!("wizard-screen-{number}"));
            let _ = writeln!(
                css,
                "#wizard-screen-{number} {{ background-color: {color}; }}"
            );
            row.append(&badge);
            let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
            texts.set_hexpand(true);
            texts.append(&ui::label(&name, &["dc-wizard-device-title", "named"]));
            let technical = ui::label(&technical, &["dc-mono"]);
            technical.set_wrap(false);
            texts.append(&technical);
            row.append(&texts);
            let click = gtk::GestureClick::new();
            let weak = Rc::downgrade(self);
            click.connect_released(move |_, _, _, _| {
                if let Some(wizard) = weak.upgrade() {
                    wizard.choose(&Chosen::Screen(id.clone()));
                }
            });
            row.add_controller(click);
            self.list.append(&row);
        }
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        self.css.load_from_data(&css);
    }

    /// The name and the color of a screen.
    fn fill_screen_form(self: &Rc<Self>, name: Option<&str>, color: Option<Color>) {
        self.entry.set_text(name.unwrap_or(""));
        self.fill_chips(Step::Screens);
        ui::clear(&self.picker);
        let label = ui::label("Color", &["dc-name", "dc-bold"]);
        label.set_width_request(70);
        self.picker.append(&label);
        let mut leader: Option<gtk::ToggleButton> = None;
        let mut css = String::new();
        for (at, (swatch, text)) in SWATCHES.iter().enumerate() {
            let button = gtk::ToggleButton::builder()
                .tooltip_text(*text)
                .active(color == Some(*swatch))
                .css_classes(["dc-swatch-pick"])
                .width_request(36)
                .height_request(36)
                .build();
            button.set_widget_name(&format!("wizard-swatch-{at}"));
            let _ = writeln!(css, "#wizard-swatch-{at} {{ background-color: {swatch}; }}");
            if let Some(first) = &leader {
                button.set_group(Some(first));
            } else {
                leader = Some(button.clone());
            }
            let weak = Rc::downgrade(self);
            let swatch = *swatch;
            button.connect_toggled(move |button| {
                if button.is_active() {
                    if let Some(wizard) = weak.upgrade() {
                        wizard.save_color(swatch);
                    }
                }
            });
            self.picker.append(&button);
        }
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        self.swatch_css.load_from_data(&css);
    }

    /// The last step: keep the place where you are.
    fn show_place(self: &Rc<Self>) {
        self.form.set_visible(true);
        self.place_box.set_visible(true);
        self.follows.set_visible(false);
        ui::clear(&self.list);
        ui::clear(&self.picker);
        self.fill_chips(Step::Places);
        let latest = self.latest.borrow();
        let preview = places_model::new_place(
            String::new(),
            latest.sound.as_ref(),
            latest.screens.as_ref(),
            latest.wifi.as_deref(),
        );
        let mut lines = Vec::new();
        if let Some(condition) = preview.conditions.first() {
            lines.push(format!(
                "Recognized when {}.",
                places_model::because(&latest.config, condition)
            ));
        }
        if let Some(state) = latest.sound.as_ref() {
            let names: Vec<String> = [
                preview.outputs.first().map(|id| {
                    format!(
                        "Sound: {}",
                        places_model::device_name(&latest.config, Some(state), id)
                    )
                }),
                preview.inputs.first().map(|id| {
                    format!(
                        "Microphone: {}",
                        places_model::device_name(&latest.config, Some(state), id)
                    )
                }),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !names.is_empty() {
                lines.push(format!("{}.", names.join(" · ")));
            }
        }
        lines.push(format!(
            "Screens: {}.",
            places_model::layout_words(preview.layout.as_ref()).replace('\n', ", ")
        ));
        let known = latest.config.places().len();
        drop(latest);
        if known > 0 {
            lines.push(format!(
                "You have {known} place(s) already: this one is added next to them."
            ));
        }
        self.place_summary.set_label(&lines.join("\n"));
        if self.entry.text().is_empty() {
            self.entry.set_text("Office");
        }
    }

    /// Saves the place named in the entry.
    fn add_place(self: &Rc<Self>) {
        let latest = self.latest.borrow();
        let name = self.entry.text().trim().to_owned();
        let name = if name.is_empty() {
            places_model::unique_name(latest.config.places(), "New place")
        } else {
            places_model::unique_name(latest.config.places(), &name)
        };
        let place = places_model::new_place(
            name.clone(),
            latest.sound.as_ref(),
            latest.screens.as_ref(),
            latest.wifi.as_deref(),
        );
        drop(latest);
        edit(&self.backend, move |config| config.places_mut().push(place));
        glib::g_debug!("deskcheck", "assistant: place {name} saved");
        self.entry.set_text("");
        self.go(self.step.get().next());
    }

    /// Saves the name in the entry, for the device or the screen chosen.
    fn save(&self) {
        let Some(chosen) = self.chosen.borrow().clone() else {
            return;
        };
        if self.step.get() == Step::Places {
            return;
        }
        let text = self.entry.text().trim().to_owned();
        let latest = self.latest.borrow();
        let unchanged = match &chosen {
            Chosen::Device(id) => {
                latest.config.audio_name(id) == Some(text.as_str())
                    || (text.is_empty() && latest.config.audio_name(id).is_none())
            }
            Chosen::Screen(id) => {
                latest.config.screen_name(id) == Some(text.as_str())
                    || (text.is_empty() && latest.config.screen_name(id).is_none())
            }
        };
        drop(latest);
        if unchanged {
            return;
        }
        let name = (!text.is_empty()).then_some(text);
        edit(&self.backend, move |config| match &chosen {
            Chosen::Device(id) => config.set_audio_name(id, name),
            Chosen::Screen(id) => config.set_screen_name(id, name),
        });
    }

    fn save_icon(&self, icon: DeviceIcon) {
        if let Some(Chosen::Device(id)) = self.chosen.borrow().clone() {
            if self.latest.borrow().config.audio_icon(&id) != Some(icon) {
                edit(&self.backend, move |config| {
                    config.set_audio_icon(&id, Some(icon));
                });
            }
        }
    }

    fn save_color(&self, color: Color) {
        if let Some(Chosen::Screen(id)) = self.chosen.borrow().clone() {
            edit(&self.backend, move |config| {
                config.set_screen_color(&id, Some(color));
            });
        }
    }

    /// A new peak of a microphone: its meter moves, and the one that hears
    /// the voice is chosen, until the user clicks one.
    pub fn peak(self: &Rc<Self>, source: &DeviceId, peak: f32) {
        if !self.window.is_visible() || self.step.get() != Step::Microphones {
            return;
        }
        let hearing = {
            let mut mics = self.mics.borrow_mut();
            let Some(row) = mics.get_mut(source) else {
                return;
            };
            let fraction = row.level.update(peak);
            row.meter.set_fraction(fraction);
            if to_db(peak) > VOICE_DB {
                row.heard_at = Some(Instant::now());
            }
            let levels: Vec<(DeviceId, f32, bool)> = mics
                .iter()
                .map(|(id, row)| {
                    (
                        id.clone(),
                        row.level.fraction(),
                        row.heard_at.is_some_and(|at| at.elapsed() < VOICE_FRESH),
                    )
                })
                .collect();
            let hearing = wizard_model::hearing(&levels);
            for (id, row) in mics.iter() {
                let hears = hearing.as_ref() == Some(id);
                if hears {
                    row.row.add_css_class("hearing");
                    row.title.set_label("This one hears you");
                } else {
                    row.row.remove_css_class("hearing");
                    row.title.set_label(&row.name);
                }
            }
            hearing
        };
        if let Some(id) = hearing {
            if !self.manual.get()
                && self.chosen.borrow().as_ref() != Some(&Chosen::Device(id.clone()))
            {
                self.choose(&Chosen::Device(id));
            }
        }
    }
}

/// Changes the settings file, then asks to show everything again.
fn edit(backend: &Backend, change: impl FnOnce(&mut Config)) {
    let saved = Store::new().and_then(|store| {
        let mut config = store.load()?;
        change(&mut config);
        store.save(&config)
    });
    match saved {
        Ok(()) => backend.request(Request::Refresh),
        Err(error) => glib::g_warning!("deskcheck", "the setting could not be saved: {error}"),
    }
}
