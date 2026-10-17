//! Live changes, from the sound server and from GNOME.
//!
//! Both backends send a [`Change`] on the same channel
//! (`std::sync::mpsc::Sender<Change>`), so the interface waits on one place.
//! A change says *what* changed, not the new state: the interface reads the
//! state again, which keeps one source of truth.

/// Something changed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Change {
    /// A sound output or input was plugged, unplugged or changed (volume,
    /// mute, port).
    SoundDevices,
    /// An app started, stopped or moved a sound stream.
    SoundStreams,
    /// The default output or input changed.
    SoundDefaults,
    /// Screens were plugged, unplugged, turned on or off, or moved.
    Screens,
    /// A source of changes stopped, with the reason. No more changes will
    /// come from it.
    Lost {
        /// `sound` or `screens`.
        source: &'static str,
        /// Why it stopped.
        reason: String,
    },
}

impl Change {
    /// `true` for a change of the sound state.
    #[must_use]
    pub fn is_sound(&self) -> bool {
        matches!(
            self,
            Self::SoundDevices | Self::SoundStreams | Self::SoundDefaults
        )
    }
}

/// Keeps each change once, in the order they first came. Servers often send
/// a burst of the same change (a volume slider sends many), and the
/// interface needs to read the state again only once for it.
///
/// ```
/// use deskcheck_core::events::{Change, coalesce};
///
/// let burst = [Change::SoundDevices, Change::Screens, Change::SoundDevices];
/// assert_eq!(coalesce(burst), [Change::SoundDevices, Change::Screens]);
/// ```
pub fn coalesce(changes: impl IntoIterator<Item = Change>) -> Vec<Change> {
    let mut kept: Vec<Change> = Vec::new();
    for change in changes {
        if !kept.contains(&change) {
            kept.push(change);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_changes_are_told_apart() {
        assert!(Change::SoundStreams.is_sound());
        assert!(!Change::Screens.is_sound());
        assert!(
            !Change::Lost {
                source: "sound",
                reason: String::new()
            }
            .is_sound()
        );
    }

    #[test]
    fn a_burst_becomes_one_change_each() {
        let burst = vec![Change::SoundDevices; 12];
        assert_eq!(coalesce(burst), [Change::SoundDevices]);
        assert_eq!(coalesce([]), []);
    }
}
