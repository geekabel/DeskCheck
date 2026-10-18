//! The five sections of the sidebar, in the order of the design.

/// A section of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// What you hear, who hears you, and your screens, right now.
    Now,
    /// Every output, microphone and app.
    Audio,
    /// Every screen, with its number and color.
    Screens,
    /// Places like home and office, with their settings.
    Places,
    /// Checks, and fixes in one click.
    Diagnostic,
}

impl Section {
    /// All sections, in sidebar order.
    pub const ALL: [Self; 5] = [
        Self::Now,
        Self::Audio,
        Self::Screens,
        Self::Places,
        Self::Diagnostic,
    ];

    /// The name of its page in the stack.
    pub fn id(self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::Audio => "audio",
            Self::Screens => "screens",
            Self::Places => "places",
            Self::Diagnostic => "diagnostic",
        }
    }

    /// The title shown in the sidebar and the header bar.
    pub fn title(self) -> &'static str {
        match self {
            Self::Now => "Now",
            Self::Audio => "Audio",
            Self::Screens => "Screens",
            Self::Places => "Places",
            Self::Diagnostic => "Diagnostic",
        }
    }

    /// Its icon, from the design.
    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Now => "dc-now-symbolic",
            Self::Audio => "dc-audio-symbolic",
            Self::Screens => "dc-screen-symbolic",
            Self::Places => "dc-place-symbolic",
            Self::Diagnostic => "dc-diagnostic-symbolic",
        }
    }

    /// The section at a position in the sidebar.
    pub fn at(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_sections_in_the_order_of_the_design() {
        let titles: Vec<&str> = Section::ALL.iter().map(|s| s.title()).collect();
        assert_eq!(titles, ["Now", "Audio", "Screens", "Places", "Diagnostic"]);
        assert_eq!(Section::at(0), Some(Section::Now));
        assert_eq!(Section::at(4), Some(Section::Diagnostic));
        assert_eq!(Section::at(5), None);
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = Section::ALL.iter().map(|s| s.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 5);
    }
}
