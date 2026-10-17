//! Loading and saving the configuration file on disk.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::{Config, ParseError};

/// Name of the file, inside the DeskCheck config folder.
pub const FILE_NAME: &str = "config.toml";

/// What can go wrong while loading or saving the file.
#[derive(Debug)]
pub enum Error {
    /// The home folder of the user is not known, so neither is the config
    /// folder.
    NoHome,
    /// The file exists but cannot be read.
    Read {
        /// The file.
        path: PathBuf,
        /// What the system said.
        source: io::Error,
    },
    /// The file cannot be written.
    Write {
        /// The file.
        path: PathBuf,
        /// What the system said.
        source: io::Error,
    },
    /// The file was read but its content is not valid.
    Parse {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        source: ParseError,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHome => f.write_str("cannot find your home folder, so no config folder"),
            Self::Read { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::Write { path, source } => write!(f, "cannot save {}: {source}", path.display()),
            Self::Parse { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoHome => None,
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

/// Where the configuration file is, with the code to load and save it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    /// The usual file: `$XDG_CONFIG_HOME/deskcheck/config.toml`, or
    /// `~/.config/deskcheck/config.toml` when that variable is not set.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoHome`] when the home folder is not known.
    pub fn new() -> Result<Self, Error> {
        let dirs = ProjectDirs::from("io.github", "geekabel", "DeskCheck").ok_or(Error::NoHome)?;
        Ok(Self::at(dirs.config_dir().join(FILE_NAME)))
    }

    /// A file at the given path, for tests or a `--config` option.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Path of the file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the file. A missing file gives an empty configuration.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Read`] when the file exists but cannot be read, or
    /// [`Error::Parse`] when its content is not valid.
    pub fn load(&self) -> Result<Config, Error> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Config::default()),
            Err(source) => {
                return Err(Error::Read {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        Config::from_toml(&text).map_err(|source| Error::Parse {
            path: self.path.clone(),
            source,
        })
    }

    /// Saves the configuration, creating the folder if needed.
    ///
    /// The text goes to a temporary file in the same folder, which is
    /// flushed to disk and then renamed over the old file. A rename in one
    /// folder replaces the file in one step, so the file is always either
    /// the old one or the new one. When the path is a symbolic link (for
    /// example into a dotfiles folder), the file it points to is replaced
    /// and the link stays.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Write`] when the folder or the file cannot be
    /// written. The old file is then left as it was.
    pub fn save(&self, config: &Config) -> Result<(), Error> {
        write_atomically(&self.path, &config.to_toml()).map_err(|source| Error::Write {
            path: self.path.clone(),
            source,
        })
    }
}

/// Most links followed before giving up, as `MAXSYMLINKS` on Linux.
const MAX_LINKS: usize = 40;

/// The real file to replace: the end of a chain of symbolic links, or the
/// path itself. The last target does not need to exist yet.
fn target(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_owned();
    for _ in 0..MAX_LINKS {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let link = fs::read_link(&current)?;
                // A relative target is relative to the folder of the link.
                current = match current.parent() {
                    Some(folder) => folder.join(link),
                    None => link,
                };
            }
            Ok(_) => return Ok(current),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(current),
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("too many levels of symbolic links"))
}

/// Name of the temporary file next to `path`. The process id keeps two
/// DeskCheck processes from using the same temporary file.
fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(|| FILE_NAME.into(), |name| name.to_string_lossy());
    path.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}

fn write_new_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = match File::create_new(path) {
        Ok(file) => file,
        // Left over by a crash of a process that had the same id.
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            fs::remove_file(path)?;
            File::create_new(path)?
        }
        Err(error) => return Err(error),
    };
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}

fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let path = target(path)?;
    let folder = match path.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder.to_owned(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&folder)?;

    let temporary = temporary_path(&path);
    let written = write_new_file(&temporary, contents).and_then(|()| fs::rename(&temporary, &path));
    if let Err(error) = written {
        // Best effort: the error that matters is the one above.
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    // Flush the folder too, so the rename itself survives a power cut.
    File::open(&folder)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temporary_file_is_hidden_next_to_the_file() {
        let temporary = temporary_path(Path::new("/home/godwin/.config/deskcheck/config.toml"));
        let name = temporary.file_name().unwrap().to_string_lossy();
        assert_eq!(
            temporary.parent(),
            Some(Path::new("/home/godwin/.config/deskcheck"))
        );
        assert!(name.starts_with(".config.toml."), "{name}");
        assert!(name.ends_with(".tmp"), "{name}");
    }

    #[test]
    fn the_usual_file_is_in_the_deskcheck_config_folder() {
        // The home folder is known in the test environment.
        let store = Store::new().unwrap();
        assert!(
            store.path().ends_with("deskcheck/config.toml"),
            "{}",
            store.path().display()
        );
    }
}
