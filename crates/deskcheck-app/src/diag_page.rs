//! The Diagnostic page: a list of checks for the problem the user has,
//! each with its state and, when DeskCheck can, a fix in one click, as in
//! the design. What the checks say comes from [`crate::diag_model`].
//!
//! The list is built again when the state changes or the user answers.
//! The level of the microphone moves in place, 25 times a second.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Instant;

use adw::prelude::*;
use deskcheck_config::Config;
use deskcheck_core::audio::level::{LevelBar, to_db};
use deskcheck_core::audio::{AudioState, DeviceId, best_microphone};
use deskcheck_core::display::DisplayState;
use gtk::glib;

use crate::audio_model::{AudioTab, VOICE_DB};
use crate::backend::{Backend, Request};
use crate::diag_model::{self, Act, Answers, Check, Look, Scenario, Status};
use crate::ui::{self, ButtonKind};

/// Blocks of the level meter, as in the design.
const METER_BLOCKS: usize = 12;

/// Where a button of the page sends the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Go {
    /// The Now page: the diagnosis is done.
    Now,
    /// A tab of the Audio page.
    Audio(AudioTab),
}

/// What the Go buttons call.
type GoHandler = Box<dyn Fn(Go)>;

/// The last state the page received.
#[derive(Default)]
struct Latest {
    config: Config,
    sound: Option<AudioState>,
    screens: Option<Result<DisplayState, String>>,
}

/// The microphone the page listens to, for the voice check.
#[derive(Default)]
struct Listening {
    source: Option<DeviceId>,
    bar: LevelBar,
    heard_at: Option<Instant>,
    meter: Option<ui::Meter>,
    label: Option<gtk::Label>,
}

/// The Diagnostic page.
pub struct DiagPage {
    /// The page, to put in the window.
    pub root: gtk::ScrolledWindow,
    backend: Backend,
    title: gtk::Label,
    subtitle: gtk::Label,
    others: gtk::Box,
    progress: gtk::ProgressBar,
    progress_text: gtk::Label,
    list: gtk::Box,
    scenario: Cell<Scenario>,
    answers: RefCell<Answers>,
    latest: RefCell<Latest>,
    listening: RefCell<Listening>,
    on_go: RefCell<Option<GoHandler>>,
}

impl DiagPage {
    /// Builds the page, on "They can't hear me".
    pub fn new(backend: Backend) -> Rc<Self> {
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
            .spacing(8)
            .build();
        let texts = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        texts.append(&ui::label("Diagnostic", &["dc-small"]));
        let (block, title, subtitle) = ui::page_title("", "");
        block.set_margin_top(2);
        texts.append(&block);
        top.append(&texts);
        let others = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .valign(gtk::Align::End)
            .build();
        top.append(&others);
        page.append(&top);

        let progress_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let progress = gtk::ProgressBar::builder()
            .hexpand(true)
            .valign(gtk::Align::Center)
            .css_classes(["dc-progress"])
            .build();
        let progress_text = ui::label("", &["dc-check-detail"]);
        progress_text.set_wrap(false);
        progress_row.append(&progress);
        progress_row.append(&progress_text);
        page.append(&progress_row);

        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["dc-card"])
            .overflow(gtk::Overflow::Hidden)
            .build();
        page.append(&list);

        let bottom = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["dc-card", "dc-bottom-bar"])
            .vexpand(true)
            .valign(gtk::Align::End)
            .build();
        let remember = gtk::CheckButton::builder()
            .label("Remember the fix for this place")
            .hexpand(true)
            .sensitive(false)
            .tooltip_text("Comes with places, in a later version")
            .build();
        bottom.append(&remember);
        let again = ui::button("Start again", None, ButtonKind::Flat);
        let done = ui::button("Done", None, ButtonKind::Primary);
        bottom.append(&again);
        bottom.append(&done);
        page.append(&bottom);

        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&page)
            .vexpand(true)
            .build();
        let page = Rc::new(Self {
            root,
            backend,
            title,
            subtitle,
            others,
            progress,
            progress_text,
            list,
            scenario: Cell::new(Scenario::NotHeard),
            answers: RefCell::default(),
            latest: RefCell::default(),
            listening: RefCell::default(),
            on_go: RefCell::default(),
        });
        {
            let page = Rc::clone(&page);
            again.connect_clicked(move |_| page.start_again());
        }
        {
            let page = Rc::clone(&page);
            done.connect_clicked(move |_| {
                page.start_again();
                page.go(Go::Now);
            });
        }
        page.tick_every_second();
        page.render();
        page
    }

    /// Calls `handler` when a button sends the user to another page.
    pub fn connect_go(&self, handler: impl Fn(Go) + 'static) {
        *self.on_go.borrow_mut() = Some(Box::new(handler));
    }

    fn go(&self, to: Go) {
        if let Some(handler) = self.on_go.borrow().as_ref() {
            handler(to);
        }
    }

    /// Forgets the answers and starts the checks again.
    fn start_again(self: &Rc<Self>) {
        *self.answers.borrow_mut() = Answers::default();
        let mut listening = self.listening.borrow_mut();
        listening.heard_at = None;
        drop(listening);
        self.render();
    }

    /// Updates "You spoke 2 s ago" each second, without building again.
    fn tick_every_second(self: &Rc<Self>) {
        let weak: Weak<Self> = Rc::downgrade(self);
        glib::timeout_add_seconds_local(1, move || {
            let Some(page) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let listening = page.listening.borrow();
            if let (Some(label), Some(at)) = (&listening.label, listening.heard_at) {
                label.set_label(&diag_model::spoke_ago(at.elapsed()));
            }
            glib::ControlFlow::Continue
        });
    }

    /// Takes the new state, and shows the checks again.
    pub fn show(
        self: &Rc<Self>,
        config: &Config,
        sound: Option<&Result<AudioState, String>>,
        screens: Option<&Result<DisplayState, String>>,
    ) {
        {
            let mut latest = self.latest.borrow_mut();
            latest.config = config.clone();
            latest.sound = sound.and_then(|s| s.as_ref().ok()).cloned();
            latest.screens = screens.cloned();
        }
        self.render();
    }

    /// Shows a scenario.
    fn show_scenario(self: &Rc<Self>, scenario: Scenario) {
        self.scenario.set(scenario);
        self.start_again();
    }

    /// Builds the page again from the state and the answers.
    fn render(self: &Rc<Self>) {
        let scenario = self.scenario.get();
        let latest = self.latest.borrow();
        let mut answers = *self.answers.borrow();
        {
            let mut listening = self.listening.borrow_mut();
            let source = latest
                .sound
                .as_ref()
                .and_then(best_microphone)
                .map(|mic| mic.id.clone());
            if source != listening.source {
                listening.source = source;
                listening.heard_at = None;
            }
            answers.voice_ago = listening.heard_at.map(|at| at.elapsed());
            listening.meter = None;
            listening.label = None;
        }
        let checks = diag_model::checks(
            scenario,
            &latest.config,
            latest.sound.as_ref(),
            latest.screens.as_ref(),
            &answers,
        );
        drop(latest);

        self.title.set_label(scenario.title());
        self.subtitle.set_label(&scenario.subtitle(checks.len()));
        ui::clear(&self.others);
        for other in Scenario::ALL.into_iter().filter(|s| *s != scenario) {
            let button = ui::button(other.title(), None, ButtonKind::Flat);
            let page = Rc::clone(self);
            button.connect_clicked(move |_| page.show_scenario(other));
            self.others.append(&button);
        }
        let (fraction, text) = diag_model::progress(&checks);
        self.progress.set_fraction(fraction);
        self.progress_text.set_label(&text);

        ui::clear(&self.list);
        for (index, check) in checks.iter().enumerate() {
            if index > 0 {
                self.list.append(&ui::line());
            }
            self.list.append(&self.check_row(index + 1, check));
        }
    }

    /// A row of the list: the state, what is checked, what DeskCheck found,
    /// and the buttons.
    fn check_row(self: &Rc<Self>, number: usize, check: &Check) -> gtk::Box {
        let classes: &[&str] = if check.status == Status::Problem {
            &["dc-check-row", "warn"]
        } else {
            &["dc-check-row"]
        };
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(14)
            .css_classes(classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>())
            .build();
        row.append(&status_icon(check.status));

        let texts = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(3)
            .hexpand(true)
            .build();
        let mut title_classes = vec!["dc-check-title"];
        match check.status {
            Status::Problem => title_classes.push("warn"),
            Status::Locked => title_classes.push("locked"),
            _ => {}
        }
        texts.append(&ui::label(&check.heading(number), &title_classes));
        let detail = ui::label(&check.detail, &["dc-check-detail"]);
        if check.meter {
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            detail.set_hexpand(false);
            line.append(&detail);
            let meter = ui::Meter::new(METER_BLOCKS);
            line.append(&meter.root);
            texts.append(&line);
            let mut listening = self.listening.borrow_mut();
            listening.meter = Some(meter);
            if check.status == Status::Ok {
                listening.label = Some(detail);
            }
        } else {
            texts.append(&detail);
        }

        let (inline, at_end): (Vec<_>, Vec<_>) = check
            .buttons
            .iter()
            .partition(|button| button.act != Act::RecordTest);
        if !inline.is_empty() || check.note.is_some() {
            let buttons = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(10)
                .margin_top(7)
                .build();
            for button in inline {
                buttons.append(&self.action_button(button, check.status));
            }
            if let Some(note) = &check.note {
                let note = ui::label(note, &["dc-small"]);
                note.set_wrap(false);
                buttons.append(&note);
            }
            texts.append(&buttons);
        }
        if let Some(technical) = &check.technical {
            let line = ui::label(technical, &["dc-mono"]);
            line.set_wrap(false);
            line.set_ellipsize(gtk::pango::EllipsizeMode::End);
            line.set_margin_top(5);
            texts.append(&line);
        }
        row.append(&texts);
        for button in at_end {
            row.append(&self.action_button(button, check.status));
        }
        row
    }

    /// A button of a check, connected to what it does.
    fn action_button(self: &Rc<Self>, button: &diag_model::Button, status: Status) -> gtk::Button {
        let kind = match button.look {
            Look::Normal => ButtonKind::Normal,
            Look::Fix => ButtonKind::Warn,
        };
        let widget = ui::button(&button.label, None, kind);
        widget.set_valign(gtk::Align::Center);
        widget.set_sensitive(status != Status::Locked);
        let page = Rc::clone(self);
        let act = button.act.clone();
        widget.connect_clicked(move |widget| page.act(&act, widget));
        widget
    }

    /// Does what a button says.
    fn act(self: &Rc<Self>, act: &Act, from: &gtk::Button) {
        match act {
            Act::Fix(fixes) => {
                for fix in fixes {
                    self.backend.request(Request::Fix(fix.clone()));
                }
            }
            Act::MovePlaybacks(moves) => {
                for (stream, to) in moves {
                    self.backend.request(Request::MovePlayback {
                        stream: *stream,
                        to: to.clone(),
                    });
                }
            }
            Act::Answer(question, yes) => {
                self.answers.borrow_mut().answer(*question, *yes);
                self.render();
            }
            Act::Ask(question) => {
                self.answers.borrow_mut().ask(*question);
                self.render();
            }
            Act::RecordTest => {
                let devices = {
                    let latest = self.latest.borrow();
                    latest.sound.as_ref().and_then(|sound| {
                        Some((
                            best_microphone(sound)?.id.clone(),
                            sound.default_output.clone()?,
                        ))
                    })
                };
                if let Some((input, output)) = devices {
                    self.backend.request(Request::TestInput { input, output });
                    ui::toast(from, "Recording 3 seconds: speak now.");
                    self.answers.borrow_mut().recorded = true;
                    self.render();
                }
            }
            Act::PlayChime => {
                let output = self
                    .latest
                    .borrow()
                    .sound
                    .as_ref()
                    .and_then(|sound| sound.default_output.clone());
                if let Some(output) = output {
                    self.backend.request(Request::TestOutput(output));
                    self.answers.borrow_mut().chime_played = true;
                    self.render();
                }
            }
            Act::OpenAudio(tab) => self.go(Go::Audio(*tab)),
        }
    }

    /// Moves the meter of the voice check, and opens it when a voice is
    /// heard.
    pub fn peak(self: &Rc<Self>, source: &DeviceId, peak: f32) {
        let first_voice = {
            let mut listening = self.listening.borrow_mut();
            if listening.source.as_ref() != Some(source) {
                return;
            }
            let fraction = listening.bar.update(peak);
            if let Some(meter) = &listening.meter {
                meter.set_fraction(fraction);
            }
            if to_db(peak) > VOICE_DB {
                let first = listening.heard_at.is_none();
                listening.heard_at = Some(Instant::now());
                first
            } else {
                false
            }
        };
        if first_voice && self.scenario.get() == Scenario::NotHeard {
            self.render();
        }
    }
}

/// The round icon of a state.
fn status_icon(status: Status) -> gtk::Box {
    let (icon, class) = match status {
        Status::Ok => ("dc-ok-symbolic", "ok"),
        Status::Problem => ("dc-alert-symbolic", "problem"),
        Status::Question => ("dc-question-symbolic", "question"),
        Status::Waiting | Status::Locked => ("dc-dot-symbolic", "waiting"),
    };
    // Homogeneous, so the icon gets the whole circle and sits in its
    // middle; not expanding, so the circle keeps its size.
    let circle = gtk::Box::builder()
        .css_classes(["dc-status", class])
        .homogeneous(true)
        .hexpand(false)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .width_request(30)
        .height_request(30)
        .build();
    let image = ui::icon(icon, 16);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Center);
    circle.append(&image);
    circle
}
