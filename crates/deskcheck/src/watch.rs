//! `deskcheck watch`: print each change of the sound and the screens, live.
//!
//! The lines are built by pure functions from the change and the state read
//! again after it, so they are tested without a sound server or GNOME.

use std::time::Duration;

use deskcheck_config::Config;
use deskcheck_core::audio::AudioState;
use deskcheck_core::display::DisplayState;
use deskcheck_core::events::Change;

/// How long to gather a burst of changes before reading the state again.
pub const BURST: Duration = Duration::from_millis(150);

/// A list of names, or `none`.
fn names(items: Vec<String>) -> String {
    let mut unique: Vec<String> = Vec::new();
    for item in items {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    if unique.is_empty() {
        "none".to_owned()
    } else {
        unique.join(", ")
    }
}

/// Which devices are the defaults, in one sentence.
fn defaults(config: &Config, state: &AudioState) -> String {
    let hear = state.default_output_device().map_or_else(
        || "nothing".to_owned(),
        |o| format!("\"{}\"", config.output_label(o)),
    );
    let heard = state.default_input_device().map_or_else(
        || "nothing".to_owned(),
        |i| format!("\"{}\"", config.input_label(i)),
    );
    format!("You hear through {hear}, they hear you via {heard}.")
}

/// The line for a sound change, with the state read after it.
pub fn sound_line(change: &Change, config: &Config, state: &AudioState) -> String {
    match change {
        Change::SoundDevices => format!(
            "Sound devices changed. Outputs: {}. Microphones: {}.",
            names(
                state
                    .outputs
                    .iter()
                    .map(|o| config.output_label(o).to_owned())
                    .collect()
            ),
            names(
                state
                    .microphones()
                    .map(|i| config.input_label(i).into_owned())
                    .collect()
            )
        ),
        Change::SoundDefaults => format!("Defaults changed. {}", defaults(config, state)),
        Change::SoundStreams => format!(
            "Apps changed. Playing: {}. Recording: {}.",
            names(state.playbacks.iter().map(|p| p.app.name.clone()).collect()),
            names(
                state
                    .recordings
                    .iter()
                    .filter(|r| !r.level_meter)
                    .map(|r| r.app.name.clone())
                    .collect()
            )
        ),
        Change::Screens | Change::Lost { .. } => String::new(),
    }
}

/// The line for a screen change, with the screens read after it.
pub fn screens_line(config: &Config, state: &DisplayState) -> String {
    let on = state.numbered();
    let list = on
        .iter()
        .map(|(number, screen)| format!("{number} {}", config.screen_label(screen)))
        .collect::<Vec<_>>()
        .join(", ");
    match on.len() {
        0 => "Screens changed: none is on.".to_owned(),
        1 => format!("Screens changed: 1 on ({list})."),
        count => format!("Screens changed: {count} on ({list})."),
    }
}

/// The last line printed for each kind of change. A device change often
/// comes with no visible difference (a stream starting changes the state
/// of its device), so a line equal to the last one of its kind is skipped.
#[derive(Debug, Default)]
pub struct LastLines(Vec<(Change, String)>);

impl LastLines {
    /// `true` when `line` differs from the last line of this kind; it then
    /// becomes the last one.
    pub fn is_new(&mut self, change: &Change, line: &str) -> bool {
        match self.0.iter_mut().find(|(kind, _)| kind == change) {
            Some((_, last)) if last == line => false,
            Some((_, last)) => {
                line.clone_into(last);
                true
            }
            None => {
                self.0.push((change.clone(), line.to_owned()));
                true
            }
        }
    }
}

/// The line when a source of changes stops.
pub fn lost_line(source: &str, reason: &str) -> String {
    format!("Stopped watching the {source}: {reason}")
}

/// Time since the start, like `+12.3 s`.
pub fn elapsed(time: Duration) -> String {
    format!("+{:.1} s", time.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{HEADSET, SPEAKERS, audio, screens};
    use deskcheck_core::audio::{App, DeviceId, Playback, Recording, StreamId, Volume};

    #[test]
    fn device_changes_list_the_devices() {
        assert_eq!(
            sound_line(&Change::SoundDevices, &Config::default(), &audio()),
            "Sound devices changed. Outputs: Built-in Audio Analog Stereo, \
             Jabra EVOLVE LINK Analog Stereo. Microphones: Built-in Audio Analog Stereo."
        );
    }

    #[test]
    fn default_changes_use_the_given_names() {
        let mut config = Config::default();
        config.set_audio_name(&DeviceId::new(SPEAKERS), Some("Laptop speakers".to_owned()));
        let mut state = audio();
        state.default_input = None;
        assert_eq!(
            sound_line(&Change::SoundDefaults, &config, &state),
            "Defaults changed. You hear through \"Laptop speakers\", they hear you via nothing."
        );
    }

    #[test]
    fn app_changes_list_each_app_once_without_meters() {
        let mut state = audio();
        let firefox = App {
            name: "Firefox".to_owned(),
            ..App::default()
        };
        let playback = |index| Playback {
            id: StreamId::new(index),
            app: firefox.clone(),
            media: None,
            output: DeviceId::new(HEADSET),
            volume: Volume::NORMAL,
            muted: false,
        };
        state.playbacks = vec![playback(1), playback(2)];
        state.recordings = vec![Recording {
            id: StreamId::new(3),
            app: App {
                name: "GNOME Settings".to_owned(),
                ..App::default()
            },
            media: None,
            input: DeviceId::new(SPEAKERS),
            volume: Volume::NORMAL,
            muted: false,
            level_meter: true,
        }];
        assert_eq!(
            sound_line(&Change::SoundStreams, &Config::default(), &state),
            "Apps changed. Playing: Firefox. Recording: none."
        );
    }

    #[test]
    fn screen_changes_list_the_screens_that_are_on() {
        assert_eq!(
            screens_line(&Config::default(), &screens()),
            "Screens changed: 2 on (1 Built-in display, 2 ASUSTek COMPUTER INC 27\")."
        );
        assert_eq!(
            screens_line(&Config::default(), &DisplayState::default()),
            "Screens changed: none is on."
        );
    }

    #[test]
    fn a_line_equal_to_the_last_of_its_kind_is_skipped() {
        let mut last = LastLines::default();
        assert!(last.is_new(&Change::SoundDevices, "a"));
        assert!(!last.is_new(&Change::SoundDevices, "a"));
        assert!(last.is_new(&Change::SoundStreams, "a"));
        assert!(last.is_new(&Change::SoundDevices, "b"));
        assert!(last.is_new(&Change::SoundDevices, "a"));
    }

    #[test]
    fn times_and_losses_read_well() {
        assert_eq!(elapsed(Duration::from_millis(12_340)), "+12.3 s");
        assert_eq!(
            lost_line("screens", "the connection to GNOME was closed"),
            "Stopped watching the screens: the connection to GNOME was closed"
        );
    }
}
