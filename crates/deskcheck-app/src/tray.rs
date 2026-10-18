//! The icon of DeskCheck in the top bar (DC-30).
//!
//! GNOME has no tray of its own; Ubuntu comes with the extension
//! "`AppIndicator` and `KStatusNotifierItem` Support", which shows the items
//! of the `StatusNotifierItem` specification. DeskCheck makes one with the
//! `ksni` crate: an icon, and a menu (`com.canonical.dbusmenu`) with the
//! output, the microphone, "Mute the microphone", the quick panel and the
//! window.
//!
//! On Ubuntu 22.04 the extension opens the menu on a click, calls
//! `Activate` on a double click (DeskCheck then opens the quick panel) and
//! `SecondaryActivate` on a middle click (the window).
//!
//! The menu runs on the thread of `ksni`: its items only send a
//! [`TrayEvent`] to the interface, which acts.

use std::cell::RefCell;

use deskcheck_core::audio::DeviceId;
use deskcheck_core::{APP_ID, APP_NAME};
use gtk::gdk_pixbuf::Pixbuf;
use ksni::blocking::TrayMethods as _;
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Category, Icon, MenuItem, ToolTip};

use crate::tray_model::{self, TrayState};

/// The logo of DeskCheck, in the resources of the app.
const LOGO: &str = "/io/github/geekabel/DeskCheck/icons/scalable/actions/dc-logo-symbolic.svg";

/// The sizes of the icon given to the top bar: it picks the closest.
const ICON_SIZES: [i32; 4] = [16, 22, 32, 48];

/// What the user chose in the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// Open the quick panel.
    QuickPanel,
    /// Show the window of DeskCheck.
    OpenWindow,
    /// Show the Diagnostic page.
    Diagnostic,
    /// Make this output the default one.
    SetOutput(DeviceId),
    /// Make this microphone the default one.
    SetInput(DeviceId),
    /// Mute the default microphone, or turn it back on.
    ToggleMute,
    /// Show the numbers on the screens.
    Identify,
    /// Quit DeskCheck.
    Quit,
}

/// The item of the top bar, as `ksni` serves it.
struct DeskTray {
    state: TrayState,
    events: async_channel::Sender<TrayEvent>,
    icon: Vec<Icon>,
}

impl DeskTray {
    fn send(&self, event: TrayEvent) {
        // `send_blocking` on an unbounded channel never waits.
        let _ = self.events.send_blocking(event);
    }
}

/// A plain item of the menu that sends an event.
fn item(label: &str, event: TrayEvent) -> MenuItem<DeskTray> {
    StandardItem {
        label: label.to_owned(),
        activate: Box::new(move |tray: &mut DeskTray| tray.send(event.clone())),
        ..Default::default()
    }
    .into()
}

/// A submenu that picks one device of a list.
fn devices(
    label: String,
    list: &[tray_model::TrayDevice],
    selected: Option<usize>,
    pick: fn(DeviceId) -> TrayEvent,
    get: fn(&TrayState) -> &[tray_model::TrayDevice],
) -> MenuItem<DeskTray> {
    SubMenu {
        label,
        enabled: !list.is_empty(),
        submenu: vec![
            RadioGroup {
                // Out of range when none is the default: nothing is checked.
                selected: selected.unwrap_or(usize::MAX),
                select: Box::new(move |tray: &mut DeskTray, at| {
                    if let Some(device) = get(&tray.state).get(at) {
                        tray.send(pick(device.id.clone()));
                    }
                }),
                options: list
                    .iter()
                    .map(|device| RadioItem {
                        label: device.name.clone(),
                        ..Default::default()
                    })
                    .collect(),
            }
            .into(),
        ],
        ..Default::default()
    }
    .into()
}

impl ksni::Tray for DeskTray {
    fn id(&self) -> String {
        APP_ID.to_owned()
    }

    fn title(&self) -> String {
        APP_NAME.to_owned()
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icon.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: APP_NAME.to_owned(),
            description: self.state.tooltip(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayEvent::QuickPanel);
    }

    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayEvent::OpenWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let state = &self.state;
        vec![
            StandardItem {
                label: state.status(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            item("Open the quick panel", TrayEvent::QuickPanel),
            MenuItem::Separator,
            devices(
                state.output_label(),
                &state.outputs,
                state.output,
                TrayEvent::SetOutput,
                |state| &state.outputs,
            ),
            devices(
                state.input_label(),
                &state.inputs,
                state.input,
                TrayEvent::SetInput,
                |state| &state.inputs,
            ),
            CheckmarkItem {
                label: "Mute the microphone".to_owned(),
                enabled: state.mic_muted.is_some(),
                checked: state.mic_muted.unwrap_or(false),
                activate: Box::new(|tray: &mut Self| tray.send(TrayEvent::ToggleMute)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            item("Identify screens", TrayEvent::Identify),
            item("Diagnostic", TrayEvent::Diagnostic),
            item("Open DeskCheck", TrayEvent::OpenWindow),
            MenuItem::Separator,
            item("Quit", TrayEvent::Quit),
        ]
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        // GNOME Shell restarted, or the extension was turned off: the icon
        // comes back when the watcher does.
        gtk::glib::g_debug!("deskcheck", "top bar: the tray went away: {reason:?}");
        true
    }
}

/// White pixels with the alpha of `rgba`, in the ARGB32 format of the
/// specification (network byte order): the icon is drawn on the dark top
/// bar of Ubuntu.
pub fn white_argb(
    rgba: &[u8],
    width: usize,
    height: usize,
    rowstride: usize,
    channels: usize,
) -> Vec<u8> {
    let mut argb = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let at = y * rowstride + x * channels;
            let alpha = if channels == 4 {
                rgba.get(at + 3).copied().unwrap_or(0)
            } else {
                // No alpha: the dark parts of the drawing are the icon.
                255 - rgba.get(at).copied().unwrap_or(255)
            };
            argb.extend_from_slice(&[alpha, 255, 255, 255]);
        }
    }
    argb
}

/// The logo at the sizes of [`ICON_SIZES`].
fn logo() -> Vec<Icon> {
    ICON_SIZES
        .iter()
        .filter_map(|&size| {
            let pixbuf = Pixbuf::from_resource_at_scale(LOGO, size, size, true).ok()?;
            let width = usize::try_from(pixbuf.width()).ok()?;
            let height = usize::try_from(pixbuf.height()).ok()?;
            let rowstride = usize::try_from(pixbuf.rowstride()).ok()?;
            let channels = usize::try_from(pixbuf.n_channels()).ok()?;
            let bytes = pixbuf.read_pixel_bytes();
            Some(Icon {
                width: pixbuf.width(),
                height: pixbuf.height(),
                data: white_argb(&bytes, width, height, rowstride, channels),
            })
        })
        .collect()
}

/// The icon in the top bar, while it is there.
pub struct TopBar {
    handle: ksni::blocking::Handle<DeskTray>,
    shown: RefCell<TrayState>,
}

impl TopBar {
    /// Puts the icon in the top bar. The menu sends its events on
    /// `events`.
    ///
    /// # Errors
    ///
    /// Returns why the icon cannot be shown, like a GNOME without the
    /// `AppIndicator` extension.
    pub fn start(events: async_channel::Sender<TrayEvent>) -> Result<Self, String> {
        let tray = DeskTray {
            state: TrayState::default(),
            events,
            icon: logo(),
        };
        let handle = tray.spawn().map_err(|error| error.to_string())?;
        Ok(Self {
            handle,
            shown: RefCell::default(),
        })
    }

    /// Shows a new state in the menu, when it changed.
    pub fn show(&self, state: TrayState) {
        if *self.shown.borrow() == state {
            return;
        }
        self.shown.replace(state.clone());
        self.handle.update(move |tray| tray.state = state);
    }

    /// Takes the icon away, when DeskCheck quits.
    pub fn stop(&self) {
        self.handle.shutdown().wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_white_with_the_alpha_of_the_drawing() {
        // Two pixels: a black opaque one, then a transparent one, in rows
        // of 3 pixels (the last one is padding).
        let rgba = [0, 0, 0, 255, 0, 0, 0, 0, 9, 9, 9, 9];
        assert_eq!(
            white_argb(&rgba, 2, 1, 12, 4),
            [255, 255, 255, 255, 0, 255, 255, 255]
        );
        // Without alpha, black is the icon.
        assert_eq!(white_argb(&[0, 0, 0], 1, 1, 3, 3), [255, 255, 255, 255]);
    }
}
