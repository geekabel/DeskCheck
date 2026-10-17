//! `deskcheck doctor`: show the sound problems, and fix them with `--fix`.
//!
//! The text is built by pure functions from the findings and, after
//! `--fix`, from what happened to each fix. Applying the fixes is done by
//! the caller, so the report is tested without a sound server.

use std::fmt::Write as _;

use deskcheck_config::Config;
use deskcheck_core::audio::{AudioState, Finding, Fix};

use crate::audio_report::{describe, fix_text, problem_text};

/// What happened to the fix of one finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The fix was applied.
    Fixed,
    /// The fix was tried and failed, with the reason.
    Failed(String),
    /// Only the user can do it, like plugging in a microphone.
    Manual,
    /// There is no fix.
    NoFix,
}

/// The fixes DeskCheck can apply, each once, in the order of the findings.
pub fn automatic_fixes(findings: &[Finding]) -> Vec<&Fix> {
    let mut fixes: Vec<&Fix> = Vec::new();
    for fix in findings.iter().filter_map(|finding| finding.fix.as_ref()) {
        if fix.is_automatic() && !fixes.contains(&fix) {
            fixes.push(fix);
        }
    }
    fixes
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// The report without `--fix`: the problems, and how to fix them.
pub fn render_check(config: &Config, state: &AudioState, findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "No problem found.\n".to_owned();
    }
    let mut out = String::from("Problems\n");
    for finding in findings {
        let _ = writeln!(out, "  ! {}", describe(config, state, finding));
    }
    let count = automatic_fixes(findings).len();
    if count > 0 {
        let _ = writeln!(
            out,
            "\nRun `deskcheck doctor --fix` to apply {}.",
            plural(count, "fix", "fixes")
        );
    }
    out
}

/// The report after `--fix`: each problem with what happened to its fix,
/// then how many problems are left.
///
/// `state` is the state before the fixes, so the names match the problems.
/// `left` is the diagnosis after the fixes.
pub fn render_fixed(
    config: &Config,
    state: &AudioState,
    findings: &[Finding],
    outcomes: &[Outcome],
    left: &[Finding],
) -> String {
    let mut out = String::from("Problems\n");
    for (finding, outcome) in findings.iter().zip(outcomes) {
        let _ = writeln!(out, "  ! {}", problem_text(config, state, &finding.problem));
        let fix = finding.fix.as_ref().map(|fix| fix_text(config, state, fix));
        match (outcome, fix) {
            (Outcome::Fixed, Some(fix)) => {
                let _ = writeln!(out, "    Fixed: {fix}");
            }
            (Outcome::Failed(reason), Some(fix)) => {
                let _ = writeln!(out, "    Not fixed: {fix}\n    {reason}");
            }
            (Outcome::Manual, Some(fix)) => {
                let _ = writeln!(out, "    To do yourself: {fix}");
            }
            _ => {
                let _ = writeln!(out, "    No fix.");
            }
        }
    }
    if left.is_empty() {
        out.push_str("\nAfter the fixes: no problem left.\n");
    } else {
        let _ = writeln!(
            out,
            "\nAfter the fixes: {} left. Run `deskcheck doctor` to see {}.",
            plural(left.len(), "problem", "problems"),
            if left.len() == 1 { "it" } else { "them" }
        );
    }
    out
}

/// Applies the fixes with `apply`, each once, and returns the outcome of
/// every finding.
pub fn fix_all(
    findings: &[Finding],
    mut apply: impl FnMut(&Fix) -> Result<(), String>,
) -> Vec<Outcome> {
    let mut done: Vec<(&Fix, Outcome)> = Vec::new();
    findings
        .iter()
        .map(|finding| match &finding.fix {
            None => Outcome::NoFix,
            Some(fix) if !fix.is_automatic() => Outcome::Manual,
            Some(fix) => {
                if let Some((_, outcome)) = done.iter().find(|(f, _)| *f == fix) {
                    return outcome.clone();
                }
                let outcome = match apply(fix) {
                    Ok(()) => Outcome::Fixed,
                    Err(reason) => Outcome::Failed(reason),
                };
                done.push((fix, outcome.clone()));
                outcome
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskcheck_core::audio::{
        App, DeviceId, Hardware, Input, InputKind, Output, Recording, StreamId, Volume, diagnose,
    };

    const SPEAKERS: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo";
    const MIC: &str = "alsa_input.pci-0000_00_1f.3.analog-stereo";
    const MONITOR: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor";

    fn device_input(id: &str, description: &str, kind: InputKind) -> Input {
        Input {
            id: DeviceId::new(id),
            description: description.to_owned(),
            hardware: Hardware::default(),
            kind,
            volume: Volume::NORMAL,
            muted: false,
        }
    }

    /// Godwin's home devices, with Firefox recording the monitor and the
    /// speakers muted.
    fn broken() -> AudioState {
        AudioState {
            outputs: vec![Output {
                id: DeviceId::new(SPEAKERS),
                description: "Built-in Audio Analog Stereo".to_owned(),
                hardware: Hardware::default(),
                volume: Volume::NORMAL,
                muted: true,
            }],
            inputs: vec![
                device_input(
                    MONITOR,
                    "Monitor of Built-in Audio Analog Stereo",
                    InputKind::Monitor {
                        of: DeviceId::new(SPEAKERS),
                    },
                ),
                device_input(MIC, "Built-in Audio Analog Stereo", InputKind::Capture),
            ],
            default_output: Some(DeviceId::new(SPEAKERS)),
            default_input: Some(DeviceId::new(MIC)),
            playbacks: vec![],
            recordings: vec![Recording {
                id: StreamId::new(42),
                app: App {
                    name: "Firefox".to_owned(),
                    ..App::default()
                },
                media: None,
                input: DeviceId::new(MONITOR),
                volume: Volume::NORMAL,
                muted: false,
            }],
        }
    }

    #[test]
    fn a_good_setup() {
        assert_eq!(
            render_check(&Config::default(), &AudioState::default(), &[]),
            "No problem found.\n"
        );
    }

    #[test]
    fn the_check_lists_problems_and_says_how_to_fix() {
        let state = broken();
        let expected = "\
Problems
  ! Firefox records \"Monitor of Built-in Audio Analog Stereo\". This is a copy of an output, not a microphone: in a call, nobody hears you.
    Fix: move Firefox to \"Built-in Audio Analog Stereo\".
  ! The output \"Built-in Audio Analog Stereo\" is muted: you hear nothing.
    Fix: turn the sound of \"Built-in Audio Analog Stereo\" back on.

Run `deskcheck doctor --fix` to apply 2 fixes.
";
        assert_eq!(
            render_check(&Config::default(), &state, &diagnose(&state)),
            expected
        );
    }

    #[test]
    fn only_advice_means_no_fix_to_run() {
        let state = AudioState::default();
        let report = render_check(&Config::default(), &state, &diagnose(&state));
        assert!(
            report.ends_with("    Fix: plug in a headset or a microphone.\n"),
            "{report}"
        );
    }

    #[test]
    fn each_outcome_is_shown() {
        let mut state = broken();
        state.inputs.retain(Input::is_monitor);
        state.default_input = None;
        let findings = diagnose(&state);
        let outcomes = fix_all(&findings, |fix| match fix {
            Fix::Unmute(_) => Err("the device is busy".to_owned()),
            _ => Ok(()),
        });
        assert_eq!(
            outcomes,
            [
                Outcome::NoFix,
                Outcome::Manual,
                Outcome::Failed("the device is busy".to_owned())
            ]
        );
        let report = render_fixed(&Config::default(), &state, &findings, &outcomes, &findings);
        assert!(
            report.contains("nobody hears you.\n    No fix.\n"),
            "{report}"
        );
        assert!(report.contains(
            "  ! No microphone is plugged in.\n    To do yourself: plug in a headset or a microphone.\n"
        ));
        assert!(report.contains(
            "    Not fixed: turn the sound of \"Built-in Audio Analog Stereo\" back on.\n    \
             the device is busy\n"
        ));
        assert!(report.ends_with(
            "\nAfter the fixes: 3 problems left. Run `deskcheck doctor` to see them.\n"
        ));
    }

    #[test]
    fn fixed_problems_and_nothing_left() {
        let state = broken();
        let findings = diagnose(&state);
        let outcomes = fix_all(&findings, |_| Ok(()));
        let report = render_fixed(&Config::default(), &state, &findings, &outcomes, &[]);
        assert!(report.contains("    Fixed: move Firefox to \"Built-in Audio Analog Stereo\".\n"));
        assert!(report.ends_with("\nAfter the fixes: no problem left.\n"));
    }

    #[test]
    fn the_same_fix_is_applied_once() {
        let mut state = broken();
        // Two Firefox streams record the monitor: two moves, each once.
        let mut second = state.recordings[0].clone();
        second.id = StreamId::new(43);
        state.recordings.push(second.clone());
        // And the same stream listed twice gives the same fix twice.
        state.recordings.push(second);
        let findings = diagnose(&state);
        let mut applied = Vec::new();
        let outcomes = fix_all(&findings, |fix| {
            applied.push(fix.clone());
            Ok(())
        });
        assert_eq!(applied.len(), 3, "{applied:?}");
        assert_eq!(automatic_fixes(&findings).len(), 3);
        assert!(outcomes.iter().all(|outcome| *outcome == Outcome::Fixed));
    }
}
