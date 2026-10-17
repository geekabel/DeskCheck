//! Tests against the real GNOME session.
//!
//! They are ignored by default, because CI has no GNOME session. Run them
//! on the desktop with:
//!
//! ```sh
//! cargo test -p deskcheck-mutter -- --ignored
//! ```

#[test]
#[ignore = "needs a running GNOME session"]
fn reads_the_screens_of_the_running_session() {
    let state = deskcheck_mutter::read_state().expect("GNOME should answer");

    assert!(!state.screens.is_empty(), "no screen found");
    let enabled = state.enabled();
    assert!(!enabled.is_empty(), "no screen is on");
    assert_eq!(
        enabled.iter().filter(|screen| screen.is_primary()).count(),
        1,
        "there should be exactly one primary screen"
    );
    for screen in enabled {
        assert!(
            screen.current_mode.is_some(),
            "{} is on but has no current mode",
            screen.connector
        );
    }
}
