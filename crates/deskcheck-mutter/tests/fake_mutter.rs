//! End-to-end test of the D-Bus call with a fake Mutter.
//!
//! The fake service and the client talk over a peer-to-peer D-Bus
//! connection on a socket pair, as in the `iface_and_proxy_unix_p2p` test of
//! zbus. No bus daemon and no GNOME session are needed.

use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use deskcheck_mutter::dbus::{Properties, WireLogicalMonitor, WireMonitor, WireSpec};
use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::zvariant::{OwnedValue, Str};

fn spec(connector: &str, vendor: &str, product: &str, serial: &str) -> WireSpec {
    (
        connector.to_owned(),
        vendor.to_owned(),
        product.to_owned(),
        serial.to_owned(),
    )
}

fn props(entries: Vec<(&str, OwnedValue)>) -> Properties {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

/// A fake `org.gnome.Mutter.DisplayConfig` with the office setup: the laptop
/// panel on the left, the Dell screen on the right, the Dell screen primary.
struct FakeMutter;

#[zbus::interface(name = "org.gnome.Mutter.DisplayConfig")]
impl FakeMutter {
    #[allow(clippy::unused_self, reason = "D-Bus interface methods take &self")]
    #[zbus(out_args("serial", "monitors", "logical_monitors", "properties"))]
    fn get_current_state(&self) -> (u32, Vec<WireMonitor>, Vec<WireLogicalMonitor>, Properties) {
        let laptop = spec("eDP-1", "BOE", "0x0a1c", "0x00000000");
        let dell = spec("DP-1", "DEL", "DELL U2419H", "ABC1234");
        let monitors = vec![
            (
                laptop.clone(),
                vec![(
                    "1920x1080@60.010".to_owned(),
                    1920,
                    1080,
                    60.01,
                    1.25,
                    vec![1.0, 1.25, 1.5],
                    props(vec![("is-current", OwnedValue::from(true))]),
                )],
                props(vec![
                    ("is-builtin", OwnedValue::from(true)),
                    (
                        "display-name",
                        OwnedValue::from(Str::from("Built-in display")),
                    ),
                ]),
            ),
            (
                dell.clone(),
                vec![
                    (
                        "1920x1080@75".to_owned(),
                        1920,
                        1080,
                        75.0,
                        1.0,
                        vec![1.0],
                        Properties::new(),
                    ),
                    (
                        "1920x1080@60".to_owned(),
                        1920,
                        1080,
                        60.0,
                        1.0,
                        vec![1.0],
                        props(vec![("is-current", OwnedValue::from(true))]),
                    ),
                ],
                props(vec![
                    ("is-builtin", OwnedValue::from(false)),
                    (
                        "display-name",
                        OwnedValue::from(Str::from("Dell Inc. 24\"")),
                    ),
                ]),
            ),
        ];
        let logical_monitors = vec![
            (0, 0, 1.25, 0, false, vec![laptop], Properties::new()),
            (1536, 0, 1.0, 0, true, vec![dell], Properties::new()),
        ];
        (1, monitors, logical_monitors, Properties::new())
    }
}

#[test]
fn reads_the_screens_from_a_fake_mutter() {
    let (_server, client) = connect(
        Some("/org/gnome/Mutter/DisplayConfig"),
        Duration::from_secs(5),
    );

    let state = deskcheck_mutter::read_state_with(&client).unwrap();

    assert_eq!(state.screens.len(), 2);
    let primary = state.primary().unwrap();
    assert_eq!(primary.connector, "DP-1");
    assert_eq!(primary.id.product, "DELL U2419H");
    assert_eq!(primary.display_name.as_deref(), Some("Dell Inc. 24\""));
    assert_eq!(
        primary.current_mode.unwrap().to_string(),
        "1920 x 1080 @ 60 Hz"
    );

    let laptop = state.builtin().unwrap();
    assert_eq!(laptop.connector, "eDP-1");
    assert!(!laptop.id.has_serial());
    assert!((laptop.placement.unwrap().scale - 1.25).abs() < f64::EPSILON);

    let order: Vec<&str> = state
        .enabled()
        .iter()
        .map(|s| s.connector.as_str())
        .collect();
    assert_eq!(order, ["eDP-1", "DP-1"]);
}

/// Builds a peer-to-peer pair. `serve_at`, when given, is the path where
/// the fake Mutter is served.
fn connect(serve_at: Option<&'static str>, timeout: Duration) -> (Connection, Connection) {
    let (server_end, client_end) = UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let server = thread::spawn(move || {
        let builder = Builder::unix_stream(server_end).server(guid).unwrap().p2p();
        let builder = match serve_at {
            Some(path) => builder.serve_at(path, FakeMutter).unwrap(),
            None => builder,
        };
        builder.build().unwrap()
    });
    let client = Builder::unix_stream(client_end)
        .p2p()
        .method_timeout(timeout)
        .build()
        .unwrap();
    (server.join().unwrap(), client)
}

/// The service answers, but has no screen object at the expected path.
#[test]
fn an_unknown_object_gives_a_dbus_error() {
    let (_server, client) = connect(Some("/somewhere/else"), Duration::from_secs(5));
    let error = deskcheck_mutter::read_state_with(&client).unwrap_err();
    assert!(
        matches!(error, deskcheck_mutter::Error::DBus(_)),
        "{error:?}"
    );
}

/// The service never answers: the call stops after the timeout.
#[test]
fn no_answer_stops_after_the_timeout() {
    let (_server, client) = connect(None, Duration::from_millis(200));
    let error = deskcheck_mutter::read_state_with(&client).unwrap_err();
    assert_eq!(error, deskcheck_mutter::Error::NoAnswer);
}

const PATH: &str = "/org/gnome/Mutter/DisplayConfig";
const INTERFACE: &str = "org.gnome.Mutter.DisplayConfig";

/// GNOME sends `MonitorsChanged`: the watcher turns it into a change, and
/// stops quickly when dropped.
#[test]
fn the_watcher_sends_a_change_for_each_signal() {
    use std::sync::mpsc;
    use std::time::Instant;

    use deskcheck_core::events::Change;

    let (server, client) = connect(Some(PATH), Duration::from_secs(5));
    let (changes, received) = mpsc::channel();
    let watcher = deskcheck_mutter::ScreenWatcher::start_with(&client, changes).unwrap();

    for _ in 0..2 {
        server
            .emit_signal(None::<()>, PATH, INTERFACE, "MonitorsChanged", &())
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)),
            Ok(Change::Screens)
        );
    }

    let started = Instant::now();
    drop(watcher);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    // Stopped on purpose: nothing more, and no "lost" change.
    assert!(received.recv_timeout(Duration::from_millis(200)).is_err());
}

/// Outside GNOME, the watcher does not start.
#[test]
fn the_watcher_needs_gnome() {
    let (_server, client) = connect(Some("/somewhere/else"), Duration::from_secs(5));
    let (changes, _received) = std::sync::mpsc::channel();
    let error = deskcheck_mutter::ScreenWatcher::start_with(&client, changes).unwrap_err();
    assert!(
        matches!(error, deskcheck_mutter::Error::DBus(_)),
        "{error:?}"
    );
}
