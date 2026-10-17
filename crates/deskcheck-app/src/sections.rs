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

    /// A symbolic icon of the Adwaita theme, present in Ubuntu 22.04.
    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Now => "go-home-symbolic",
            Self::Audio => "audio-speakers-symbolic",
            Self::Screens => "video-display-symbolic",
            Self::Places => "mark-location-symbolic",
            Self::Diagnostic => "emblem-ok-symbolic",
        }
    }

    /// One line on what the page is for, until the page itself is built.
    pub fn summary(self) -> &'static str {
        match self {
            Self::Now => {
                "Where the sound goes, which microphone hears you, and which screen you are on."
            }
            Self::Audio => "Every output, microphone and app, with a test for each.",
            Self::Screens => "Every screen, with its number and color.",
            Self::Places => "Home, office: the settings of each place, applied when you arrive.",
            Self::Diagnostic => "Every check, and a fix in one click.",
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
