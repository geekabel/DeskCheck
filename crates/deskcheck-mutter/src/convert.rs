//! Conversion from the raw Mutter data to the DeskCheck model.

use deskcheck_core::display::{DisplayState, Mode, Placement, Screen, ScreenId};

use crate::raw::{self, MonitorSpec};

/// Converts a pixel size from D-Bus (`i32`) to the model (`u32`).
/// A negative size is not valid and becomes 0.
fn pixels(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

fn current_mode(monitor: &raw::Monitor) -> Option<Mode> {
    monitor
        .modes
        .iter()
        .find(|mode| mode.is_current)
        .map(|mode| Mode {
            width: pixels(mode.width),
            height: pixels(mode.height),
            refresh_rate: mode.refresh_rate,
        })
}

/// Finds where a monitor sits on the desktop: the logical monitor that
/// lists it. A monitor that no logical monitor lists is switched off.
fn placement(state: &raw::State, spec: &MonitorSpec) -> Option<Placement> {
    state
        .logical_monitors
        .iter()
        .find(|logical| logical.monitors.contains(spec))
        .map(|logical| Placement {
            x: logical.x,
            y: logical.y,
            scale: logical.scale,
            primary: logical.primary,
        })
}

/// Converts what Mutter sent to the DeskCheck model.
pub(crate) fn display_state(state: &raw::State) -> DisplayState {
    let screens = state
        .monitors
        .iter()
        .map(|monitor| Screen {
            id: ScreenId::new(
                monitor.spec.vendor.as_str(),
                monitor.spec.product.as_str(),
                monitor.spec.serial.as_str(),
            ),
            connector: monitor.spec.connector.clone(),
            display_name: monitor.display_name.clone(),
            builtin: monitor.is_builtin,
            current_mode: current_mode(monitor),
            placement: placement(state, &monitor.spec),
        })
        .collect();
    DisplayState { screens }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::{LogicalMonitor, Monitor};

    fn spec(connector: &str, vendor: &str, product: &str, serial: &str) -> MonitorSpec {
        MonitorSpec {
            connector: connector.to_owned(),
            vendor: vendor.to_owned(),
            product: product.to_owned(),
            serial: serial.to_owned(),
        }
    }

    fn mode(width: i32, height: i32, refresh_rate: f64, is_current: bool) -> raw::Mode {
        raw::Mode {
            width,
            height,
            refresh_rate,
            is_current,
        }
    }

    /// Laptop on the left, office screen on the right, office screen primary.
    fn office() -> raw::State {
        let laptop = spec("eDP-1", "BOE", "0x0a1c", "0x00000000");
        let dell = spec("DP-1", "DEL", "DELL U2419H", "ABC1234");
        raw::State {
            monitors: vec![
                Monitor {
                    spec: laptop.clone(),
                    modes: vec![mode(1920, 1080, 60.01, true), mode(1280, 720, 59.94, false)],
                    is_builtin: true,
                    display_name: Some("Built-in display".to_owned()),
                },
                Monitor {
                    spec: dell.clone(),
                    modes: vec![mode(1920, 1080, 75.0, false), mode(1920, 1080, 60.0, true)],
                    is_builtin: false,
                    display_name: Some("Dell Inc. 24\"".to_owned()),
                },
            ],
            logical_monitors: vec![
                LogicalMonitor {
                    x: 0,
                    y: 0,
                    scale: 1.25,
                    primary: false,
                    monitors: vec![laptop],
                },
                LogicalMonitor {
                    x: 1536,
                    y: 0,
                    scale: 1.0,
                    primary: true,
                    monitors: vec![dell],
                },
            ],
        }
    }

    #[test]
    fn monitors_become_screens() {
        let state = display_state(&office());
        assert_eq!(state.screens.len(), 2);
        let dell = &state.screens[1];
        assert_eq!(dell.id, ScreenId::new("DEL", "DELL U2419H", "ABC1234"));
        assert_eq!(dell.connector, "DP-1");
        assert_eq!(dell.display_name.as_deref(), Some("Dell Inc. 24\""));
        assert!(!dell.builtin);
        assert!(state.screens[0].builtin);
    }

    #[test]
    fn the_current_mode_is_the_one_marked_current() {
        let state = display_state(&office());
        let mode = state.screens[1].current_mode.unwrap();
        assert_eq!((mode.width, mode.height), (1920, 1080));
        assert!((mode.refresh_rate - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn placement_and_primary_come_from_the_logical_monitors() {
        let state = display_state(&office());
        assert_eq!(state.primary().unwrap().connector, "DP-1");
        let laptop = state.screens[0].placement.unwrap();
        assert_eq!((laptop.x, laptop.y), (0, 0));
        assert!((laptop.scale - 1.25).abs() < f64::EPSILON);
        let order: Vec<&str> = state
            .enabled()
            .iter()
            .map(|s| s.connector.as_str())
            .collect();
        assert_eq!(order, ["eDP-1", "DP-1"]);
    }

    #[test]
    fn a_monitor_without_logical_monitor_is_off() {
        let mut raw = office();
        raw.logical_monitors.remove(1);
        for mode in &mut raw.monitors[1].modes {
            mode.is_current = false;
        }
        let state = display_state(&raw);
        let dell = &state.screens[1];
        assert!(!dell.is_enabled());
        assert!(dell.current_mode.is_none());
        assert!(state.primary().is_none());
    }

    #[test]
    fn mirrored_monitors_share_one_placement() {
        let mut raw = office();
        let dell = raw.logical_monitors.remove(1).monitors.remove(0);
        raw.logical_monitors[0].monitors.push(dell);
        let state = display_state(&raw);
        assert_eq!(state.screens[0].placement, state.screens[1].placement);
    }

    #[test]
    fn negative_sizes_become_zero() {
        assert_eq!(pixels(-1), 0);
        assert_eq!(pixels(2560), 2560);
    }

    #[test]
    fn empty_state() {
        assert_eq!(
            display_state(&raw::State::default()),
            DisplayState::default()
        );
    }
}
