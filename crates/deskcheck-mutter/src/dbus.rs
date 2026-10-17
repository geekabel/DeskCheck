//! The D-Bus call `org.gnome.Mutter.DisplayConfig.GetCurrentState`.
//!
//! Its reply has the signature
//! `(u a((ssss)a(siiddada{sv})a{sv}) a(iiduba(ssss)a{sv}) a{sv})`:
//! a serial, the physical monitors, the logical monitors and global
//! properties. The type aliases below follow that signature.

use std::collections::HashMap;

use zbus::proxy;
use zbus::zvariant::OwnedValue;

use crate::raw;

/// An `a{sv}` property map.
pub type Properties = HashMap<String, OwnedValue>;
/// `(ssss)`: connector, vendor, product, serial.
pub type WireSpec = (String, String, String, String);
/// `(siiddada{sv})`: id, width, height, refresh rate, preferred scale,
/// supported scales, properties.
pub type WireMode = (String, i32, i32, f64, f64, Vec<f64>, Properties);
/// `((ssss)a(siiddada{sv})a{sv})`: spec, modes, properties.
pub type WireMonitor = (WireSpec, Vec<WireMode>, Properties);
/// `(iiduba(ssss)a{sv})`: x, y, scale, transform, primary, monitors,
/// properties.
pub type WireLogicalMonitor = (i32, i32, f64, u32, bool, Vec<WireSpec>, Properties);
/// The whole reply of `GetCurrentState`.
pub type WireState = (u32, Vec<WireMonitor>, Vec<WireLogicalMonitor>, Properties);

/// Well-known name and object path of the service, on the session bus.
pub(crate) const SERVICE: &str = "org.gnome.Mutter.DisplayConfig";

#[proxy(
    interface = "org.gnome.Mutter.DisplayConfig",
    default_service = "org.gnome.Mutter.DisplayConfig",
    default_path = "/org/gnome/Mutter/DisplayConfig",
    gen_async = false
)]
pub(crate) trait DisplayConfig {
    /// Returns the current state of the monitors.
    fn get_current_state(&self) -> zbus::Result<WireState>;

    /// Sent by GNOME when screens are plugged, unplugged or changed.
    #[zbus(signal)]
    fn monitors_changed(&self) -> zbus::Result<()>;
}

/// Reads a boolean property; a missing or wrong-type value is `false`.
fn bool_property(properties: &Properties, key: &str) -> bool {
    properties
        .get(key)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

/// Reads a string property; a missing or wrong-type value is `None`.
fn string_property(properties: &Properties, key: &str) -> Option<String> {
    properties
        .get(key)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(str::to_owned)
}

fn spec((connector, vendor, product, serial): WireSpec) -> raw::MonitorSpec {
    raw::MonitorSpec {
        connector,
        vendor,
        product,
        serial,
    }
}

/// Copies the fields DeskCheck needs from the D-Bus reply.
pub(crate) fn to_raw(wire: WireState) -> raw::State {
    let (_serial, monitors, logical_monitors, _properties) = wire;
    raw::State {
        monitors: monitors
            .into_iter()
            .map(|(wire_spec, modes, properties)| raw::Monitor {
                spec: spec(wire_spec),
                modes: modes
                    .into_iter()
                    .map(
                        |(_id, width, height, refresh_rate, _, _, properties)| raw::Mode {
                            width,
                            height,
                            refresh_rate,
                            is_current: bool_property(&properties, "is-current"),
                        },
                    )
                    .collect(),
                is_builtin: bool_property(&properties, "is-builtin"),
                display_name: string_property(&properties, "display-name"),
            })
            .collect(),
        logical_monitors: logical_monitors
            .into_iter()
            .map(
                |(x, y, scale, _transform, primary, monitors, _)| raw::LogicalMonitor {
                    x,
                    y,
                    scale,
                    primary,
                    monitors: monitors.into_iter().map(spec).collect(),
                },
            )
            .collect(),
    }
}

/// Starts listening to `MonitorsChanged` on the given connection.
pub(crate) fn monitors_changed(
    connection: &zbus::blocking::Connection,
) -> zbus::Result<MonitorsChangedIterator> {
    DisplayConfigProxy::new(connection)?.receive_monitors_changed()
}

/// Calls `GetCurrentState` on the given connection.
pub(crate) fn current_state(connection: &zbus::blocking::Connection) -> zbus::Result<WireState> {
    DisplayConfigProxy::new(connection)?.get_current_state()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Str, Value};

    fn props(entries: &[(&str, OwnedValue)]) -> Properties {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.try_clone().unwrap()))
            .collect()
    }

    #[test]
    fn properties_are_read() {
        let properties = props(&[
            ("is-builtin", OwnedValue::from(true)),
            (
                "display-name",
                OwnedValue::from(Str::from("Built-in display")),
            ),
        ]);
        assert!(bool_property(&properties, "is-builtin"));
        assert!(!bool_property(&properties, "is-underscanning"));
        assert_eq!(
            string_property(&properties, "display-name").as_deref(),
            Some("Built-in display")
        );
    }

    #[test]
    fn a_property_of_the_wrong_type_is_ignored() {
        let properties = props(&[(
            "is-builtin",
            OwnedValue::try_from(Value::from("yes")).unwrap(),
        )]);
        assert!(!bool_property(&properties, "is-builtin"));
        assert_eq!(string_property(&properties, "missing"), None);
    }

    #[test]
    fn the_reply_is_copied() {
        let wire: WireState = (
            7,
            vec![(
                (
                    "DP-1".into(),
                    "DEL".into(),
                    "DELL U2419H".into(),
                    "ABC1234".into(),
                ),
                vec![(
                    "1920x1080@60".into(),
                    1920,
                    1080,
                    60.0,
                    1.0,
                    vec![1.0, 2.0],
                    props(&[("is-current", OwnedValue::from(true))]),
                )],
                props(&[(
                    "display-name",
                    OwnedValue::from(Str::from("Dell Inc. 24\"")),
                )]),
            )],
            vec![(
                1536,
                0,
                1.0,
                0,
                true,
                vec![(
                    "DP-1".into(),
                    "DEL".into(),
                    "DELL U2419H".into(),
                    "ABC1234".into(),
                )],
                Properties::new(),
            )],
            Properties::new(),
        );
        let state = to_raw(wire);
        assert_eq!(state.monitors[0].spec.connector, "DP-1");
        assert!(state.monitors[0].modes[0].is_current);
        assert!(!state.monitors[0].is_builtin);
        assert_eq!(
            state.monitors[0].display_name.as_deref(),
            Some("Dell Inc. 24\"")
        );
        assert_eq!(
            state.logical_monitors[0].monitors[0],
            state.monitors[0].spec
        );
        assert!(state.logical_monitors[0].primary);
    }
}
