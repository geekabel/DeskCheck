//! Plain copies of what Mutter sends in `GetCurrentState`.
//!
//! The D-Bus reply uses nested tuples and `a{sv}` property maps. We copy
//! the fields we need into these types, which are easy to build in tests.

/// The four strings that identify a physical monitor in Mutter:
/// connector, vendor, product and serial.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MonitorSpec {
    pub connector: String,
    pub vendor: String,
    pub product: String,
    pub serial: String,
}

/// One mode of a monitor.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mode {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f64,
    /// `is-current` property: the mode in use.
    pub is_current: bool,
}

/// A connected physical monitor.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Monitor {
    pub spec: MonitorSpec,
    pub modes: Vec<Mode>,
    /// `is-builtin` property.
    pub is_builtin: bool,
    /// `display-name` property.
    pub display_name: Option<String>,
}

/// A logical monitor: a region of the desktop shown by one or more monitors.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogicalMonitor {
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub primary: bool,
    pub monitors: Vec<MonitorSpec>,
}

/// Everything read from `GetCurrentState`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct State {
    pub monitors: Vec<Monitor>,
    pub logical_monitors: Vec<LogicalMonitor>,
}
