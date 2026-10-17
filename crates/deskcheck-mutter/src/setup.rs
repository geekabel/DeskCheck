//! Changing the screens: what GNOME needs to know to apply a layout, and
//! the plan of a change, checked with the rules of Mutter before it is
//! sent.
//!
//! [`Setup`] is the full reply of `GetCurrentState`: every mode of every
//! screen, the scales each mode accepts, and the serial a change must
//! give back. A [`Change`] turns it into a [`Plan`], the logical monitors
//! to pass to `ApplyMonitorsConfig`. Everything here is pure, so it is
//! tested without GNOME.
//!
//! The rules come from `meta_verify_monitors_config` in Mutter 42: one
//! primary screen, no overlap, each screen touching another one, the top
//! left corner at 0,0, the same scale everywhere when GNOME requires it
//! (X11), and a scale the mode supports.

use std::fmt;

use crate::dbus::{Properties, WireMonitor, WireState};

/// How GNOME sizes a logical monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// The size of the mode divided by the scale (Wayland).
    Logical,
    /// The size of the mode (X11).
    Physical,
}

/// One mode of a screen.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeOption {
    /// Its id, to pass back to GNOME.
    pub id: String,
    /// Width in pixels.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
    /// Refresh rate in hertz.
    pub refresh_rate: f64,
    /// The scale GNOME suggests for it.
    pub preferred_scale: f64,
    /// The scales it accepts.
    pub scales: Vec<f64>,
    /// The best mode of the screen.
    pub preferred: bool,
}

/// A screen plugged in, with its modes.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorSetup {
    /// Connector, like `DP-1`.
    pub connector: String,
    /// Every mode it can show.
    pub modes: Vec<ModeOption>,
    /// The id of the mode in use, when the screen is on.
    pub current: Option<String>,
}

impl MonitorSetup {
    /// A mode by its id.
    #[must_use]
    pub fn mode(&self, id: &str) -> Option<&ModeOption> {
        self.modes.iter().find(|mode| mode.id == id)
    }

    /// The mode to use when nothing else is said: the current one, else
    /// the preferred one, else the first.
    #[must_use]
    pub fn default_mode(&self) -> Option<&ModeOption> {
        self.current
            .as_deref()
            .and_then(|id| self.mode(id))
            .or_else(|| self.modes.iter().find(|mode| mode.preferred))
            .or_else(|| self.modes.first())
    }
}

/// A logical monitor in use: a place on the desktop, shown by one screen
/// or by several (mirrored).
#[derive(Debug, Clone, PartialEq)]
pub struct LogicalSetup {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Scale.
    pub scale: f64,
    /// Rotation, as GNOME numbers it (0 is normal).
    pub transform: u32,
    /// The primary one.
    pub primary: bool,
    /// The connectors of the screens that show it.
    pub connectors: Vec<String>,
}

/// Everything needed to change the screens.
#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    /// The serial to give back with a change.
    pub serial: u32,
    /// How logical monitors are sized.
    pub layout_mode: LayoutMode,
    /// `true` when the layout mode may be sent with a change.
    pub supports_changing_layout_mode: bool,
    /// `true` when every screen must use the same scale (X11).
    pub global_scale_required: bool,
    /// The screens plugged in.
    pub monitors: Vec<MonitorSetup>,
    /// The logical monitors in use.
    pub logical: Vec<LogicalSetup>,
}

fn bool_property(properties: &Properties, key: &str) -> bool {
    properties
        .get(key)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

fn monitor_setup((spec, modes, _properties): WireMonitor) -> MonitorSetup {
    let mut current = None;
    let modes = modes
        .into_iter()
        .map(
            |(id, width, height, refresh_rate, preferred_scale, scales, properties)| {
                if bool_property(&properties, "is-current") {
                    current = Some(id.clone());
                }
                ModeOption {
                    preferred: bool_property(&properties, "is-preferred"),
                    id,
                    width,
                    height,
                    refresh_rate,
                    preferred_scale,
                    scales,
                }
            },
        )
        .collect();
    MonitorSetup {
        connector: spec.0,
        modes,
        current,
    }
}

impl Setup {
    /// Reads the reply of `GetCurrentState`.
    #[must_use]
    pub fn from_wire(wire: WireState) -> Self {
        let (serial, monitors, logical, properties) = wire;
        // Absent, the layout mode is logical (see the D-Bus interface).
        let layout_mode = match properties
            .get("layout-mode")
            .and_then(|value| u32::try_from(value).ok())
        {
            Some(2) => LayoutMode::Physical,
            _ => LayoutMode::Logical,
        };
        Self {
            serial,
            layout_mode,
            supports_changing_layout_mode: bool_property(
                &properties,
                "supports-changing-layout-mode",
            ),
            global_scale_required: bool_property(&properties, "global-scale-required"),
            monitors: monitors.into_iter().map(monitor_setup).collect(),
            logical: logical
                .into_iter()
                .map(
                    |(x, y, scale, transform, primary, monitors, _)| LogicalSetup {
                        x,
                        y,
                        scale,
                        transform,
                        primary,
                        connectors: monitors.into_iter().map(|spec| spec.0).collect(),
                    },
                )
                .collect(),
        }
    }

    /// A screen by its connector.
    #[must_use]
    pub fn monitor(&self, connector: &str) -> Option<&MonitorSetup> {
        self.monitors.iter().find(|m| m.connector == connector)
    }
}

/// How the screens share the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrangement {
    /// Each screen shows its own part of the desktop.
    Extend,
    /// Every screen shows the same picture.
    Mirror,
    /// Only the primary screen is on.
    Single,
}

/// A change the user asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Extend, mirror, or one screen.
    Arrange(Arrangement),
    /// Make a screen the primary one.
    Primary(String),
    /// Change the mode of a screen.
    Mode {
        /// The screen.
        connector: String,
        /// The id of the mode.
        mode: String,
    },
    /// Change the scale of a screen.
    Scale {
        /// The screen.
        connector: String,
        /// The scale.
        scale: f64,
    },
    /// Place a screen near a point of the desktop. It is moved to touch
    /// the nearest screen without covering any.
    Move {
        /// The screen.
        connector: String,
        /// Where its left edge should be.
        x: i32,
        /// Where its top edge should be.
        y: i32,
    },
}

/// Why a change cannot be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// No screen is known by that connector, or it is off.
    UnknownScreen(String),
    /// The screen has no such mode.
    UnknownMode(String),
    /// The mode does not accept the scale.
    ScaleNotSupported(String),
    /// The screens have no resolution in common, so they cannot mirror.
    NothingInCommon,
    /// The layout breaks a rule of GNOME.
    Invalid(&'static str),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScreen(connector) => write!(f, "no screen is on at {connector}"),
            Self::UnknownMode(mode) => write!(f, "the screen has no mode {mode}"),
            Self::ScaleNotSupported(scale) => {
                write!(f, "this resolution does not accept the scale {scale}")
            }
            Self::NothingInCommon => {
                f.write_str("these screens have no resolution in common, so they cannot mirror")
            }
            Self::Invalid(rule) => f.write_str(rule),
        }
    }
}

impl std::error::Error for PlanError {}

/// A logical monitor to send to GNOME.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicalPlan {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Scale.
    pub scale: f64,
    /// Rotation, kept as it is.
    pub transform: u32,
    /// The primary one.
    pub primary: bool,
    /// The screens that show it, with the id of their mode.
    pub monitors: Vec<(String, String)>,
}

/// The layout to send to GNOME.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The logical monitors.
    pub logical: Vec<LogicalPlan>,
}

/// A rectangle on the desktop: x, y, width, height.
type Rect = (i32, i32, i32, i32);

fn overlaps(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Same rule as `meta_rectangle_is_adjacent_to`: a shared edge of some
/// length.
fn adjacent(a: Rect, b: Rect) -> bool {
    let (ax2, ay2, bx2, by2) = (a.0 + a.2, a.1 + a.3, b.0 + b.2, b.1 + b.3);
    ((a.0 == bx2 || ax2 == b.0) && !(ay2 <= b.1 || a.1 >= by2))
        || ((a.1 == by2 || ay2 == b.1) && !(ax2 <= b.0 || a.0 >= bx2))
}

fn same_scale(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-4
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "screen sizes are a few thousand pixels"
)]
fn round(value: f64) -> i32 {
    value.round() as i32
}

impl Plan {
    /// The layout in use now.
    #[must_use]
    pub fn current(setup: &Setup) -> Self {
        let logical = setup
            .logical
            .iter()
            .map(|logical| LogicalPlan {
                x: logical.x,
                y: logical.y,
                scale: logical.scale,
                transform: logical.transform,
                primary: logical.primary,
                monitors: logical
                    .connectors
                    .iter()
                    .filter_map(|connector| {
                        let monitor = setup.monitor(connector)?;
                        Some((connector.clone(), monitor.default_mode()?.id.clone()))
                    })
                    .collect(),
            })
            .collect();
        Self { logical }
    }

    /// The size of a logical monitor on the desktop.
    fn size(setup: &Setup, logical: &LogicalPlan) -> Option<(i32, i32)> {
        let (connector, mode) = logical.monitors.first()?;
        let mode = setup.monitor(connector)?.mode(mode)?;
        let (width, height) = if logical.transform % 2 == 1 {
            (mode.height, mode.width)
        } else {
            (mode.width, mode.height)
        };
        Some(match setup.layout_mode {
            LayoutMode::Logical => (
                round(f64::from(width) / logical.scale),
                round(f64::from(height) / logical.scale),
            ),
            LayoutMode::Physical => (width, height),
        })
    }

    fn rects(&self, setup: &Setup) -> Vec<Rect> {
        self.logical
            .iter()
            .map(|logical| {
                let (w, h) = Self::size(setup, logical).unwrap_or((0, 0));
                (logical.x, logical.y, w, h)
            })
            .collect()
    }

    fn find(&self, connector: &str) -> Result<usize, PlanError> {
        self.logical
            .iter()
            .position(|l| l.monitors.iter().any(|(c, _)| c == connector))
            .ok_or_else(|| PlanError::UnknownScreen(connector.to_owned()))
    }

    /// Moves everything so the top left corner is at 0,0.
    fn normalize(&mut self) {
        let min_x = self.logical.iter().map(|l| l.x).min().unwrap_or(0);
        let min_y = self.logical.iter().map(|l| l.y).min().unwrap_or(0);
        for logical in &mut self.logical {
            logical.x -= min_x;
            logical.y -= min_y;
        }
    }

    /// Places the screens again after a size changed, keeping their order:
    /// side by side when they were side by side, else one above the other.
    fn repack(&mut self, setup: &Setup) {
        if self.logical.len() < 2 {
            self.normalize();
            return;
        }
        let rects = self.rects(setup);
        let mut order: Vec<usize> = (0..self.logical.len()).collect();
        let stacked = {
            let mut by_y = order.clone();
            by_y.sort_by_key(|&i| (rects[i].1, rects[i].0));
            by_y.windows(2)
                .all(|pair| rects[pair[1]].1 >= rects[pair[0]].1 + rects[pair[0]].3)
        };
        if stacked {
            order.sort_by_key(|&i| (rects[i].1, rects[i].0));
        } else {
            order.sort_by_key(|&i| (rects[i].0, rects[i].1));
        }
        let sizes: Vec<(i32, i32)> = self
            .logical
            .iter()
            .map(|l| Self::size(setup, l).unwrap_or((0, 0)))
            .collect();
        let mut previous: Option<Rect> = None;
        for &i in &order {
            let (w, h) = sizes[i];
            let (x, y) = match previous {
                None => (self.logical[i].x, self.logical[i].y),
                Some((px, py, pw, ph)) if stacked => {
                    // Below the previous one, keeping some width in common.
                    let x = self.logical[i].x.clamp(px - w + 1, px + pw - 1);
                    (x, py + ph)
                }
                Some((px, py, pw, ph)) => {
                    // On its right, keeping some height in common.
                    let y = self.logical[i].y.clamp(py - h + 1, py + ph - 1);
                    (px + pw, y)
                }
            };
            self.logical[i].x = x;
            self.logical[i].y = y;
            previous = Some((x, y, w, h));
        }
        self.normalize();
    }

    /// Applies a change to this layout, then checks it.
    ///
    /// # Errors
    ///
    /// Returns a [`PlanError`] when the change names an unknown screen or
    /// mode, or when the result breaks a rule of GNOME.
    pub fn change(mut self, setup: &Setup, change: &Change) -> Result<Self, PlanError> {
        match change {
            Change::Arrange(arrangement) => self = Self::arrange(setup, &self, *arrangement)?,
            Change::Primary(connector) => {
                let at = self.find(connector)?;
                for (i, logical) in self.logical.iter_mut().enumerate() {
                    logical.primary = i == at;
                }
            }
            Change::Mode { connector, mode } => {
                let at = self.find(connector)?;
                let option = setup
                    .monitor(connector)
                    .and_then(|m| m.mode(mode))
                    .ok_or_else(|| PlanError::UnknownMode(mode.clone()))?;
                let logical = &mut self.logical[at];
                for (c, m) in &mut logical.monitors {
                    if c == connector {
                        m.clone_from(mode);
                    }
                }
                // A scale the new mode does not accept becomes its own.
                if !option.scales.iter().any(|s| same_scale(*s, logical.scale)) {
                    logical.scale = option.preferred_scale;
                }
                self.follow_global_scale(setup, at);
                self.repack(setup);
            }
            Change::Scale { connector, scale } => {
                let at = self.find(connector)?;
                self.logical[at].scale = *scale;
                self.follow_global_scale(setup, at);
                self.repack(setup);
            }
            Change::Move { connector, x, y } => {
                let at = self.find(connector)?;
                self.place_near(setup, at, *x, *y);
                self.normalize();
            }
        }
        self.check(setup)?;
        Ok(self)
    }

    /// On X11 every screen has the scale of the one just changed.
    fn follow_global_scale(&mut self, setup: &Setup, at: usize) {
        if setup.global_scale_required {
            let scale = self.logical[at].scale;
            for logical in &mut self.logical {
                logical.scale = scale;
            }
        }
    }

    /// Puts a logical monitor touching the nearest other one, as close to
    /// `(x, y)` as it can without covering a third one.
    fn place_near(&mut self, setup: &Setup, at: usize, x: i32, y: i32) {
        let rects = self.rects(setup);
        let (_, _, w, h) = rects[at];
        let others: Vec<Rect> = rects
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, r)| *r)
            .collect();
        let mut best: Option<((i32, i32), i64)> = None;
        for &(ox, oy, ow, oh) in &others {
            let along_x = x.clamp(ox - w + 1, ox + ow - 1);
            let along_y = y.clamp(oy - h + 1, oy + oh - 1);
            for candidate in [
                (ox - w, along_y),
                (ox + ow, along_y),
                (along_x, oy - h),
                (along_x, oy + oh),
            ] {
                let rect = (candidate.0, candidate.1, w, h);
                if others.iter().any(|other| overlaps(rect, *other)) {
                    continue;
                }
                let distance =
                    i64::from(candidate.0 - x).pow(2) + i64::from(candidate.1 - y).pow(2);
                if best.is_none_or(|(_, d)| distance < d) {
                    best = Some((candidate, distance));
                }
            }
        }
        if let Some(((nx, ny), _)) = best {
            self.logical[at].x = nx;
            self.logical[at].y = ny;
        }
    }

    /// Extend, mirror, or keep only the primary screen.
    fn arrange(setup: &Setup, current: &Self, arrangement: Arrangement) -> Result<Self, PlanError> {
        let primary = current
            .logical
            .iter()
            .find(|l| l.primary)
            .or_else(|| current.logical.first())
            .and_then(|l| l.monitors.first())
            .map(|(c, _)| c.clone());
        let scale_of = |connector: &str| -> f64 {
            current
                .logical
                .iter()
                .find(|l| l.monitors.iter().any(|(c, _)| c == connector))
                .map(|l| l.scale)
                .or_else(|| {
                    setup
                        .monitor(connector)
                        .and_then(MonitorSetup::default_mode)
                        .map(|m| m.preferred_scale)
                })
                .unwrap_or(1.0)
        };
        let mut plan = match arrangement {
            Arrangement::Single => {
                let connector = primary.ok_or(PlanError::Invalid("no screen is on"))?;
                let mode = current
                    .logical
                    .iter()
                    .flat_map(|l| l.monitors.iter())
                    .find(|(c, _)| *c == connector)
                    .map(|(_, m)| m.clone())
                    .ok_or_else(|| PlanError::UnknownScreen(connector.clone()))?;
                Self {
                    logical: vec![LogicalPlan {
                        x: 0,
                        y: 0,
                        scale: scale_of(&connector),
                        transform: 0,
                        primary: true,
                        monitors: vec![(connector, mode)],
                    }],
                }
            }
            Arrangement::Extend => {
                // Out of a mirror, each screen gets its own best mode again.
                let from_mirror = current.logical.iter().any(|l| l.monitors.len() > 1);
                let mut x = 0;
                let mut logical = Vec::new();
                // The primary screen first, then the others in the order of GNOME.
                let mut monitors: Vec<&MonitorSetup> = setup.monitors.iter().collect();
                monitors.sort_by_key(|m| Some(&m.connector) != primary.as_ref());
                for monitor in monitors {
                    let preferred = monitor.modes.iter().find(|m| m.preferred);
                    let Some(mode) = preferred
                        .filter(|_| from_mirror)
                        .or_else(|| monitor.default_mode())
                    else {
                        continue;
                    };
                    let mut scale = scale_of(&monitor.connector);
                    if !mode.scales.iter().any(|s| same_scale(*s, scale)) {
                        scale = mode.preferred_scale;
                    }
                    let item = LogicalPlan {
                        x,
                        y: 0,
                        scale,
                        transform: 0,
                        primary: Some(&monitor.connector) == primary.as_ref(),
                        monitors: vec![(monitor.connector.clone(), mode.id.clone())],
                    };
                    x += Self::size(setup, &item).map_or(0, |(w, _)| w);
                    logical.push(item);
                }
                if setup.global_scale_required {
                    if let Some(scale) = logical.first().map(|l| l.scale) {
                        for item in &mut logical {
                            item.scale = scale;
                        }
                    }
                }
                let mut plan = Self { logical };
                plan.repack(setup);
                plan
            }
            Arrangement::Mirror => Self::mirror(setup, scale_of(primary.as_deref().unwrap_or("")))?,
        };
        plan.normalize();
        Ok(plan)
    }

    /// One logical monitor shown by every screen, at a size they all have.
    fn mirror(setup: &Setup, wanted_scale: f64) -> Result<Self, PlanError> {
        let monitors: Vec<&MonitorSetup> = setup
            .monitors
            .iter()
            .filter(|m| !m.modes.is_empty())
            .collect();
        let first = monitors
            .first()
            .ok_or(PlanError::Invalid("no screen is plugged in"))?;
        let mut sizes: Vec<(i32, i32)> = first
            .modes
            .iter()
            .map(|m| (m.width, m.height))
            .filter(|size| {
                monitors
                    .iter()
                    .all(|other| other.modes.iter().any(|m| (m.width, m.height) == *size))
            })
            .collect();
        sizes.sort_by_key(|(w, h)| std::cmp::Reverse(i64::from(*w) * i64::from(*h)));
        let size = *sizes.first().ok_or(PlanError::NothingInCommon)?;
        let chosen: Vec<&ModeOption> = monitors
            .iter()
            .filter_map(|monitor| {
                monitor
                    .modes
                    .iter()
                    .filter(|m| (m.width, m.height) == size)
                    .max_by(|a, b| a.refresh_rate.total_cmp(&b.refresh_rate))
            })
            .collect();
        let common_scales: Vec<f64> = chosen[0]
            .scales
            .iter()
            .copied()
            .filter(|s| {
                chosen
                    .iter()
                    .all(|m| m.scales.iter().any(|o| same_scale(*o, *s)))
            })
            .collect();
        let scale = [wanted_scale, 1.0]
            .into_iter()
            .find(|s| common_scales.iter().any(|o| same_scale(*o, *s)))
            .or_else(|| common_scales.first().copied())
            .unwrap_or(1.0);
        Ok(Self {
            logical: vec![LogicalPlan {
                x: 0,
                y: 0,
                scale,
                transform: 0,
                primary: true,
                monitors: monitors
                    .iter()
                    .zip(chosen)
                    .map(|(monitor, mode)| (monitor.connector.clone(), mode.id.clone()))
                    .collect(),
            }],
        })
    }

    /// Checks the rules of GNOME, as `meta_verify_monitors_config` does.
    ///
    /// # Errors
    ///
    /// Returns the first rule the layout breaks.
    pub fn check(&self, setup: &Setup) -> Result<(), PlanError> {
        if self.logical.is_empty() {
            return Err(PlanError::Invalid("at least one screen must be on"));
        }
        if self.logical.iter().filter(|l| l.primary).count() != 1 {
            return Err(PlanError::Invalid("exactly one screen must be primary"));
        }
        for logical in &self.logical {
            let mut size = None;
            for (connector, mode) in &logical.monitors {
                let option = setup
                    .monitor(connector)
                    .ok_or_else(|| PlanError::UnknownScreen(connector.clone()))?
                    .mode(mode)
                    .ok_or_else(|| PlanError::UnknownMode(mode.clone()))?;
                if !option.scales.iter().any(|s| same_scale(*s, logical.scale)) {
                    return Err(PlanError::ScaleNotSupported(format!(
                        "{:.2}",
                        logical.scale
                    )));
                }
                let this = (option.width, option.height);
                if size.is_some_and(|size| size != this) {
                    return Err(PlanError::Invalid(
                        "mirrored screens need the same resolution",
                    ));
                }
                size = Some(this);
            }
        }
        if setup.global_scale_required
            && self
                .logical
                .windows(2)
                .any(|pair| !same_scale(pair[0].scale, pair[1].scale))
        {
            return Err(PlanError::Invalid("every screen must use the same scale"));
        }
        let rects = self.rects(setup);
        for (i, a) in rects.iter().enumerate() {
            if rects.iter().skip(i + 1).any(|b| overlaps(*a, *b)) {
                return Err(PlanError::Invalid("two screens would cover each other"));
            }
            if rects.len() > 1
                && !rects
                    .iter()
                    .enumerate()
                    .any(|(j, b)| j != i && adjacent(*a, *b))
            {
                return Err(PlanError::Invalid("every screen must touch another one"));
            }
        }
        let min_x = rects.iter().map(|r| r.0).min().unwrap_or(0);
        let min_y = rects.iter().map(|r| r.1).min().unwrap_or(0);
        if (min_x, min_y) != (0, 0) {
            return Err(PlanError::Invalid("the top left screen must start at 0,0"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(
        id: &str,
        size: (i32, i32),
        refresh: f64,
        scales: &[f64],
        preferred: bool,
    ) -> ModeOption {
        ModeOption {
            id: id.to_owned(),
            width: size.0,
            height: size.1,
            refresh_rate: refresh,
            preferred_scale: 1.0,
            scales: scales.to_vec(),
            preferred,
        }
    }

    /// Godwin's home: the laptop on the left, primary, the ASUS on the right.
    fn home() -> Setup {
        Setup {
            serial: 7,
            layout_mode: LayoutMode::Logical,
            supports_changing_layout_mode: false,
            global_scale_required: false,
            monitors: vec![
                MonitorSetup {
                    connector: "eDP-1".to_owned(),
                    modes: vec![
                        mode("1920x1080@60", (1920, 1080), 60.0, &[1.0, 1.25, 1.5], true),
                        mode("1280x720@60", (1280, 720), 60.0, &[1.0], false),
                    ],
                    current: Some("1920x1080@60".to_owned()),
                },
                MonitorSetup {
                    connector: "DP-2".to_owned(),
                    modes: vec![
                        mode(
                            "2560x1440@144",
                            (2560, 1440),
                            144.0,
                            &[1.0, 1.25, 2.0],
                            true,
                        ),
                        mode("1920x1080@60", (1920, 1080), 60.0, &[1.0, 1.25], false),
                    ],
                    current: Some("2560x1440@144".to_owned()),
                },
            ],
            logical: vec![
                LogicalSetup {
                    x: 0,
                    y: 0,
                    scale: 1.0,
                    transform: 0,
                    primary: true,
                    connectors: vec!["eDP-1".to_owned()],
                },
                LogicalSetup {
                    x: 1920,
                    y: 0,
                    scale: 1.0,
                    transform: 0,
                    primary: false,
                    connectors: vec!["DP-2".to_owned()],
                },
            ],
        }
    }

    fn places(plan: &Plan) -> Vec<(i32, i32, bool)> {
        plan.logical.iter().map(|l| (l.x, l.y, l.primary)).collect()
    }

    #[test]
    fn the_current_layout_is_valid() {
        let setup = home();
        let plan = Plan::current(&setup);
        assert_eq!(plan.check(&setup), Ok(()));
        assert_eq!(places(&plan), [(0, 0, true), (1920, 0, false)]);
    }

    #[test]
    fn the_primary_screen_changes() {
        let setup = home();
        let plan = Plan::current(&setup)
            .change(&setup, &Change::Primary("DP-2".to_owned()))
            .unwrap();
        assert_eq!(places(&plan), [(0, 0, false), (1920, 0, true)]);
    }

    #[test]
    fn a_smaller_mode_keeps_the_screens_side_by_side() {
        let setup = home();
        let plan = Plan::current(&setup)
            .change(
                &setup,
                &Change::Mode {
                    connector: "eDP-1".to_owned(),
                    mode: "1280x720@60".to_owned(),
                },
            )
            .unwrap();
        // The ASUS now starts where the smaller laptop ends.
        assert_eq!(places(&plan), [(0, 0, true), (1280, 0, false)]);
        // 1280 x 720 accepts only 100 %: a scale of 125 % would be dropped.
        assert!((plan.logical[0].scale - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_bigger_scale_makes_the_screen_smaller_on_the_desktop() {
        let setup = home();
        let plan = Plan::current(&setup)
            .change(
                &setup,
                &Change::Scale {
                    connector: "eDP-1".to_owned(),
                    scale: 1.25,
                },
            )
            .unwrap();
        // 1920 / 1.25 = 1536.
        assert_eq!(places(&plan), [(0, 0, true), (1536, 0, false)]);
        let refused = Plan::current(&setup).change(
            &setup,
            &Change::Scale {
                connector: "eDP-1".to_owned(),
                scale: 3.0,
            },
        );
        assert_eq!(
            refused,
            Err(PlanError::ScaleNotSupported("3.00".to_owned()))
        );
    }

    #[test]
    fn a_screen_moved_to_the_left_snaps_against_the_other() {
        let setup = home();
        // The ASUS dropped a bit left of the laptop, a bit lower.
        let plan = Plan::current(&setup)
            .change(
                &setup,
                &Change::Move {
                    connector: "DP-2".to_owned(),
                    x: -2400,
                    y: 100,
                },
            )
            .unwrap();
        // Now on the left; the whole desktop starts at 0,0 again.
        assert_eq!(places(&plan), [(2560, 0, true), (0, 100, false)]);
        // Dropped above: it sits on top of the laptop.
        let above = Plan::current(&setup)
            .change(
                &setup,
                &Change::Move {
                    connector: "DP-2".to_owned(),
                    x: 200,
                    y: -1500,
                },
            )
            .unwrap();
        assert_eq!(places(&above), [(0, 1440, true), (200, 0, false)]);
    }

    #[test]
    fn mirror_uses_a_resolution_both_screens_have() {
        let setup = home();
        let plan = Plan::current(&setup)
            .change(&setup, &Change::Arrange(Arrangement::Mirror))
            .unwrap();
        assert_eq!(plan.logical.len(), 1);
        assert_eq!(
            plan.logical[0].monitors,
            [
                ("eDP-1".to_owned(), "1920x1080@60".to_owned()),
                ("DP-2".to_owned(), "1920x1080@60".to_owned())
            ]
        );
        // From mirror back to extend: two screens side by side again, each
        // with its best mode, not the one they shared.
        let mut shared = home();
        shared.monitors[1].current = Some("1920x1080@60".to_owned());
        let mirrored = Setup {
            logical: vec![LogicalSetup {
                x: 0,
                y: 0,
                scale: 1.0,
                transform: 0,
                primary: true,
                connectors: vec!["eDP-1".to_owned(), "DP-2".to_owned()],
            }],
            ..shared
        };
        let extended = Plan::current(&mirrored)
            .change(&mirrored, &Change::Arrange(Arrangement::Extend))
            .unwrap();
        assert_eq!(places(&extended), [(0, 0, true), (1920, 0, false)]);
        assert_eq!(extended.logical[1].monitors[0].1, "2560x1440@144");
    }

    #[test]
    fn one_screen_keeps_the_primary_one() {
        let setup = home();
        let plan = Plan::current(&setup)
            .change(&setup, &Change::Arrange(Arrangement::Single))
            .unwrap();
        assert_eq!(
            plan.logical[0].monitors,
            [("eDP-1".to_owned(), "1920x1080@60".to_owned())]
        );
    }

    #[test]
    fn the_rules_of_gnome_are_checked() {
        let setup = home();
        let mut plan = Plan::current(&setup);
        plan.logical[1].x = 1000;
        assert_eq!(
            plan.check(&setup),
            Err(PlanError::Invalid("two screens would cover each other"))
        );
        plan.logical[1].x = 3000;
        assert_eq!(
            plan.check(&setup),
            Err(PlanError::Invalid("every screen must touch another one"))
        );
        plan.logical[1].x = 1920;
        plan.logical[1].primary = true;
        assert_eq!(
            plan.check(&setup),
            Err(PlanError::Invalid("exactly one screen must be primary"))
        );
        let x11 = Setup {
            global_scale_required: true,
            ..home()
        };
        let plan = Plan::current(&x11)
            .change(
                &x11,
                &Change::Scale {
                    connector: "DP-2".to_owned(),
                    scale: 1.25,
                },
            )
            .unwrap();
        // On X11 both screens follow.
        assert!(plan.logical.iter().all(|l| (l.scale - 1.25).abs() < 1e-9));
        assert_eq!(
            Plan::current(&home()).change(&home(), &Change::Primary("HDMI-9".to_owned())),
            Err(PlanError::UnknownScreen("HDMI-9".to_owned()))
        );
    }
}
