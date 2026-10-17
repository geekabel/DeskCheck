//! The Audio page: every output, every microphone with the monitors apart,
//! and every app, as in the design. Each device has a Test button and a
//! Rename button.
//!
//! The tabs are in the header bar, as in the mockup: the window puts
//! [`AudioPage::tabs`] there while the page is shown.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use adw::prelude::*;
use deskcheck_config::{Config, Store};
use deskcheck_core::audio::level::{LevelBar, to_db};
use deskcheck_core::audio::{AudioState, DeviceId, Target};

use crate::audio_model::{
    self, AudioTab, DeviceRow, Hearing, Hold, Monitors, StreamKind, StreamRow,
};
use crate::backend::{Backend, Request};
use crate::ui::{self, ButtonKind, TagKind, TileKind};

/// Blocks of the level meters of the rows, as in the design.
const ROW_BLOCKS: usize = 14;

/// Width of the tag after a meter, so the buttons line up.
const TAG_WIDTH: i32 = 132;

/// Width of the end of an app row: its tag and its switch.
const APP_END_WIDTH: i32 = 330;

/// Names offered when renaming, as in the naming mockup.
const OUTPUT_NAMES: [&str; 4] = ["Headset", "Laptop speakers", "Office screen", "Home screen"];
const MICROPHONE_NAMES: [&str; 4] = ["Headset mic", "Laptop mic", "Webcam mic", "Office mic"];

/// What a row meter measures, which picks the words of its tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Measured {
    /// A microphone: does it hear your voice?
    Microphone,
    /// The monitor of an output: does it play?
    Output,
}

/// The meter and the tag of a row, moved by the peaks.
struct RowLevel {
    meter: ui::Meter,
    tag: gtk::Label,
    measured: Measured,
    muted: bool,
    bar: LevelBar,
    shown: Option<Hearing>,
}

/// One tab: a page with a title, a tip, and content built again at each
/// change.
struct TabPage {
    root: gtk::ScrolledWindow,
    content: gtk::Box,
}

impl TabPage {
    fn new(tab: AudioTab) -> Self {
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
            .spacing(12)
            .build();
        let (title, _, _) = ui::page_title(tab.title(), tab.subtitle());
        top.append(&title);
        if tab != AudioTab::Apps {
            let name_new = ui::button(
                "Name a new device",
                Some("dc-add-symbolic"),
                ButtonKind::Normal,
            );
            name_new.set_valign(gtk::Align::End);
            name_new.set_sensitive(false);
            name_new.set_tooltip_text(Some("Comes with the first-run assistant"));
            top.append(&name_new);
        }
        page.append(&top);

        let tip = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["dc-info"])
            .build();
        tip.append(&ui::icon(tab.tip_icon(), 18));
        let (bold, rest) = tab.tip();
        let text = gtk::Label::builder()
            .use_markup(true)
            .label(format!(
                "<b>{}</b> {}",
                glib_escape(bold),
                glib_escape(rest)
            ))
            .xalign(0.0)
            .wrap(true)
            .build();
        tip.append(&text);
        page.append(&tip);

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .build();
        page.append(&content);

        if tab != AudioTab::Apps {
            // As in the design; these come in a later version.
            let bottom = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .vexpand(true)
                .valign(gtk::Align::End)
                .build();
            for text in [
                "Advanced options: sound card profiles, ports",
                "Hidden devices (0)",
            ] {
                let button = ui::button(text, None, ButtonKind::Flat);
                button.set_sensitive(false);
                button.set_tooltip_text(Some("Comes in a later version"));
                bottom.append(&button);
            }
            page.append(&bottom);
        }

        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&page)
            .vexpand(true)
            .build();
        Self { root, content }
    }
}

/// Escapes a text for Pango markup.
fn glib_escape(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}

/// The Audio page.
pub struct AudioPage {
    /// The pages of the three tabs, to put in the window.
    pub root: gtk::Stack,
    /// The tabs, to put in the header bar.
    pub tabs: gtk::Box,
    tab_buttons: Vec<(AudioTab, gtk::ToggleButton)>,
    outputs: TabPage,
    microphones: TabPage,
    apps: TabPage,
    backend: Backend,
    levels: RefCell<HashMap<DeviceId, RowLevel>>,
    holds: RefCell<HashMap<DeviceId, Hold>>,
    last: RefCell<Option<(Config, AudioState)>>,
    monitors_hidden: Cell<bool>,
}

impl AudioPage {
    /// Builds the page, on the Outputs tab.
    pub fn new(backend: Backend) -> Rc<Self> {
        let root = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        let tabs = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(2)
            .css_classes(["dc-seg"])
            .valign(gtk::Align::Center)
            .build();
        let outputs = TabPage::new(AudioTab::Outputs);
        let microphones = TabPage::new(AudioTab::Microphones);
        let apps = TabPage::new(AudioTab::Apps);
        let mut tab_buttons: Vec<(AudioTab, gtk::ToggleButton)> = Vec::new();
        for (tab, page) in AudioTab::ALL
            .into_iter()
            .zip([&outputs, &microphones, &apps])
        {
            root.add_named(&page.root, Some(tab.id()));
            let button = gtk::ToggleButton::builder().label(tab.title()).build();
            if let Some((_, first)) = tab_buttons.first() {
                button.set_group(Some(first));
            }
            tabs.append(&button);
            tab_buttons.push((tab, button));
        }
        let page = Rc::new(Self {
            root,
            tabs,
            tab_buttons,
            outputs,
            microphones,
            apps,
            backend,
            levels: RefCell::default(),
            holds: RefCell::default(),
            last: RefCell::default(),
            monitors_hidden: Cell::new(false),
        });
        for (tab, button) in &page.tab_buttons {
            let page = Rc::clone(&page);
            let tab = *tab;
            button.connect_toggled(move |button| {
                if button.is_active() {
                    page.root.set_visible_child_name(tab.id());
                }
            });
        }
        page.show_tab(AudioTab::Outputs);
        page
    }

    /// Shows a tab.
    pub fn show_tab(&self, tab: AudioTab) {
        if let Some((_, button)) = self.tab_buttons.iter().find(|(t, _)| *t == tab) {
            button.set_active(true);
        }
        self.root.set_visible_child_name(tab.id());
    }

    /// Shows the sound: builds the three tabs again.
    pub fn show_sound(
        self: &Rc<Self>,
        config: &Config,
        sound: Option<&Result<AudioState, String>>,
    ) {
        let state = match sound {
            Some(Ok(state)) => Some(state),
            _ => None,
        };
        for (tab, button) in &self.tab_buttons {
            button.set_label(&audio_model::tab_label(*tab, state));
        }
        self.levels.borrow_mut().clear();
        for page in [&self.outputs, &self.microphones, &self.apps] {
            ui::clear(&page.content);
        }
        let Some(state) = state else {
            let text = if sound.is_none() {
                "Reading the sound…"
            } else {
                "No sound server answers. Is PulseAudio or PipeWire running?"
            };
            for page in [&self.outputs, &self.microphones, &self.apps] {
                page.content.append(&ui::label(text, &["dc-small"]));
            }
            *self.last.borrow_mut() = None;
            return;
        };
        *self.last.borrow_mut() = Some((config.clone(), state.clone()));
        self.build_outputs(config, state);
        self.build_microphones(config, state);
        self.build_apps(config, state);
    }

    /// Builds the Microphones tab again, from the last state.
    fn rebuild_microphones(self: &Rc<Self>) {
        let last = self.last.borrow().clone();
        if let Some((config, state)) = last {
            self.levels
                .borrow_mut()
                .retain(|_, level| level.measured != Measured::Microphone);
            ui::clear(&self.microphones.content);
            self.build_microphones(&config, &state);
        }
    }

    fn build_outputs(self: &Rc<Self>, config: &Config, state: &AudioState) {
        let rows = audio_model::output_rows(config, state);
        if rows.is_empty() {
            self.outputs.content.append(&ui::label(
                "No output: is a sound card plugged in?",
                &["dc-small"],
            ));
            return;
        }
        let list = list_card();
        let mut group: Option<gtk::CheckButton> = None;
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                list.append(&ui::line());
            }
            let monitor = audio_model::monitor_of(state, &row.id).cloned();
            let line = self.device_line(row, Target::Output(row.id.clone()), &mut group);
            let level = self.level_widgets(&line, row.muted, Measured::Output, monitor.as_ref());
            if let Some(monitor) = monitor {
                self.levels.borrow_mut().insert(monitor, level);
            }
            let test = ui::button("Test", Some("dc-play-symbolic"), ButtonKind::Normal);
            test.set_tooltip_text(Some("Plays a short chime on this output"));
            {
                let backend = self.backend.clone();
                let id = row.id.clone();
                test.connect_clicked(move |button| {
                    backend.request(Request::TestOutput(id.clone()));
                    ui::toast(button, "Playing a test sound…");
                });
            }
            line.append(&test);
            line.append(&self.rename_button(row, &Target::Output(row.id.clone())));
            list.append(&line);
        }
        self.outputs.content.append(&list);
    }

    fn build_microphones(self: &Rc<Self>, config: &Config, state: &AudioState) {
        let rows = audio_model::microphone_rows(config, state);
        if rows.is_empty() {
            self.microphones
                .content
                .append(&ui::label("No microphone is plugged in.", &["dc-small"]));
        } else {
            let list = list_card();
            let mut group: Option<gtk::CheckButton> = None;
            for (index, row) in rows.iter().enumerate() {
                if index > 0 {
                    list.append(&ui::line());
                }
                let line = self.device_line(row, Target::Input(row.id.clone()), &mut group);
                let level =
                    self.level_widgets(&line, row.muted, Measured::Microphone, Some(&row.id));
                self.levels.borrow_mut().insert(row.id.clone(), level);
                let test = ui::button("Test", Some("dc-record-symbolic"), ButtonKind::Normal);
                test.set_tooltip_text(Some(
                    "Records 3 seconds, then plays them on the default output",
                ));
                let output = state.default_output.clone();
                test.set_sensitive(output.is_some());
                {
                    let backend = self.backend.clone();
                    let input = row.id.clone();
                    test.connect_clicked(move |button| {
                        if let Some(output) = output.clone() {
                            backend.request(Request::TestInput {
                                input: input.clone(),
                                output,
                            });
                            ui::toast(button, "Recording 3 seconds: speak now.");
                        }
                    });
                }
                line.append(&test);
                line.append(&self.rename_button(row, &Target::Input(row.id.clone())));
                list.append(&line);
            }
            self.microphones.content.append(&list);
        }
        let monitors = audio_model::monitors(config, state);
        if monitors.count > 0 {
            self.microphones
                .content
                .append(&self.monitors_card(&monitors));
        }
    }

    /// The card of the monitors: what they are, those an app records with a
    /// switch to a real microphone, and the others in one line.
    fn monitors_card(self: &Rc<Self>, monitors: &Monitors) -> gtk::Box {
        let card = list_card();
        let head = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["dc-device-row"])
            .build();
        let icon = ui::icon("dc-switch-symbolic", 18);
        icon.add_css_class("dc-dim-icon");
        head.append(&icon);
        let texts = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();
        texts.append(&ui::label(
            &audio_model::monitors_title(monitors.count),
            &["dc-row-name"],
        ));
        texts.append(&ui::label(
            "They copy the sound that comes out of an output. Useful to record the screen. \
             In a call, nobody hears you.",
            &["dc-small"],
        ));
        head.append(&texts);
        let hidden = self.monitors_hidden.get();
        let toggle = ui::button(if hidden { "Show" } else { "Hide" }, None, ButtonKind::Flat);
        {
            let page = Rc::clone(self);
            toggle.connect_clicked(move |_| {
                page.monitors_hidden.set(!page.monitors_hidden.get());
                page.rebuild_microphones();
            });
        }
        head.append(&toggle);
        card.append(&head);
        if hidden {
            return card;
        }
        for row in &monitors.recorded {
            card.append(&ui::line());
            let line = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(14)
                .css_classes(["dc-device-row", "warn"])
                .build();
            let tile = ui::tile(40);
            ui::set_tile(&tile, row.icon, TileKind::Warn);
            line.append(&tile);
            line.append(&name_block(&row.name, None, &row.technical, true));
            line.append(&ui::icon_tag(
                "dc-warning-symbolic",
                &audio_model::listeners_tag(&row.apps),
                TagKind::WarnStrong,
            ));
            if let Some((mic, mic_name)) = &monitors.switch_to {
                let switch = ui::button(&format!("Switch to {mic_name}"), None, ButtonKind::Warn);
                let backend = self.backend.clone();
                let streams = row.streams.clone();
                let mic = mic.clone();
                switch.connect_clicked(move |_| {
                    for stream in &streams {
                        backend.request(Request::MoveRecording {
                            stream: *stream,
                            to: mic.clone(),
                        });
                    }
                });
                line.append(&switch);
            }
            card.append(&line);
        }
        card.append(&ui::line());
        let note = ui::label(&audio_model::monitors_note(monitors), &["dc-list-note"]);
        card.append(&note);
        card
    }

    fn build_apps(self: &Rc<Self>, config: &Config, state: &AudioState) {
        let rows = audio_model::stream_rows(state);
        if rows.is_empty() {
            self.apps.content.append(&ui::label(
                "No app uses the sound right now.",
                &["dc-small"],
            ));
            return;
        }
        let list = list_card();
        let switch_to = audio_model::monitors(config, state).switch_to;
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                list.append(&ui::line());
            }
            list.append(&self.app_line(config, state, row, switch_to.as_ref()));
        }
        self.apps.content.append(&list);
    }

    /// A row of the Apps tab: the app, the device it uses (to change), and
    /// a switch when it records a monitor.
    fn app_line(
        &self,
        config: &Config,
        state: &AudioState,
        row: &StreamRow,
        switch_to: Option<&(DeviceId, String)>,
    ) -> gtk::Box {
        let classes: &[&str] = if row.records_monitor {
            &["dc-device-row", "warn"]
        } else {
            &["dc-device-row"]
        };
        let line = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(14)
            .css_classes(classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>())
            .build();
        let row_line = line.clone();
        let tile = ui::tile(40);
        ui::set_tile(&tile, row.icon, TileKind::Neutral);
        line.append(&tile);
        let what = match row.kind {
            StreamKind::Playback => "Plays sound",
            StreamKind::Recording => "Listens",
        };
        line.append(&name_block(&row.app, None, what, false));
        line.append(&ui::row_label(match row.kind {
            StreamKind::Playback => "Sound",
            StreamKind::Recording => "Microphone",
        }));
        let choices = audio_model::stream_choices(config, state, row);
        let names: Vec<&str> = choices.iter().map(|(_, name)| name.as_str()).collect();
        let picker = gtk::DropDown::from_strings(&names);
        picker.set_width_request(220);
        picker.set_valign(gtk::Align::Center);
        if let Some(at) = choices.iter().position(|(id, _)| *id == row.device) {
            picker.set_selected(u32::try_from(at).unwrap_or(0));
        }
        {
            let backend = self.backend.clone();
            let kind = row.kind;
            let stream = row.stream;
            let current = row.device.clone();
            picker.connect_selected_notify(move |picker| {
                let Some((to, _)) = usize::try_from(picker.selected())
                    .ok()
                    .and_then(|at| choices.get(at))
                else {
                    return;
                };
                if *to == current {
                    return;
                }
                let to = to.clone();
                backend.request(match kind {
                    StreamKind::Playback => Request::MovePlayback { stream, to },
                    StreamKind::Recording => Request::MoveRecording { stream, to },
                });
            });
        }
        line.append(&picker);
        // The tag and the switch, in a column of the same width on every
        // row, so the device lists line up.
        let end = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(14)
            .width_request(APP_END_WIDTH)
            .halign(gtk::Align::End)
            .build();
        line.append(&end);
        let line = end;
        if row.records_monitor {
            line.append(&ui::icon_tag(
                "dc-warning-symbolic",
                "Not a microphone",
                TagKind::WarnStrong,
            ));
            if let Some((mic, mic_name)) = switch_to {
                let switch = ui::button(&format!("Switch to {mic_name}"), None, ButtonKind::Warn);
                let backend = self.backend.clone();
                let stream = row.stream;
                let mic = mic.clone();
                switch.connect_clicked(move |_| {
                    backend.request(Request::MoveRecording {
                        stream,
                        to: mic.clone(),
                    });
                });
                line.append(&switch);
            }
        } else {
            let tag = ui::tag("OK", TagKind::Ok);
            tag.set_width_request(TAG_WIDTH);
            line.append(&tag);
        }
        row_line
    }

    /// The start of a device row: the radio to make it the default, its
    /// tile, its names.
    fn device_line(
        &self,
        row: &DeviceRow,
        target: Target,
        group: &mut Option<gtk::CheckButton>,
    ) -> gtk::Box {
        let classes: &[&str] = if row.default {
            &["dc-device-row", "selected"]
        } else {
            &["dc-device-row"]
        };
        let line = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(14)
            .css_classes(classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>())
            .build();
        let radio = gtk::CheckButton::builder()
            .active(row.default)
            .valign(gtk::Align::Center)
            .tooltip_text("Make it the default")
            .build();
        // Every radio joins a leader that is never shown: GTK draws a check
        // button as a radio only in a group, even when the list has one row.
        let leader = group.get_or_insert_with(gtk::CheckButton::new);
        radio.set_group(Some(leader));
        {
            let backend = self.backend.clone();
            radio.connect_toggled(move |radio| {
                if radio.is_active() {
                    backend.request(Request::SetDefault(target.clone()));
                }
            });
        }
        line.append(&radio);
        let tile = ui::tile(40);
        let kind = if row.default {
            TileKind::Accent
        } else {
            TileKind::Neutral
        };
        ui::set_tile(&tile, row.icon, kind);
        line.append(&tile);
        let badge = row.default.then_some("Default");
        line.append(&name_block(&row.name, badge, &row.technical, true));
        line
    }

    /// Adds the meter and the tag of a row.
    fn level_widgets(
        &self,
        line: &gtk::Box,
        muted: bool,
        measured: Measured,
        source: Option<&DeviceId>,
    ) -> RowLevel {
        let meter = ui::Meter::new(ROW_BLOCKS);
        line.append(&meter.root);
        let tag = ui::tag("", TagKind::Neutral);
        tag.set_width_request(TAG_WIDTH);
        line.append(&tag);
        let mut level = RowLevel {
            meter,
            tag,
            measured,
            muted,
            bar: LevelBar::default(),
            shown: None,
        };
        // A row built again keeps what its source heard just before.
        let hearing = source
            .and_then(|source| {
                self.holds
                    .borrow()
                    .get(source)
                    .map(|hold| hold.hearing(Instant::now()))
            })
            .unwrap_or(Hearing::Silent);
        show_hearing(&mut level, hearing);
        level
    }

    /// The Rename button of a row, with its small form.
    fn rename_button(&self, row: &DeviceRow, target: &Target) -> gtk::MenuButton {
        let form = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .css_classes(["dc-rename"])
            .width_request(320)
            .build();
        form.append(&ui::label("Its name", &["dc-name"]));
        let entry = gtk::Entry::builder()
            .text(&row.name)
            .activates_default(false)
            .build();
        form.append(&entry);
        let suggestions = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(6)
            .row_spacing(6)
            .max_children_per_line(2)
            .build();
        let names = match target {
            Target::Output(_) => OUTPUT_NAMES,
            Target::Input(_) => MICROPHONE_NAMES,
        };
        for name in names {
            let chip = ui::button(name, None, ButtonKind::Normal);
            chip.add_css_class("dc-compact");
            let entry = entry.clone();
            chip.connect_clicked(move |_| entry.set_text(name));
            suggestions.insert(&chip, -1);
        }
        form.append(&suggestions);
        let buttons = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk::Align::End)
            .build();
        let reset = ui::button("Use the system name", None, ButtonKind::Flat);
        let save = ui::button("Save", None, ButtonKind::Primary);
        buttons.append(&reset);
        buttons.append(&save);
        form.append(&buttons);

        let popover = gtk::Popover::builder().child(&form).build();
        let button = gtk::MenuButton::builder()
            .icon_name("dc-rename-symbolic")
            .popover(&popover)
            .tooltip_text(format!("Rename {}", row.name))
            .valign(gtk::Align::Center)
            .css_classes(["dc-btn", "dc-icon-only"])
            .build();

        let id = row.id.clone();
        let backend = self.backend.clone();
        let rename = Rc::new(move |name: Option<String>, from: &gtk::Popover| {
            from.popdown();
            match save_name(&id, name) {
                Ok(()) => backend.request(Request::Refresh),
                Err(error) => ui::toast(from, &format!("The name could not be saved: {error}")),
            }
        });
        {
            let rename = Rc::clone(&rename);
            let entry = entry.clone();
            let popover = popover.clone();
            save.connect_clicked(move |_| rename(Some(entry.text().to_string()), &popover));
        }
        {
            let rename = Rc::clone(&rename);
            let popover = popover.clone();
            entry.connect_activate(move |entry| rename(Some(entry.text().to_string()), &popover));
        }
        {
            let popover = popover.clone();
            reset.connect_clicked(move |_| rename(None, &popover));
        }
        button
    }

    /// Moves the meter and the tag of a row.
    pub fn peak(&self, source: &DeviceId, peak: f32) {
        let mut levels = self.levels.borrow_mut();
        let Some(level) = levels.get_mut(source) else {
            return;
        };
        let now = Instant::now();
        let mut holds = self.holds.borrow_mut();
        let hold = holds.entry(source.clone()).or_default();
        hold.update(to_db(peak), now);
        let fraction = level.bar.update(peak);
        level.meter.set_fraction(fraction);
        show_hearing(level, hold.hearing(now));
    }
}

/// A card that holds rows, with no padding: the rows have their own.
fn list_card() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["dc-card"])
        .overflow(gtk::Overflow::Hidden)
        .build()
}

/// The name of a row, maybe with a badge, and the line under it.
fn name_block(name: &str, badge: Option<&str>, under: &str, technical: bool) -> gtk::Box {
    let texts = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = ui::label(name, &["dc-row-name"]);
    title.set_wrap(false);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    top.append(&title);
    if let Some(badge) = badge {
        top.append(&ui::tag(badge, TagKind::Accent));
    }
    texts.append(&top);
    let under = ui::label(under, &[if technical { "dc-mono" } else { "dc-small" }]);
    under.set_wrap(false);
    under.set_ellipsize(gtk::pango::EllipsizeMode::End);
    under.set_tooltip_text(Some(under.label().as_str()));
    texts.append(&under);
    texts
}

/// Shows what a row hears in its tag, when it changed.
fn show_hearing(level: &mut RowLevel, hearing: Hearing) {
    if level.muted {
        if level.shown.is_none() {
            ui::set_tag(&level.tag, "Muted", TagKind::Error);
            level.shown = Some(Hearing::Silent);
        }
        return;
    }
    if level.shown == Some(hearing) {
        return;
    }
    level.shown = Some(hearing);
    let (text, kind) = match (level.measured, hearing) {
        (Measured::Microphone, Hearing::Voice) => (hearing.label(), TagKind::Ok),
        (Measured::Microphone, _) => (hearing.label(), TagKind::Neutral),
        (Measured::Output, Hearing::Silent) => {
            (audio_model::playing_label(hearing), TagKind::Neutral)
        }
        (Measured::Output, _) => (audio_model::playing_label(hearing), TagKind::Ok),
    };
    ui::set_tag(&level.tag, text, kind);
}

/// Saves a name given in the page. `None` or a blank name removes it.
fn save_name(id: &DeviceId, name: Option<String>) -> Result<(), deskcheck_config::Error> {
    let store = Store::new()?;
    let mut config = store.load()?;
    config.set_audio_name(id, name);
    store.save(&config)
}
