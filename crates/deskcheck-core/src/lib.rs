//! Core of DeskCheck.
//!
//! This crate holds the domain model and the rules (device names, diagnostics,
//! places). It has no system dependency, so everything here is easy to test.
//!
//! - [`audio`]: outputs, inputs (microphones and monitors) and the streams
//!   of the applications.
//! - [`display`]: connected screens and their placement on the desktop.

pub mod audio;
pub mod display;
pub mod events;
pub mod places;

/// Human readable name of the application.
pub const APP_NAME: &str = "DeskCheck";

/// Application ID, used by GTK, D-Bus and the desktop file.
///
/// It follows the reverse DNS rule for projects hosted on GitHub.
pub const APP_ID: &str = "io.github.geekabel.DeskCheck";

/// Version of DeskCheck, taken from `Cargo.toml` at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the name and version, for example `DeskCheck 0.1.0`.
#[must_use]
pub fn version_line() -> String {
    format!("{APP_NAME} {VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_has_name_and_version() {
        assert_eq!(version_line(), format!("DeskCheck {VERSION}"));
    }

    #[test]
    fn app_id_is_a_valid_application_id() {
        // Rules from the GApplication documentation: at least two elements
        // separated by dots, each element made of [A-Za-z0-9_-], not starting
        // with a digit, and 255 characters at most.
        assert!(APP_ID.len() <= 255);
        let elements: Vec<&str> = APP_ID.split('.').collect();
        assert!(elements.len() >= 2);
        for element in elements {
            assert_ne!(element, "");
            assert!(!element.starts_with(|c: char| c.is_ascii_digit()));
            assert!(
                element
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            );
        }
    }
}
