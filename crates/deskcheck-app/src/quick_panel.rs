//! The quick panel (DC-30), as in the design "Rapide": the place, the
//! output with its volume, the microphone with its level, "Mute the
//! microphone", the apps that listen, the screens, and two ways to the
//! window.
//!
//! It is a small window without decorations, opened from the icon of the
//! top bar. GTK 4 cannot place a window itself; on X11, DeskCheck moves it
//! under the top bar, at the right, with the X server (`ConfigureWindow`).
//! On Wayland, GNOME places it. It closes when it loses the focus, or
//! with Esc.

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;

use adw::prelude::*;
use deskcheck_config::Config;
use deskcheck_core::audio::level::LevelBar;
use deskcheck_core::audio::{AudioState, DeviceId, Target, Volume};
use deskcheck_core::display::{DisplayState, ScreenId};
use gtk::{gdk, glib};

use crate::backend::{Backend, Request};
use crate::now_model;
use crate::tray_model::{self, TrayState};
use crate::ui::{self, ButtonKind};

/// The width of the panel, as in the design.
const WIDTH: i32 = 420;

/// The room left at the right of the screen, and under the top bar.
const MARGIN: (i32, i32) = (24, 10);

/// The height of the top bar of GNOME.
const TOP_BAR: i32 = 32;

/// The blocks of the level of the microphone.
const METER_BLOCKS: usize = 8;

/// The largest volume the slider gives, as on the Now page.
const MAX_VOLUME: f64 = 150.0;

/// What the panel shows now, to act on the right device.
#[derive(Default)]
struct Shown {
    output: Option<DeviceId>,
    input: Option<DeviceId>,
    muted: bool,
    level: LevelBar,
}

/// A device picker of the design: icon, a small line, the name, and a
/// list of the devices in a popover.
struct Picker {
    button: gtk::MenuButton,
    icon: gtk::Image,
    name: gtk::Label,
    list: gtk::Box,
    popover: gtk::Popover,
}

impl Picker {
    fn new(caption: &str, extra: Option<&gtk::Widget>) -> Self {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        let icon = ui::icon("dc-speaker-symbolic", 22);
        icon.add_css_class("dc-accent-icon");
        content.append(&icon);
        let texts = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        texts.append(&ui::label(caption, &["dc-pick-caption"]));
        let name = ui::label("…", &["dc-pick-name"]);
        name.set_wrap(false);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texts.append(&name);
        content.append(&texts);
        if let Some(extra) = extra {
            content.append(extra);
        }
        content.append(&ui::icon("dc-chevron-down-symbolic", 18));
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(["dc-menu-list"])
            .build();
        let popover = gtk::Popover::builder().child(&list).build();
        let button = gtk::MenuButton::builder()
            .popover(&popover)
            .css_classes(["dc-pick"])
            .build();
        button.set_child(Some(&content));
        Self {
            button,
            icon,
            name,
            list,
            popover,
        }
    }

    /// Shows the device in use and the list to pick from.
    fn show(
        &self,
        icon: &str,
        name: Option<&str>,
        devices: &[tray_model::TrayDevice],
        current: Option<usize>,
        pick: impl Fn(DeviceId) + Clone + 'static,
    ) {
        self.icon.set_icon_name(Some(icon));
        self.name.set_label(name.unwrap_or("None"));
        self.button.set_sensitive(!devices.is_empty());
        ui::clear(&self.list);
        for (at, device) in devices.iter().enumerate() {
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let text = ui::label(&device.name, &[]);
            text.set_hexpand(true);
            line.append(&text);
            let check = ui::icon("dc-ok-symbolic", 16);
            check.set_opacity(if current == Some(at) { 1.0 } else { 0.0 });
            line.append(&check);
            let item = gtk::Button::builder()
                .child(&line)
                .css_classes(["dc-btn", "dc-flat"])
                .build();
            let id = device.id.clone();
            let pick = pick.clone();
            let popover = self.popover.clone();
            item.connect_clicked(move |_| {
                popover.popdown();
                pick(id.clone());
            });
            self.list.append(&item);
        }
    }
}

/// The quick panel.
pub struct QuickPanel {
    window: gtk::Window,
    backend: Backend,
    place: gtk::Label,
    ready: gtk::Box,
    ready_icon: gtk::Image,
    ready_text: gtk::Label,
    output: Picker,
    volume: gtk::Scale,
    setting_volume: Cell<bool>,
    input: Picker,
    meter: ui::Meter,
    mute: gtk::Button,
    listeners: gtk::Box,
    badges: gtk::Box,
    screens_line: gtk::Label,
    css: gtk::CssProvider,
    shown: RefCell<Shown>,
    /// Where to put the panel on X11: under the top bar, at the right of
    /// the primary screen.
    anchor: Rc<Cell<Option<(i32, i32)>>>,
}

/// A small grey title of the panel.
fn section(text: &str) -> gtk::Label {
    ui::label(&text.to_uppercase(), &["dc-panel-section"])
}

impl QuickPanel {
    /// Builds the panel, hidden. The buttons at the bottom run the
    /// actions `app.open-window` and `app.diagnostic`.
    #[allow(
        clippy::too_many_lines,
        reason = "builds the parts of the design in order, easier to read in one place"
    )]
    pub fn new(app: &adw::Application, backend: Backend) -> Rc<Self> {
        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .css_classes(["dc-panel"])
            .build();

        // The place, and whether all is ready for a call.
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let place_icon = ui::icon("dc-place-symbolic", 18);
        place_icon.add_css_class("dc-accent-icon");
        head.append(&place_icon);
        let place = ui::label("", &["dc-panel-place"]);
        place.set_hexpand(true);
        head.append(&place);
        let ready = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(5)
            .valign(gtk::Align::Center)
            .css_classes(["dc-tag", "ok"])
            .build();
        let ready_icon = ui::icon("dc-ok-symbolic", 12);
        ready.append(&ready_icon);
        let ready_text = gtk::Label::new(None);
        ready.append(&ready_text);
        head.append(&ready);
        body.append(&head);

        // The sound.
        let sound = gtk::Box::new(gtk::Orientation::Vertical, 8);
        sound.append(&section("Sound"));
        let output = Picker::new("You hear through", None);
        sound.append(&output.button);
        let volume_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .margin_start(4)
            .margin_end(4)
            .build();
        volume_row.append(&ui::label("Volume", &["dc-small"]));
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, MAX_VOLUME, 1.0);
        volume.set_hexpand(true);
        volume.set_draw_value(false);
        volume.add_mark(100.0, gtk::PositionType::Bottom, None);
        volume_row.append(&volume);
        sound.append(&volume_row);
        let meter = ui::Meter::new(METER_BLOCKS);
        let input = Picker::new("They hear you via", Some(meter.root.upcast_ref()));
        sound.append(&input.button);
        let mute = ui::button(
            tray_model::mute_text(false),
            Some("dc-microphone-muted-symbolic"),
            ButtonKind::Normal,
        );
        mute.set_halign(gtk::Align::Start);
        sound.append(&mute);
        body.append(&sound);

        // Who is listening.
        let listening = gtk::Box::new(gtk::Orientation::Vertical, 8);
        listening.append(&section("Who is listening?"));
        let listeners = gtk::Box::new(gtk::Orientation::Vertical, 6);
        listening.append(&listeners);
        body.append(&listening);

        // The screens.
        let screens = gtk::Box::new(gtk::Orientation::Vertical, 8);
        screens.append(&section("Screens"));
        let screens_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let badges = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        badges.set_valign(gtk::Align::Center);
        screens_row.append(&badges);
        let screens_line = ui::label("", &["dc-panel-text"]);
        screens_line.set_hexpand(true);
        screens_row.append(&screens_line);
        let identify = ui::button("Identify", None, ButtonKind::Normal);
        identify.add_css_class("dc-compact");
        identify.set_action_name(Some("app.identify"));
        identify.set_action_target_value(Some(&0_i32.to_variant()));
        screens_row.append(&identify);
        screens.append(&screens_row);
        body.append(&screens);

        // The way to the window.
        body.append(&ui::line());
        let foot = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let diag = ui::button("Diagnostic", None, ButtonKind::Flat);
        foot.append(&diag);
        let spacer = gtk::Box::builder().hexpand(true).build();
        foot.append(&spacer);
        let open = ui::button("Open DeskCheck", None, ButtonKind::Normal);
        foot.append(&open);
        body.append(&foot);

        let window = gtk::Window::builder()
            .application(app)
            .title("DeskCheck quick panel")
            .decorated(false)
            .resizable(false)
            .default_width(WIDTH)
            .css_classes(["dc-panel-window"])
            .child(&body)
            .hide_on_close(true)
            .build();

        let css = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let panel = Rc::new(Self {
            window,
            backend,
            place,
            ready,
            ready_icon,
            ready_text,
            output,
            volume,
            setting_volume: Cell::new(false),
            input,
            meter,
            mute,
            listeners,
            badges,
            screens_line,
            css,
            shown: RefCell::default(),
            anchor: Rc::default(),
        });
        diag.set_action_name(Some("app.diagnostic"));
        open.set_action_name(Some("app.open-window"));
        panel.connect(&[&diag, &open, &identify]);
        panel
    }

    /// Connects the controls. The `closing` buttons close the panel.
    fn connect(self: &Rc<Self>, closing: &[&gtk::Button]) {
        for button in closing {
            let window = self.window.clone();
            button.connect_clicked(move |_| window.set_visible(false));
        }
        {
            let panel = Rc::downgrade(self);
            self.volume.connect_value_changed(move |scale| {
                let Some(panel) = panel.upgrade() else {
                    return;
                };
                if panel.setting_volume.get() {
                    return;
                }
                let Some(output) = panel.shown.borrow().output.clone() else {
                    return;
                };
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "the slider goes from 0 to 150"
                )]
                let percent = scale.value().round() as u16;
                panel.backend.request(Request::SetVolume(
                    Target::Output(output),
                    Volume::from_percent(percent),
                ));
            });
        }
        {
            let panel = Rc::downgrade(self);
            self.mute.connect_clicked(move |_| {
                if let Some(panel) = panel.upgrade() {
                    panel.toggle_mute();
                }
            });
        }
        // Closes when another window takes the focus, like a menu. Its own
        // lists of devices take the focus too: they keep it open.
        let panel = Rc::downgrade(self);
        self.window.connect_is_active_notify(move |window| {
            let Some(panel) = panel.upgrade() else {
                return;
            };
            let listing = panel.output.popover.is_visible() || panel.input.popover.is_visible();
            if !window.is_active() && !listing {
                window.set_visible(false);
            }
        });
        let keys = gtk::EventControllerKey::new();
        {
            let window = self.window.clone();
            keys.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Escape {
                    window.set_visible(false);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        }
        self.window.add_controller(keys);
        // Placed once GDK has made its X11 window.
        let anchor = Rc::clone(&self.anchor);
        self.window.connect_map(move |window| {
            if let Some((x, y)) = anchor.get() {
                place_on_x11(window, x, y);
            }
        });
    }

    /// `true` while the panel is open.
    pub fn is_open(&self) -> bool {
        self.window.is_visible()
    }

    /// Opens the panel, or closes it when it is open.
    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.window.set_visible(false);
        } else {
            self.window.present();
        }
    }

    /// Mutes the default microphone, or turns it back on.
    pub fn toggle_mute(&self) {
        let shown = self.shown.borrow();
        if let Some(input) = shown.input.clone() {
            self.backend
                .request(Request::SetMute(Target::Input(input), !shown.muted));
        }
    }

    /// Shows the latest state.
    pub fn show(
        &self,
        config: &Config,
        sound: Option<&AudioState>,
        screens: Option<&DisplayState>,
        mouse: Option<&ScreenId>,
        place: Option<&str>,
    ) {
        let tray = TrayState::new(config, sound, place);
        self.place.set_label(place.unwrap_or("No place recognized"));
        self.ready_text
            .set_label(tray_model::ready_text(tray.ready));
        self.ready_icon.set_icon_name(Some(if tray.ready {
            "dc-ok-symbolic"
        } else {
            "dc-warning-symbolic"
        }));
        self.ready
            .set_css_classes(&["dc-tag", if tray.ready { "ok" } else { "warn" }]);
        let output = sound.and_then(AudioState::default_output_device);
        let input = sound
            .and_then(AudioState::default_input_device)
            .filter(|input| !input.is_monitor());
        {
            let mut shown = self.shown.borrow_mut();
            if shown.input.as_ref() != input.map(|i| &i.id) {
                shown.level = LevelBar::default();
                self.meter.set_fraction(0.0);
            }
            shown.output = output.map(|o| o.id.clone());
            shown.input = input.map(|i| i.id.clone());
            shown.muted = input.is_some_and(|i| i.muted);
        }
        let backend = self.backend.clone();
        self.output.show(
            output.map_or("dc-speaker-symbolic", |o| {
                now_model::icon_of(config, &o.id, o.hardware, false)
            }),
            tray.output_name(),
            &tray.outputs,
            tray.output,
            move |id| backend.request(Request::SetDefault(Target::Output(id))),
        );
        let backend = self.backend.clone();
        self.input.show(
            input.map_or("dc-microphone-symbolic", |i| {
                now_model::icon_of(config, &i.id, i.hardware, true)
            }),
            tray.input_name(),
            &tray.inputs,
            tray.input,
            move |id| backend.request(Request::SetDefault(Target::Input(id))),
        );
        if let Some(output) = output {
            self.setting_volume.set(true);
            self.volume.set_value(f64::from(output.volume.percent()));
            self.setting_volume.set(false);
        }
        self.volume.set_sensitive(output.is_some());
        if let Some(label) = self
            .mute
            .child()
            .and_then(|content| content.last_child())
            .and_downcast::<gtk::Label>()
        {
            label.set_label(tray_model::mute_text(tray.mic_muted.unwrap_or(false)));
        }
        self.mute.set_sensitive(tray.mic_muted.is_some());
        self.show_listeners(config, sound);
        self.show_screens(config, screens, mouse);
    }

    fn show_listeners(&self, config: &Config, sound: Option<&AudioState>) {
        ui::clear(&self.listeners);
        let rows = sound
            .map(|state| tray_model::listeners(config, state))
            .unwrap_or_default();
        if rows.is_empty() {
            self.listeners
                .append(&ui::label("No app is listening.", &["dc-small"]));
        }
        for row in rows {
            let line = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(10)
                .css_classes(["dc-listener"])
                .build();
            let icon = ui::icon(row.icon, 18);
            icon.add_css_class("dc-dim-icon");
            line.append(&icon);
            let name = ui::label(&row.name, &["dc-panel-text"]);
            name.set_hexpand(true);
            name.set_wrap(false);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            line.append(&name);
            let arrow = ui::icon("dc-back-symbolic", 16);
            arrow.add_css_class("dc-dim-icon");
            line.append(&arrow);
            let source = ui::label(&row.source, &["dc-panel-strong"]);
            source.set_wrap(false);
            line.append(&source);
            let mark = if row.ok {
                ui::icon("dc-ok-symbolic", 18)
            } else {
                ui::icon("dc-warning-symbolic", 18)
            };
            mark.add_css_class(if row.ok { "dc-ok-icon" } else { "dc-warn-icon" });
            line.append(&mark);
            self.listeners.append(&line);
        }
    }

    fn show_screens(
        &self,
        config: &Config,
        screens: Option<&DisplayState>,
        mouse: Option<&ScreenId>,
    ) {
        ui::clear(&self.badges);
        self.anchor.set(screens.and_then(anchor));
        let Some(state) = screens else {
            self.screens_line.set_label("Screens: not a GNOME session");
            return;
        };
        let (badges, line) = tray_model::screens_part(config, state, mouse);
        let mut css = String::new();
        for badge in badges {
            let label = gtk::Label::builder()
                .label(badge.number.to_string())
                .css_classes(["dc-panel-badge"])
                .width_request(26)
                .height_request(26)
                .build();
            if badge.mouse {
                label.add_css_class("mouse");
            }
            label.set_widget_name(&format!("panel-screen-{}", badge.number));
            let _ = writeln!(
                css,
                "#panel-screen-{} {{ background-color: {}; }}",
                badge.number, badge.color
            );
            self.badges.append(&label);
        }
        #[allow(deprecated, reason = "load_from_string needs GTK 4.12")]
        self.css.load_from_data(&css);
        self.screens_line.set_label(&line);
    }

    /// Moves the level meter of the microphone.
    pub fn peak(&self, source: &DeviceId, peak: f32) {
        if !self.window.is_visible() {
            return;
        }
        let mut shown = self.shown.borrow_mut();
        if shown.input.as_ref() == Some(source) {
            let fraction = shown.level.update(peak);
            self.meter.set_fraction(fraction);
        }
    }
}

/// The top left corner of the panel: under the top bar, at the right of
/// the primary screen, in the pixels of the X server.
fn anchor(state: &DisplayState) -> Option<(i32, i32)> {
    let primary = state.screens.iter().find(|screen| screen.is_primary())?;
    let placement = primary.placement?;
    let width = i32::try_from(primary.current_mode?.width).ok()?;
    Some((
        placement.x + width - WIDTH - MARGIN.0,
        placement.y + TOP_BAR + MARGIN.1,
    ))
}

/// On X11, moves the panel to `x`, `y`, and keeps it out of the dock and
/// of Alt+Tab.
fn place_on_x11(window: &gtk::Window, x: i32, y: i32) {
    use gdk4_x11::X11Surface;
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _};

    let Some(surface) = window.surface().and_downcast::<X11Surface>() else {
        return;
    };
    surface.set_skip_taskbar_hint(true);
    surface.set_skip_pager_hint(true);
    let Ok(xid) = u32::try_from(surface.xid()) else {
        return;
    };
    let Ok((connection, _)) = x11rb::connect(None) else {
        return;
    };
    let moved = connection
        .configure_window(xid, &ConfigureWindowAux::new().x(x).y(y))
        .is_ok()
        && connection.flush().is_ok();
    if !moved {
        glib::g_debug!("deskcheck", "quick panel: cannot move it");
    }
}
