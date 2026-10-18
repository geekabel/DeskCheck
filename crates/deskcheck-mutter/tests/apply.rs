//! End-to-end test of a change of the screens, with a fake Mutter that
//! keeps the calls to `ApplyMonitorsConfig`.
//!
//! As in `fake_mutter.rs`, the fake service and the client talk over a
//! peer-to-peer D-Bus connection on a socket pair.

use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use deskcheck_mutter::dbus::{Properties, WireLogicalConfig, WireLogicalMonitor, WireMonitor};
use deskcheck_mutter::setup::Change;
use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::zvariant::OwnedValue;

/// One call: serial, method, logical monitors.
type Call = (u32, u32, Vec<WireLogicalConfig>);

/// A fake `org.gnome.Mutter.DisplayConfig`: the laptop on the left,
/// primary, the Dell screen on the right. `refuse` makes the check fail.
struct FakeMutter {
    calls: Arc<Mutex<Vec<Call>>>,
    refuse: bool,
}

fn current() -> Properties {
    [("is-current".to_owned(), OwnedValue::from(true))]
        .into_iter()
        .collect()
}

#[zbus::interface(name = "org.gnome.Mutter.DisplayConfig")]
impl FakeMutter {
    #[allow(clippy::unused_self, reason = "D-Bus interface methods take &self")]
    #[zbus(out_args("serial", "monitors", "logical_monitors", "properties"))]
    fn get_current_state(&self) -> (u32, Vec<WireMonitor>, Vec<WireLogicalMonitor>, Properties) {
        let laptop = (
            "eDP-1".to_owned(),
            "BOE".to_owned(),
            "x".to_owned(),
            String::new(),
        );
        let dell = (
            "DP-1".to_owned(),
            "DEL".to_owned(),
            "U2419H".to_owned(),
            "A1".to_owned(),
        );
        let monitors = vec![
            (
                laptop.clone(),
                vec![(
                    "1920x1080@60".to_owned(),
                    1920,
                    1080,
                    60.0,
                    1.0,
                    vec![1.0, 1.25],
                    current(),
                )],
                Properties::new(),
            ),
            (
                dell.clone(),
                vec![(
                    "1920x1080@60".to_owned(),
                    1920,
                    1080,
                    60.0,
                    1.0,
                    vec![1.0],
                    current(),
                )],
                Properties::new(),
            ),
        ];
        let logical = vec![
            (0, 0, 1.0, 0, true, vec![laptop], Properties::new()),
            (1920, 0, 1.0, 0, false, vec![dell], Properties::new()),
        ];
        (42, monitors, logical, Properties::new())
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "zbus gives D-Bus arguments by value"
    )]
    fn apply_monitors_config(
        &self,
        serial: u32,
        method: u32,
        logical_monitors: Vec<WireLogicalConfig>,
        properties: Properties,
    ) -> zbus::fdo::Result<()> {
        // GNOME does not say it can change the layout mode: none is sent.
        assert!(properties.is_empty());
        if self.refuse {
            return Err(zbus::fdo::Error::Failed(
                "Logical monitors not adjacent".to_owned(),
            ));
        }
        self.calls
            .lock()
            .unwrap()
            .push((serial, method, logical_monitors));
        Ok(())
    }
}

fn connect(refuse: bool) -> (Connection, Connection, Arc<Mutex<Vec<Call>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let fake = FakeMutter {
        calls: Arc::clone(&calls),
        refuse,
    };
    let (server_end, client_end) = UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let server = thread::spawn(move || {
        Builder::unix_stream(server_end)
            .server(guid)
            .unwrap()
            .p2p()
            .serve_at("/org/gnome/Mutter/DisplayConfig", fake)
            .unwrap()
            .build()
            .unwrap()
    });
    let client = Builder::unix_stream(client_end)
        .p2p()
        .method_timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    (server.join().unwrap(), client, calls)
}

#[test]
fn a_change_is_checked_then_applied_for_good() {
    let (_server, client, calls) = connect(false);
    deskcheck_mutter::apply_with(&client, &Change::Primary("DP-1".to_owned())).unwrap();
    let calls = calls.lock().unwrap();
    // First "verify" (0), then "persistent" (2), with the serial GNOME gave.
    let methods: Vec<(u32, u32)> = calls.iter().map(|(s, m, _)| (*s, *m)).collect();
    assert_eq!(methods, [(42, 0), (42, 2)]);
    let primaries: Vec<(i32, bool, &str)> = calls[1]
        .2
        .iter()
        .map(|(x, _, _, _, primary, monitors)| (*x, *primary, monitors[0].0.as_str()))
        .collect();
    assert_eq!(primaries, [(0, false, "eDP-1"), (1920, true, "DP-1")]);
    assert_eq!(calls[1].2[1].5[0].1, "1920x1080@60");
}

#[test]
fn a_change_against_the_rules_is_not_sent() {
    let (_server, client, calls) = connect(false);
    let error = deskcheck_mutter::apply_with(
        &client,
        &Change::Scale {
            connector: "DP-1".to_owned(),
            scale: 2.0,
        },
    )
    .unwrap_err();
    assert!(
        matches!(&error, deskcheck_mutter::Error::Refused(reason) if reason.contains("scale")),
        "{error:?}"
    );
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn a_refusal_of_gnome_is_told() {
    let (_server, client, _calls) = connect(true);
    let error =
        deskcheck_mutter::apply_with(&client, &Change::Primary("DP-1".to_owned())).unwrap_err();
    assert_eq!(
        error,
        deskcheck_mutter::Error::Refused("Logical monitors not adjacent".to_owned())
    );
    assert!(
        error
            .to_string()
            .starts_with("the screens cannot be changed")
    );
}

#[test]
fn the_layout_of_a_place_is_applied_without_the_dialog() {
    use deskcheck_core::display::ScreenId;
    use deskcheck_core::places::{SavedLayout, SavedScreen};
    let (_server, client, calls) = connect(false);
    let screen = |id: ScreenId, x: i32, primary: bool| SavedScreen {
        id,
        x,
        y: 0,
        scale: 1.0,
        primary,
        width: 1920,
        height: 1080,
        refresh_rate: 60.0,
    };
    // The Dell on the left, primary, the laptop on its right.
    let saved = SavedLayout {
        screens: vec![
            screen(ScreenId::new("DEL", "U2419H", "A1"), 0, true),
            screen(ScreenId::new("BOE", "x", ""), 1920, false),
        ],
    };
    deskcheck_mutter::apply_layout_with(&client, &saved).unwrap();
    let calls = calls.lock().unwrap();
    // "verify" (0), then "temporary" (1): GNOME asks nothing.
    let methods: Vec<(u32, u32)> = calls.iter().map(|(s, m, _)| (*s, *m)).collect();
    assert_eq!(methods, [(42, 0), (42, 1)]);
    let layout: Vec<(i32, bool, &str)> = calls[1]
        .2
        .iter()
        .map(|(x, _, _, _, primary, monitors)| (*x, *primary, monitors[0].0.as_str()))
        .collect();
    assert_eq!(layout, [(0, true, "DP-1"), (1920, false, "eDP-1")]);
}
