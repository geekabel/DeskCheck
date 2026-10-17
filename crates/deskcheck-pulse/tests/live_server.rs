//! Tests against a real sound server.
//!
//! They are ignored by default, because CI has no sound server. Run them on
//! a desktop with:
//!
//! ```sh
//! cargo test -p deskcheck-pulse -- --ignored
//! ```

#[test]
#[ignore = "needs a running sound server (PulseAudio or pipewire-pulse)"]
fn reads_the_state_of_the_running_server() {
    let state = deskcheck_pulse::read_state().expect("the sound server should answer");

    // Every output has a monitor in PulseAudio and in pipewire-pulse.
    assert!(!state.outputs.is_empty(), "no output found");
    for output in &state.outputs {
        assert!(
            state.monitors().any(|monitor| matches!(
                &monitor.kind,
                deskcheck_core::audio::InputKind::Monitor { of } if of == &output.id
            )),
            "no monitor found for {}",
            output.id
        );
    }

    // Defaults, when set, point to devices that exist.
    if state.default_output.is_some() {
        assert!(state.default_output_device().is_some());
    }
    if state.default_input.is_some() {
        assert!(state.default_input_device().is_some());
    }

    // Every stream points to a device that exists.
    for playback in &state.playbacks {
        assert!(state.output(&playback.output).is_some());
    }
    for recording in &state.recordings {
        assert!(state.input(&recording.input).is_some());
    }
}
