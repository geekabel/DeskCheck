//! Loading and saving real files, in a folder under Cargo's target folder.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use deskcheck_config::{Config, Error, ParseError, Store};
use deskcheck_core::audio::DeviceId;
use deskcheck_core::display::ScreenId;

/// An empty folder for one test. `CARGO_TARGET_TMPDIR` is set by Cargo for
/// integration tests.
fn folder(test: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("config-{test}"));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    folder
}

fn sample() -> Config {
    let mut config = Config::default();
    config.set_audio_name(
        &DeviceId::new("alsa_input.usb-Jabra"),
        Some("Headset".to_owned()),
    );
    let asus = ScreenId::new("AUS", "XG27ACS", "T2LMTF031523");
    config.set_screen_name(&asus, Some("Big screen".to_owned()));
    config.set_screen_color(&asus, Some("#3584e4".parse().unwrap()));
    config
}

fn names_in(folder: &PathBuf) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_missing_file_is_an_empty_config() {
    let store = Store::at(folder("missing").join("deskcheck/config.toml"));
    assert_eq!(store.load().unwrap(), Config::default());
}

#[test]
fn save_creates_the_folder_and_load_reads_it_back() {
    let folder = folder("round-trip");
    let store = Store::at(folder.join("deskcheck/config.toml"));
    store.save(&sample()).unwrap();
    assert_eq!(store.load().unwrap(), sample());
    // No temporary file is left behind.
    assert_eq!(names_in(&folder.join("deskcheck")), ["config.toml"]);
}

#[test]
fn save_replaces_the_whole_old_file() {
    let folder = folder("replace");
    let store = Store::at(folder.join("config.toml"));
    store.save(&sample()).unwrap();
    store.save(&Config::default()).unwrap();
    let text = fs::read_to_string(store.path()).unwrap();
    assert!(text.ends_with("\nversion = 1\n"), "{text}");
    assert_eq!(store.load().unwrap(), Config::default());
}

#[test]
fn save_keeps_a_symbolic_link() {
    let folder = folder("link");
    fs::create_dir_all(folder.join("dotfiles")).unwrap();
    let link = folder.join("config.toml");
    // A relative link to a file that does not exist yet.
    symlink("dotfiles/deskcheck.toml", &link).unwrap();

    let store = Store::at(&link);
    store.save(&sample()).unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        Store::at(folder.join("dotfiles/deskcheck.toml"))
            .load()
            .unwrap(),
        sample()
    );
    assert_eq!(store.load().unwrap(), sample());
}

#[test]
fn a_bad_file_names_the_path() {
    let folder = folder("bad");
    let store = Store::at(folder.join("config.toml"));
    fs::write(
        store.path(),
        "version = 1\n[[audio]]\nid = \"a\"\nnick = \"b\"\n",
    )
    .unwrap();
    let error = store.load().unwrap_err();
    assert!(
        matches!(
            &error,
            Error::Parse {
                source: ParseError::Toml(_),
                ..
            }
        ),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("config.toml"), "{message}");
    assert!(message.contains("name"), "{message}");
}

#[test]
fn a_failed_save_leaves_the_old_file() {
    let folder = folder("failed");
    let store = Store::at(folder.join("config.toml"));
    store.save(&sample()).unwrap();
    // A folder where the temporary file should go makes the write fail.
    let temporary = folder.join(format!(".config.toml.{}.tmp", std::process::id()));
    fs::create_dir(&temporary).unwrap();
    fs::write(temporary.join("keep"), "").unwrap();

    let error = store.save(&Config::default()).unwrap_err();
    assert!(matches!(error, Error::Write { .. }), "{error:?}");
    assert_eq!(store.load().unwrap(), sample());
}

#[test]
fn a_folder_in_place_of_the_file_cannot_be_read() {
    let folder = folder("not-a-file");
    fs::create_dir(folder.join("config.toml")).unwrap();
    let error = Store::at(folder.join("config.toml")).load().unwrap_err();
    assert!(matches!(error, Error::Read { .. }), "{error:?}");
}
