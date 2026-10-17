//! Small widgets of the design, shared by the pages. Their look is in
//! `resources/style.css`; the icons named `dc-…` come from the mockups.

use std::cell::Cell;

use adw::prelude::*;

/// A label, left aligned, that wraps, with its style classes.
pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .css_classes(classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>())
        .build()
}

/// The small grey title at the top of a card, in capitals.
pub fn caption(text: &str) -> gtk::Label {
    label(&text.to_uppercase(), &["dc-caption"])
}

/// A label with a fixed width, before a slider or a meter.
pub fn row_label(text: &str) -> gtk::Label {
    let label = label(text, &["dc-small"]);
    label.set_wrap(false);
    label.set_width_request(54);
    label
}

/// A symbolic icon of the given size.
pub fn icon(name: &str, size: i32) -> gtk::Image {
    gtk::Image::builder()
        .icon_name(name)
        .pixel_size(size)
        .build()
}

/// A card of the design: white, with a thin border.
pub fn card() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .css_classes(["dc-card", "dc-padded"])
        .hexpand(true)
        .build()
}

/// A thin line between rows.
pub fn line() -> gtk::Separator {
    gtk::Separator::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["dc-line"])
        .build()
}

/// The title and the line under it, at the top of a page.
pub fn page_title(title: &str, subtitle: &str) -> (gtk::Box, gtk::Label, gtk::Label) {
    let block = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();
    let title = label(title, &["dc-page-title"]);
    let subtitle = label(subtitle, &["dc-page-subtitle"]);
    block.append(&title);
    block.append(&subtitle);
    (block, title, subtitle)
}

/// The kinds of buttons of the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// White, with a border.
    Normal,
    /// Only the text.
    Flat,
    /// Filled in orange: the fix of a problem.
    Warn,
    /// Filled in blue: the main action of a form.
    Primary,
}

impl ButtonKind {
    fn classes(self) -> &'static [&'static str] {
        match self {
            Self::Normal => &["dc-btn"],
            Self::Flat => &["dc-btn", "dc-flat"],
            Self::Warn => &["dc-btn", "dc-warn"],
            Self::Primary => &["dc-btn", "dc-primary"],
        }
    }
}

/// The inside of a button: an icon, maybe, and a text.
fn button_content(text: &str, icon_name: Option<&str>) -> gtk::Box {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Center)
        .build();
    if let Some(name) = icon_name {
        content.append(&icon(name, 18));
    }
    content.append(&gtk::Label::new(Some(text)));
    content
}

/// A button of the design, with an icon before the text.
pub fn button(text: &str, icon_name: Option<&str>, kind: ButtonKind) -> gtk::Button {
    gtk::Button::builder()
        .child(&button_content(text, icon_name))
        .css_classes(
            kind.classes()
                .iter()
                .map(|c| (*c).to_owned())
                .collect::<Vec<_>>(),
        )
        .valign(gtk::Align::Center)
        .build()
}

/// The colors of a tile behind an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    /// Blue: the device in use.
    Accent,
    /// Grey: another device.
    Neutral,
    /// Orange: a monitor, not a microphone.
    Warn,
}

/// The tile behind the icon of a device: `size` pixels, icon in the middle.
pub fn tile(size: i32) -> gtk::Image {
    gtk::Image::builder()
        .pixel_size(size * 6 / 11)
        .css_classes(["dc-tile"])
        .width_request(size)
        .height_request(size)
        .valign(gtk::Align::Center)
        .build()
}

/// Changes the colors of a tile.
pub fn set_tile(tile: &gtk::Image, icon_name: &str, kind: TileKind) {
    tile.set_icon_name(Some(icon_name));
    for (class, on) in [
        ("neutral", kind == TileKind::Neutral),
        ("warn", kind == TileKind::Warn),
    ] {
        if on {
            tile.add_css_class(class);
        } else {
            tile.remove_css_class(class);
        }
    }
}

/// The colors of a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// Green: all is well.
    Ok,
    /// Light orange: something to look at.
    Warn,
    /// Darker orange, on an orange row.
    WarnStrong,
    /// Blue: "Default".
    Accent,
    /// Grey: neither good nor bad.
    Neutral,
    /// Red: muted, broken.
    Error,
}

impl TagKind {
    fn class(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::WarnStrong => "warn-strong",
            Self::Accent => "accent",
            Self::Neutral => "neutral",
            Self::Error => "error",
        }
    }
}

/// A small rounded tag.
pub fn tag(text: &str, kind: TagKind) -> gtk::Label {
    let tag = gtk::Label::builder().valign(gtk::Align::Center).build();
    set_tag(&tag, text, kind);
    tag
}

/// Shows a tag with a new text and color.
pub fn set_tag(tag: &gtk::Label, text: &str, kind: TagKind) {
    tag.set_label(text);
    tag.set_css_classes(&["dc-tag", kind.class()]);
    tag.set_visible(true);
}

/// A tag with a small icon before its text.
pub fn icon_tag(icon_name: &str, text: &str, kind: TagKind) -> gtk::Box {
    let tag = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(5)
        .valign(gtk::Align::Center)
        .css_classes(["dc-tag", kind.class()])
        .build();
    tag.append(&icon(icon_name, 12));
    tag.append(&gtk::Label::new(Some(text)));
    tag
}

/// Shows a short message at the bottom of the window of `widget`.
pub fn toast(widget: &impl IsA<gtk::Widget>, text: &str) {
    if let Some(overlay) = widget
        .ancestor(adw::ToastOverlay::static_type())
        .and_downcast::<adw::ToastOverlay>()
    {
        let toast = adw::Toast::new(text);
        toast.set_timeout(4);
        overlay.add_toast(toast);
    }
}

/// Removes every child of a box.
pub fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

/// A level meter of the design: small blocks, green when lit.
///
/// Built from boxes rather than a `gtk::LevelBar`, so each block has the
/// size and the colors of the mockup.
pub struct Meter {
    /// The meter, to put in a page.
    pub root: gtk::Box,
    blocks: Vec<gtk::Box>,
    lit: Cell<usize>,
}

impl Meter {
    /// A meter of `count` blocks, all off.
    pub fn new(count: usize) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(3)
            .css_classes(["dc-meter"])
            .valign(gtk::Align::Center)
            .build();
        let blocks: Vec<gtk::Box> = (0..count)
            .map(|_| {
                let block = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                root.append(&block);
                block
            })
            .collect();
        Self {
            root,
            blocks,
            lit: Cell::new(0),
        }
    }

    /// Lights the share `fraction` (0.0 to 1.0) of the blocks.
    pub fn set_fraction(&self, fraction: f32) {
        let lit = lit_blocks(fraction, self.blocks.len());
        if lit == self.lit.get() {
            return;
        }
        self.lit.set(lit);
        for (index, block) in self.blocks.iter().enumerate() {
            if index < lit {
                block.add_css_class("on");
            } else {
                block.remove_css_class("on");
            }
        }
    }
}

/// How many of `count` blocks a share `fraction` lights.
pub fn lit_blocks(fraction: f32, count: usize) -> usize {
    if fraction.is_nan() || fraction <= 0.0 {
        return 0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a meter has a few blocks; the share is clamped first"
    )]
    let lit = (fraction.min(1.0) * count as f32).round() as usize;
    lit.min(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_lights_the_nearest_number_of_blocks() {
        assert_eq!(lit_blocks(0.0, 8), 0);
        assert_eq!(lit_blocks(-1.0, 8), 0);
        assert_eq!(lit_blocks(f32::NAN, 8), 0);
        assert_eq!(lit_blocks(0.5, 8), 4);
        assert_eq!(lit_blocks(0.06, 16), 1);
        assert_eq!(lit_blocks(1.0, 16), 16);
        assert_eq!(lit_blocks(3.0, 16), 16);
    }
}
