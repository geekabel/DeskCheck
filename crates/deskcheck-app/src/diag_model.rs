//! What the Diagnostic page shows: for each problem the user can have, a
//! short list of checks, each with its state and, when DeskCheck can, a
//! fix in one click. Pure, so it is tested without a display.
//!
//! The checks come from the state (with the rules of
//! [`deskcheck_core::audio::diagnose`]), from what the microphone heard,
//! and from the answers of the user to what DeskCheck cannot see.

use std::time::Duration;

use deskcheck_config::Config;
use deskcheck_core::audio::{
    AudioState, DeviceId, Finding, Fix, Problem, StreamId, best_microphone, diagnose,
};
use deskcheck_core::display::DisplayState;

use crate::audio_model::AudioTab;

/// The problems the page helps with, as in the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// "They can't hear me", in a call.
    NotHeard,
    /// "I hear nothing".
    HearNothing,
    /// "A screen is not detected".
    ScreenMissing,
}

impl Scenario {
    /// The three scenarios, the first one shown first.
    pub const ALL: [Self; 3] = [Self::NotHeard, Self::HearNothing, Self::ScreenMissing];

    /// The title of the page, and the text of the button that opens it.
    pub fn title(self) -> &'static str {
        match self {
            Self::NotHeard => "They can't hear me",
            Self::HearNothing => "I hear nothing",
            Self::ScreenMissing => "A screen is not detected",
        }
    }

    /// The line under the title.
    pub fn subtitle(self, checks: usize) -> String {
        match self {
            Self::NotHeard | Self::HearNothing => {
                format!("{checks} checks. Stay in your call: nothing will be cut off.")
            }
            Self::ScreenMissing => format!("{checks} checks. Nothing changes on your screens."),
        }
    }
}

/// Where a check stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// All is well.
    Ok,
    /// Something is wrong.
    Problem,
    /// DeskCheck cannot see it: the user answers.
    Question,
    /// It waits for something to happen (a voice, a call).
    Waiting,
    /// It waits for the checks before it.
    Locked,
}

impl Status {
    /// `true` once the check is decided, for the progress bar.
    pub fn is_decided(self) -> bool {
        matches!(self, Self::Ok | Self::Problem)
    }
}

/// A question DeskCheck asks, because it cannot see the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    /// Does the site have the right to use the microphone?
    SitePermission,
    /// Did the user hear the test sound?
    HeardChime,
    /// Are the cable and the input of the screen right?
    Cable,
}

/// What a button of a check does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// Applies fixes of the diagnosis.
    Fix(Vec<Fix>),
    /// Moves apps that play to another output.
    MovePlaybacks(Vec<(StreamId, DeviceId)>),
    /// Answers a question.
    Answer(Question, bool),
    /// Forgets the answer to a question, to ask it again.
    Ask(Question),
    /// Records 3 seconds from the microphone, then plays them back.
    RecordTest,
    /// Plays the test sound on the output.
    PlayChime,
    /// Opens a tab of the Audio page.
    OpenAudio(AudioTab),
}

/// How a button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// White with a border.
    Normal,
    /// Filled in orange: the fix of the problem.
    Fix,
}

/// A button of a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    /// Its text.
    pub label: String,
    /// What it does.
    pub act: Act,
    /// How it looks.
    pub look: Look,
}

impl Button {
    fn new(label: impl Into<String>, act: Act, look: Look) -> Self {
        Self {
            label: label.into(),
            act,
            look,
        }
    }
}

/// One check of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What is checked, without its number.
    pub title: String,
    /// What DeskCheck found.
    pub detail: String,
    /// Where it stands.
    pub status: Status,
    /// Its buttons.
    pub buttons: Vec<Button>,
    /// A short line after the buttons.
    pub note: Option<String>,
    /// The technical facts, in a small mono line.
    pub technical: Option<String>,
    /// `true` for the check that shows the level of the microphone.
    pub meter: bool,
}

impl Check {
    fn new(title: &str, status: Status, detail: impl Into<String>) -> Self {
        Self {
            title: title.to_owned(),
            detail: detail.into(),
            status,
            buttons: Vec::new(),
            note: None,
            technical: None,
            meter: false,
        }
    }

    /// The title shown: numbered, with ": no" when it found a problem.
    pub fn heading(&self, number: usize) -> String {
        if self.status == Status::Problem {
            format!("{number}. {}: no", self.title)
        } else {
            format!("{number}. {}", self.title)
        }
    }
}

/// What the user told DeskCheck, and what the microphone heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Answers {
    /// The site may use the microphone (`Some(true)`), or not.
    pub site: Option<bool>,
    /// The final recording was made.
    pub recorded: bool,
    /// The test sound was played.
    pub chime_played: bool,
    /// The user heard the test sound, or not.
    pub chime: Option<bool>,
    /// The cable was checked and the screen still missing (`Some(false)`),
    /// or it works now (`Some(true)`).
    pub cable: Option<bool>,
    /// How long ago the microphone last heard a voice.
    pub voice_ago: Option<Duration>,
}

impl Answers {
    /// Takes the answer to a question.
    pub fn answer(&mut self, question: Question, yes: bool) {
        match question {
            Question::SitePermission => self.site = Some(yes),
            Question::HeardChime => self.chime = Some(yes),
            Question::Cable => self.cable = Some(yes),
        }
    }

    /// Forgets the answer to a question.
    pub fn ask(&mut self, question: Question) {
        match question {
            Question::SitePermission => self.site = None,
            Question::HeardChime => {
                self.chime = None;
                self.chime_played = false;
            }
            Question::Cable => self.cable = None,
        }
    }
}

/// The checks of a scenario.
pub fn checks(
    scenario: Scenario,
    config: &Config,
    sound: Option<&AudioState>,
    screens: Option<&Result<DisplayState, String>>,
    answers: &Answers,
) -> Vec<Check> {
    match scenario {
        Scenario::NotHeard => {
            sound.map_or_else(no_sound, |state| not_heard(config, state, answers))
        }
        Scenario::HearNothing => {
            sound.map_or_else(no_sound, |state| hear_nothing(config, state, answers))
        }
        Scenario::ScreenMissing => screen_missing(config, screens, answers),
    }
}

/// The progress line: `3 of 5 · 1 problem found`.
pub fn progress(checks: &[Check]) -> (f64, String) {
    let decided = checks.iter().filter(|c| c.status.is_decided()).count();
    let problems = checks
        .iter()
        .filter(|c| c.status == Status::Problem)
        .count();
    let found = match problems {
        0 => "no problem found".to_owned(),
        1 => "1 problem found".to_owned(),
        n => format!("{n} problems found"),
    };
    #[allow(clippy::cast_precision_loss, reason = "a handful of checks")]
    let fraction = if checks.is_empty() {
        0.0
    } else {
        decided as f64 / checks.len() as f64
    };
    (fraction, format!("{decided} of {} · {found}", checks.len()))
}

fn no_sound() -> Vec<Check> {
    vec![Check::new(
        "A sound server answers",
        Status::Problem,
        "No sound server answers. Is PulseAudio or PipeWire running?",
    )]
}

fn percent(volume: deskcheck_core::audio::Volume) -> String {
    format!("{} %", volume.percent())
}

/// When the user last spoke, in words.
pub fn spoke_ago(ago: Duration) -> String {
    match ago.as_secs() {
        0 => "You spoke just now.".to_owned(),
        s @ 1..60 => format!("You spoke {s} s ago."),
        s => format!("You spoke {} min ago.", s / 60),
    }
}

/// The name of a device for a sentence.
fn input_name(config: &Config, state: &AudioState, id: &DeviceId) -> String {
    state.input(id).map_or_else(
        || id.to_string(),
        |input| config.input_label(input).into_owned(),
    )
}

fn output_name(config: &Config, state: &AudioState, id: &DeviceId) -> String {
    state.output(id).map_or_else(
        || id.to_string(),
        |output| config.output_label(output).to_owned(),
    )
}

/// "They can't hear me": muted, voice, the app, the site, a last test.
fn not_heard(config: &Config, state: &AudioState, answers: &Answers) -> Vec<Check> {
    let findings = diagnose(state);
    let mut checks = Vec::new();

    // 1. The microphone is on.
    let first = microphone_on(config, state, &findings);
    let mic_ok = first.status == Status::Ok;
    checks.push(first);

    // 2. It hears the voice.
    let title = "The microphone hears your voice";
    let mut voice = match (mic_ok, answers.voice_ago) {
        (false, _) => Check::new(title, Status::Locked, "Once the microphone is on."),
        (true, Some(ago)) => Check::new(title, Status::Ok, spoke_ago(ago)),
        (true, None) => Check::new(
            title,
            Status::Waiting,
            "Speak out loud: the bar moves when it hears you.",
        ),
    };
    voice.meter = mic_ok;
    checks.push(voice);

    // 3. The app listens to a microphone.
    checks.push(app_listens(config, state, &findings));

    // 4. The site may use it.
    checks.push(site_allowed(answers));

    // 5. A last test, once nothing is wrong.
    let last = final_test(state, &checks, answers);
    checks.push(last);
    checks
}

/// Check 3 of "They can't hear me": no app listens to a monitor.
fn app_listens(config: &Config, state: &AudioState, findings: &[Finding]) -> Check {
    let title = "The app listens to the right microphone";
    let wrong: Vec<_> = findings
        .iter()
        .filter(|f| {
            matches!(
                f.problem,
                Problem::RecordsMonitor { .. } | Problem::DefaultInputIsMonitor { .. }
            )
        })
        .collect();
    if let Some(finding) = wrong.first() {
        let mic = best_microphone(state).map(|mic| config.input_label(mic).into_owned());
        let fixes: Vec<Fix> = wrong.iter().filter_map(|f| f.fix.clone()).collect();
        let (detail, button, technical) = match &finding.problem {
            Problem::RecordsMonitor {
                stream,
                app,
                monitor,
            } => (
                format!(
                    "{app} listens to \"{}\". It is not a microphone: it copies the sound of an \
                     output. The others hear silence.",
                    input_name(config, state, monitor)
                ),
                mic.as_ref().map(|mic| format!("Switch {app} to {mic}")),
                Some(format!("{app} · source-output {stream} → {monitor}")),
            ),
            Problem::DefaultInputIsMonitor { monitor } => (
                format!(
                    "The default microphone is \"{}\". It is not a microphone: new calls would \
                     hear the speakers.",
                    input_name(config, state, monitor)
                ),
                mic.as_ref().map(|mic| format!("Use {mic} instead")),
                Some(format!("default source → {monitor}")),
            ),
            _ => (String::new(), None, None),
        };
        let mut check = Check::new(title, Status::Problem, detail);
        if let (Some(label), false) = (button, fixes.is_empty()) {
            check
                .buttons
                .push(Button::new(label, Act::Fix(fixes), Look::Fix));
            check.note = Some("Takes effect at once, without cutting the call.".to_owned());
        }
        check.technical = technical;
        return check;
    }
    let listening: Vec<String> = state
        .recordings
        .iter()
        .filter(|r| !r.level_meter)
        .map(|r| {
            format!(
                "{} listens to {}",
                r.app.name,
                input_name(config, state, &r.input)
            )
        })
        .collect();
    if listening.is_empty() {
        Check::new(
            title,
            Status::Waiting,
            "No app listens yet. Start your call: this check runs again by itself.",
        )
    } else {
        Check::new(title, Status::Ok, format!("{}.", listening.join(", ")))
    }
}

/// "I hear nothing": the output is on, apps play on it, a test sound.
fn hear_nothing(config: &Config, state: &AudioState, answers: &Answers) -> Vec<Check> {
    let findings = diagnose(state);
    let mut checks = Vec::new();

    // 1. The output is on.
    checks.push(output_on(config, state, &findings));

    // 2. Apps play on that output.
    checks.push(apps_play_there(config, state));

    // 3. The user hears the test sound.
    let third = test_sound(state, &checks, answers);
    checks.push(third);
    checks
}

/// Check 1 of "They can't hear me": the microphone is on.
fn microphone_on(config: &Config, state: &AudioState, findings: &[Finding]) -> Check {
    let mic = best_microphone(state);
    let title = "The microphone is not muted";
    let off: Vec<_> = findings
        .iter()
        .filter(|f| {
            matches!(
                f.problem,
                Problem::MicrophoneMuted { .. } | Problem::MicrophoneSilent { .. }
            )
        })
        .collect();
    if findings.iter().any(|f| f.problem == Problem::NoMicrophone) {
        Check::new(
            title,
            Status::Problem,
            "No microphone is plugged in. Plug in your headset or a microphone.",
        )
    } else if off.is_empty() {
        let detail = mic.map_or_else(String::new, |mic| {
            format!(
                "{}: volume {}, not muted in Ubuntu.",
                config.input_label(mic),
                percent(mic.volume)
            )
        });
        Check::new(title, Status::Ok, detail)
    } else {
        let names: Vec<String> = off
            .iter()
            .map(|f| match &f.problem {
                Problem::MicrophoneMuted { input } => {
                    format!("{} is muted", input_name(config, state, input))
                }
                Problem::MicrophoneSilent { input } => {
                    format!("{} is at 0 %", input_name(config, state, input))
                }
                _ => String::new(),
            })
            .collect();
        let mut check = Check::new(
            title,
            Status::Problem,
            format!("{}. Nobody can hear it.", names.join(", ")),
        );
        let fixes: Vec<Fix> = off.iter().filter_map(|f| f.fix.clone()).collect();
        check.buttons.push(Button::new(
            "Turn the microphone on",
            Act::Fix(fixes),
            Look::Fix,
        ));
        check
    }
}

/// Check 4 of "They can't hear me": the site may use the microphone.
fn site_allowed(answers: &Answers) -> Check {
    let title = "The site may use the microphone";
    match answers.site {
        None => {
            let mut check = Check::new(
                title,
                Status::Question,
                "DeskCheck cannot see it from here. The microphone icon in the address bar \
                 must not be crossed out.",
            );
            check.buttons.push(Button::new(
                "It is not crossed out",
                Act::Answer(Question::SitePermission, true),
                Look::Normal,
            ));
            check.buttons.push(Button::new(
                "It is crossed out",
                Act::Answer(Question::SitePermission, false),
                Look::Normal,
            ));
            check
        }
        Some(true) => Check::new(title, Status::Ok, "The site may use the microphone."),
        Some(false) => {
            let mut check = Check::new(
                title,
                Status::Problem,
                "Click the crossed-out microphone in the address bar, then allow it for this \
                 site. Reload the page if the browser asks.",
            );
            check.buttons.push(Button::new(
                "Done, check again",
                Act::Ask(Question::SitePermission),
                Look::Normal,
            ));
            check
        }
    }
}

/// Check 5 of "They can't hear me": record and listen, once nothing is
/// wrong.
fn final_test(state: &AudioState, checks: &[Check], answers: &Answers) -> Check {
    let mic = best_microphone(state);
    let title = "Final test";
    let ready = checks
        .iter()
        .all(|c| matches!(c.status, Status::Ok | Status::Waiting));
    let mut last = if answers.recorded {
        Check::new(
            title,
            Status::Ok,
            "Recording played back. If you heard yourself, they hear you too.",
        )
    } else {
        Check::new(
            title,
            if ready {
                Status::Waiting
            } else {
                Status::Locked
            },
            "Record 3 seconds with the chosen microphone, then listen to yourself.",
        )
    };
    // Shown greyed while the checks above are not fine, as in the design.
    if mic.is_some() && state.default_output.is_some() {
        last.buttons.push(Button::new(
            if answers.recorded {
                "Record again"
            } else {
                "Record"
            },
            Act::RecordTest,
            Look::Normal,
        ));
    }
    last
}

/// Check 1 of "I hear nothing": the output is on.
fn output_on(config: &Config, state: &AudioState, findings: &[Finding]) -> Check {
    let title = "The output is not muted";
    let off: Vec<_> = findings
        .iter()
        .filter(|f| {
            matches!(
                f.problem,
                Problem::OutputMuted { .. } | Problem::OutputSilent { .. }
            )
        })
        .collect();
    let output = state.default_output_device();
    if output.is_none() {
        Check::new(
            title,
            Status::Problem,
            "No output is chosen. Plug in a headset or speakers.",
        )
    } else if off.is_empty() {
        let output = output.map_or_else(String::new, |o| {
            format!(
                "{}: volume {}, not muted.",
                config.output_label(o),
                percent(o.volume)
            )
        });
        Check::new(title, Status::Ok, output)
    } else {
        let names: Vec<String> = off
            .iter()
            .map(|f| match &f.problem {
                Problem::OutputMuted { output } => {
                    format!("{} is muted", output_name(config, state, output))
                }
                Problem::OutputSilent { output } => {
                    format!("{} is at 0 %", output_name(config, state, output))
                }
                _ => String::new(),
            })
            .collect();
        let mut check = Check::new(title, Status::Problem, format!("{}.", names.join(", ")));
        let fixes: Vec<Fix> = off.iter().filter_map(|f| f.fix.clone()).collect();
        check
            .buttons
            .push(Button::new("Turn the sound on", Act::Fix(fixes), Look::Fix));
        check
    }
}

/// Check 2 of "I hear nothing": apps play on the chosen output.
fn apps_play_there(config: &Config, state: &AudioState) -> Check {
    let title = "Apps play on the chosen output";
    match &state.default_output {
        None => Check::new(title, Status::Locked, "Once an output is chosen."),
        Some(_) if state.playbacks.is_empty() => Check::new(
            title,
            Status::Waiting,
            "No app plays right now. Start a video or your call: this check runs again.",
        ),
        Some(default) => {
            let elsewhere: Vec<_> = state
                .playbacks
                .iter()
                .filter(|p| &p.output != default)
                .collect();
            let chosen = output_name(config, state, default);
            if elsewhere.is_empty() {
                let mut apps: Vec<&str> = state
                    .playbacks
                    .iter()
                    .map(|p| p.app.name.as_str())
                    .collect();
                apps.dedup();
                Check::new(
                    title,
                    Status::Ok,
                    format!("{} plays on {chosen}.", apps.join(", ")),
                )
            } else {
                let parts: Vec<String> = elsewhere
                    .iter()
                    .map(|p| {
                        format!(
                            "{} plays on {}",
                            p.app.name,
                            output_name(config, state, &p.output)
                        )
                    })
                    .collect();
                let mut check = Check::new(
                    title,
                    Status::Problem,
                    format!("{}, not on {chosen}.", parts.join(", ")),
                );
                let moves = elsewhere.iter().map(|p| (p.id, default.clone())).collect();
                check.buttons.push(Button::new(
                    format!("Move to {chosen}"),
                    Act::MovePlaybacks(moves),
                    Look::Fix,
                ));
                check.note = Some("The sound moves at once, without stopping.".to_owned());
                check
            }
        }
    }
}

/// Check 3 of "I hear nothing": the user hears the test sound.
fn test_sound(state: &AudioState, checks: &[Check], answers: &Answers) -> Check {
    let title = "You hear the test sound";
    let ready =
        state.default_output.is_some() && checks.iter().all(|c| c.status != Status::Problem);
    match (ready, answers.chime_played, answers.chime) {
        (false, _, _) => Check::new(title, Status::Locked, "Once the checks above are fine."),
        (true, _, Some(true)) => Check::new(
            title,
            Status::Ok,
            "You hear the sound of the chosen output.",
        ),
        (true, _, Some(false)) => {
            let mut check = Check::new(
                title,
                Status::Problem,
                "The sound goes somewhere you are not listening. Choose the output you wear \
                 or the speakers near you.",
            );
            check.buttons.push(Button::new(
                "Choose another output",
                Act::OpenAudio(AudioTab::Outputs),
                Look::Fix,
            ));
            check.buttons.push(Button::new(
                "Try again",
                Act::Ask(Question::HeardChime),
                Look::Normal,
            ));
            check
        }
        (true, false, None) => {
            let mut check = Check::new(
                title,
                Status::Waiting,
                "Play two short notes on the chosen output.",
            );
            check.buttons.push(Button::new(
                "Play the test sound",
                Act::PlayChime,
                Look::Normal,
            ));
            check
        }
        (true, true, None) => {
            let mut check = Check::new(title, Status::Question, "Did you hear two notes?");
            check.buttons.push(Button::new(
                "Yes",
                Act::Answer(Question::HeardChime, true),
                Look::Normal,
            ));
            check.buttons.push(Button::new(
                "No",
                Act::Answer(Question::HeardChime, false),
                Look::Normal,
            ));
            check
                .buttons
                .push(Button::new("Play again", Act::PlayChime, Look::Normal));
            check
        }
    }
}

/// "A screen is not detected": GNOME sees it, it is on, the cable.
fn screen_missing(
    config: &Config,
    screens: Option<&Result<DisplayState, String>>,
    answers: &Answers,
) -> Vec<Check> {
    let mut checks = Vec::new();
    let title = "GNOME sees your screens";
    let state = match screens {
        None => {
            checks.push(Check::new(title, Status::Waiting, "Reading the screens…"));
            return checks;
        }
        Some(Err(_)) => {
            checks.push(Check::new(
                title,
                Status::Problem,
                "Screens can be read only in a GNOME session.",
            ));
            return checks;
        }
        Some(Ok(state)) => state,
    };
    let seen: Vec<String> = state
        .numbered()
        .iter()
        .map(|(number, screen)| format!("{number} {}", config.screen_label(screen)))
        .collect();
    let count = state.screens.len();
    let plural = if count == 1 { "screen" } else { "screens" };
    let mut detail = format!("GNOME sees {count} {plural}");
    if !seen.is_empty() {
        let _ = std::fmt::Write::write_fmt(&mut detail, format_args!(": {}", seen.join(", ")));
    }
    detail.push('.');
    checks.push(Check::new(title, Status::Ok, detail));

    let title = "Every screen plugged in is on";
    let off: Vec<&str> = state
        .screens
        .iter()
        .filter(|s| !s.is_enabled())
        .map(|s| config.screen_label(s))
        .collect();
    checks.push(if off.is_empty() {
        Check::new(title, Status::Ok, "Every screen GNOME sees is on.")
    } else {
        Check::new(
            title,
            Status::Problem,
            format!(
                "{} is plugged in but off. Turn it on in Settings, Displays.",
                off.join(", ")
            ),
        )
    });

    let title = "The cable and the input of the screen";
    let mut cable = match answers.cable {
        None => Check::new(
            title,
            Status::Question,
            "DeskCheck cannot see a screen GNOME does not see. Is the cable plugged in at both \
             ends, and is the screen on the right input (HDMI, DisplayPort)?",
        ),
        Some(true) => Check::new(title, Status::Ok, "The screen works now."),
        Some(false) => Check::new(
            title,
            Status::Problem,
            "GNOME still sees nothing there. Try another cable or another port. A docking \
             station may need its own power.",
        ),
    };
    match answers.cable {
        None => {
            cable.buttons.push(Button::new(
                "It works now",
                Act::Answer(Question::Cable, true),
                Look::Normal,
            ));
            cable.buttons.push(Button::new(
                "Checked, still nothing",
                Act::Answer(Question::Cable, false),
                Look::Normal,
            ));
        }
        Some(_) => cable.buttons.push(Button::new(
            "Check again",
            Act::Ask(Question::Cable),
            Look::Normal,
        )),
    }
    checks.push(cable);
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{
        App, Connection, Form, Hardware, Input, InputKind, Output, Playback, Recording, Volume,
    };
    use deskcheck_core::display::{Mode, Placement, Screen, ScreenId};

    fn app(name: &str) -> App {
        App {
            name: name.to_owned(),
            ..App::default()
        }
    }

    /// The office of the design: the headset mic is fine, Firefox listens
    /// to the monitor of the Dell screen.
    fn office() -> AudioState {
        let headset = Hardware {
            form: Form::Headset,
            connection: Connection::Usb,
        };
        AudioState {
            outputs: vec![
                Output {
                    id: DeviceId::new("headset"),
                    description: "USB Headset".to_owned(),
                    hardware: headset,
                    volume: Volume::from_percent(62),
                    muted: false,
                },
                Output {
                    id: DeviceId::new("dell"),
                    description: "Dell screen".to_owned(),
                    hardware: Hardware::default(),
                    volume: Volume::NORMAL,
                    muted: false,
                },
            ],
            inputs: vec![
                Input {
                    id: DeviceId::new("headset.mic"),
                    description: "Headset mic".to_owned(),
                    hardware: headset,
                    kind: InputKind::Capture,
                    volume: Volume::from_percent(85),
                    muted: false,
                },
                Input {
                    id: DeviceId::new("dell.monitor"),
                    description: "Monitor of Dell screen".to_owned(),
                    hardware: Hardware::default(),
                    kind: InputKind::Monitor {
                        of: DeviceId::new("dell"),
                    },
                    volume: Volume::NORMAL,
                    muted: false,
                },
            ],
            playbacks: vec![Playback {
                id: StreamId::new(3),
                app: app("Firefox"),
                media: None,
                output: DeviceId::new("headset"),
                volume: Volume::NORMAL,
                muted: false,
            }],
            recordings: vec![Recording {
                id: StreamId::new(42),
                app: app("Firefox"),
                media: None,
                input: DeviceId::new("dell.monitor"),
                volume: Volume::NORMAL,
                muted: false,
                level_meter: false,
            }],
            default_output: Some(DeviceId::new("headset")),
            default_input: Some(DeviceId::new("headset.mic")),
        }
    }

    fn statuses(checks: &[Check]) -> Vec<Status> {
        checks.iter().map(|c| c.status).collect()
    }

    #[test]
    fn the_office_case_of_the_design() {
        let answers = Answers {
            voice_ago: Some(Duration::from_secs(2)),
            ..Answers::default()
        };
        let checks = checks(
            Scenario::NotHeard,
            &Config::default(),
            Some(&office()),
            None,
            &answers,
        );
        assert_eq!(
            statuses(&checks),
            [
                Status::Ok,
                Status::Ok,
                Status::Problem,
                Status::Question,
                Status::Locked
            ]
        );
        assert_eq!(
            checks[0].detail,
            "Headset mic: volume 85 %, not muted in Ubuntu."
        );
        assert_eq!(checks[1].detail, "You spoke 2 s ago.");
        assert!(checks[1].meter);
        let app = &checks[2];
        assert_eq!(
            app.heading(3),
            "3. The app listens to the right microphone: no"
        );
        assert!(
            app.detail
                .starts_with("Firefox listens to \"Monitor of Dell screen\".")
        );
        assert_eq!(app.buttons[0].label, "Switch Firefox to Headset mic");
        assert_eq!(
            app.buttons[0].act,
            Act::Fix(vec![Fix::MoveRecording {
                stream: StreamId::new(42),
                to: DeviceId::new("headset.mic"),
            }])
        );
        assert_eq!(
            app.technical.as_deref(),
            Some("Firefox · source-output #42 → dell.monitor")
        );
        // The record button waits, greyed, while a check is wrong.
        assert_eq!(checks[4].buttons[0].act, Act::RecordTest);
        assert_eq!(progress(&checks).1, "3 of 5 · 1 problem found");
    }

    #[test]
    fn once_all_is_fine_the_final_test_opens() {
        let mut state = office();
        state.recordings[0].input = DeviceId::new("headset.mic");
        let mut answers = Answers {
            site: Some(true),
            ..Answers::default()
        };
        let list = checks(
            Scenario::NotHeard,
            &Config::default(),
            Some(&state),
            None,
            &answers,
        );
        assert_eq!(list[1].status, Status::Waiting);
        assert_eq!(list[2].detail, "Firefox listens to Headset mic.");
        assert_eq!(list[4].status, Status::Waiting);
        assert_eq!(list[4].buttons[0].act, Act::RecordTest);
        answers.recorded = true;
        let list = checks(
            Scenario::NotHeard,
            &Config::default(),
            Some(&state),
            None,
            &answers,
        );
        assert_eq!(list[4].status, Status::Ok);
    }

    #[test]
    fn a_muted_microphone_locks_the_voice_check() {
        let mut state = office();
        state.inputs[0].muted = true;
        state.inputs[0].volume = Volume::from_percent(0);
        let list = checks(
            Scenario::NotHeard,
            &Config::default(),
            Some(&state),
            None,
            &Answers::default(),
        );
        assert_eq!(list[0].status, Status::Problem);
        assert_eq!(
            list[0].detail,
            "Headset mic is muted, Headset mic is at 0 %. Nobody can hear it."
        );
        let Act::Fix(fixes) = &list[0].buttons[0].act else {
            panic!("a fix");
        };
        assert_eq!(fixes.len(), 2);
        assert_eq!(list[1].status, Status::Locked);
        assert!(!list[1].meter);
    }

    #[test]
    fn hear_nothing_moves_apps_and_asks_about_the_sound() {
        let mut state = office();
        state.playbacks[0].output = DeviceId::new("dell");
        let mut answers = Answers::default();
        let list = checks(
            Scenario::HearNothing,
            &Config::default(),
            Some(&state),
            None,
            &answers,
        );
        assert_eq!(
            statuses(&list),
            [Status::Ok, Status::Problem, Status::Locked]
        );
        assert_eq!(
            list[1].detail,
            "Firefox plays on Dell screen, not on USB Headset."
        );
        assert_eq!(
            list[1].buttons[0].act,
            Act::MovePlaybacks(vec![(StreamId::new(3), DeviceId::new("headset"))])
        );

        state.playbacks[0].output = DeviceId::new("headset");
        let list = checks(
            Scenario::HearNothing,
            &Config::default(),
            Some(&state),
            None,
            &answers,
        );
        assert_eq!(list[2].buttons[0].act, Act::PlayChime);
        answers.chime_played = true;
        let list = checks(
            Scenario::HearNothing,
            &Config::default(),
            Some(&state),
            None,
            &answers,
        );
        assert_eq!(list[2].status, Status::Question);
        answers.answer(Question::HeardChime, false);
        let list = checks(
            Scenario::HearNothing,
            &Config::default(),
            Some(&state),
            None,
            &answers,
        );
        assert_eq!(list[2].buttons[0].act, Act::OpenAudio(AudioTab::Outputs));
        answers.ask(Question::HeardChime);
        assert_eq!((answers.chime, answers.chime_played), (None, false));
    }

    #[test]
    fn a_screen_that_is_off_is_found() {
        let screen = |connector: &str, on: bool| Screen {
            id: ScreenId::new("DEL", connector, ""),
            connector: connector.to_owned(),
            display_name: Some(format!("Screen {connector}")),
            builtin: false,
            current_mode: on.then_some(Mode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
            }),
            placement: on.then_some(Placement {
                x: 0,
                y: 0,
                scale: 1.0,
                primary: true,
            }),
        };
        let state = DisplayState {
            screens: vec![screen("DP-1", true), screen("DP-2", false)],
        };
        let list = checks(
            Scenario::ScreenMissing,
            &Config::default(),
            None,
            Some(&Ok(state)),
            &Answers::default(),
        );
        assert_eq!(
            statuses(&list),
            [Status::Ok, Status::Problem, Status::Question]
        );
        assert_eq!(list[0].detail, "GNOME sees 2 screens: 1 Screen DP-1.");
        assert!(
            list[1]
                .detail
                .starts_with("Screen DP-2 is plugged in but off.")
        );
    }

    #[test]
    fn words_for_the_last_voice() {
        assert_eq!(spoke_ago(Duration::from_millis(300)), "You spoke just now.");
        assert_eq!(spoke_ago(Duration::from_secs(12)), "You spoke 12 s ago.");
        assert_eq!(spoke_ago(Duration::from_secs(130)), "You spoke 2 min ago.");
        assert_eq!(
            Scenario::NotHeard.subtitle(5),
            "5 checks. Stay in your call: nothing will be cut off."
        );
    }
}
