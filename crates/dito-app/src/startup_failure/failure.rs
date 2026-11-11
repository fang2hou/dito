use std::path::{Path, PathBuf};

use dito_core::config::{ConfigError, parse_problem};
use dito_core::i18n::{Lang, Tr, fmt};
use dito_core::store::StoreError;

use crate::opts::Opts;
use crate::state::database_path;

/// A startup failure that has a window of its own. Every other failure ends the process with its error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Starting over from the default settings keeps the file as a backup, and leaves it alone when even that copy
    /// cannot be made.
    UnreadableSettings {
        path: PathBuf,
        /// Line and column, both from 1.
        position: Option<(usize, usize)>,
        message: String,
    },
    /// Only an update helps, so nothing offers to change the data.
    NewerData { path: PathBuf, found: u32, supported: u32 },
}

impl Failure {
    pub fn from_error(error: &anyhow::Error, opts: &Opts) -> Option<Self> {
        if let Some(error) = error.downcast_ref::<ConfigError>() {
            return match error {
                ConfigError::Invalid { path, source } => {
                    let problem = parse_problem(path, source);
                    Some(Self::UnreadableSettings {
                        path: absolute(path),
                        position: problem.position,
                        message: problem.message,
                    })
                }
                ConfigError::NewerSchema { path, found, supported } => {
                    Some(Self::NewerData { path: absolute(path), found: *found, supported: *supported })
                }
                // A settings file that exists but cannot be opened or decoded is as unreadable as one that does not
                // parse. A missing one never gets here: that is a first launch.
                ConfigError::Read { path, source } => {
                    Some(Self::UnreadableSettings { path: absolute(path), position: None, message: source.to_string() })
                }
                ConfigError::Write(_) | ConfigError::Encode(_) => None,
            };
        }
        match error.downcast_ref::<StoreError>()? {
            StoreError::SchemaTooNew { found, supported } => {
                Some(Self::NewerData { path: absolute(&database_path(opts)), found: *found, supported: *supported })
            }
            StoreError::Database(_) | StoreError::Io(_) => None,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::UnreadableSettings { path, .. } | Self::NewerData { path, .. } => path,
        }
    }

    pub fn can_restore(&self) -> bool {
        matches!(self, Self::UnreadableSettings { .. })
    }

    pub fn title(&self, lang: Lang) -> &'static str {
        lang.tr(match self {
            Self::UnreadableSettings { .. } => Tr::StartupUnreadableTitle,
            Self::NewerData { .. } => Tr::StartupNewerTitle,
        })
    }

    pub fn body(&self, lang: Lang) -> &'static str {
        lang.tr(match self {
            Self::UnreadableSettings { .. } => Tr::StartupUnreadableBody,
            Self::NewerData { .. } => Tr::StartupNewerBody,
        })
    }

    pub fn hint(&self, lang: Lang) -> Option<&'static str> {
        self.can_restore().then(|| lang.tr(Tr::StartupUnreadableHint))
    }

    pub fn reason(&self, lang: Lang) -> String {
        match self {
            Self::UnreadableSettings { position: Some((line, column)), message, .. } => {
                fmt::line_column_error(lang, &column.to_string(), message, &line.to_string())
            }
            Self::UnreadableSettings { message, .. } => message.clone(),
            Self::NewerData { found, supported, .. } => {
                fmt::startup_newer_version(lang, &found.to_string(), &supported.to_string())
            }
        }
    }
}

/// `path` as an absolute path, so what the window shows and reveals does not depend on the working directory.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

pub fn display_path(path: &Path) -> String {
    tilde(path, std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

fn tilde(path: &Path, home: Option<&Path>) -> String {
    // A home folder of `/` would turn every path into `~/…`.
    let home = home.filter(|home| home.parent().is_some());
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use dito_core::config::{ConfigStore, SCHEMA_VERSION};

    use super::*;

    fn opts_in(dir: &str) -> Opts {
        Opts { data_dir: Some(PathBuf::from(dir)), ..Opts::default() }
    }

    #[test]
    fn only_unusable_data_files_get_a_window() {
        let path = PathBuf::from("/data/config.toml");
        let newer = anyhow::Error::from(ConfigError::NewerSchema { path: path.clone(), found: 9, supported: 2 });
        assert_eq!(
            Failure::from_error(&newer, &opts_in("/data")),
            Some(Failure::NewerData { path: path.clone(), found: 9, supported: 2 })
        );

        let database = anyhow::Error::from(StoreError::SchemaTooNew { found: 5, supported: 3 }).context("opening");
        assert_eq!(
            Failure::from_error(&database, &opts_in("/data")),
            Some(Failure::NewerData { path: PathBuf::from("/data/dito.db"), found: 5, supported: 3 }),
            "the database error does not name its file, and a context around it does not hide it"
        );

        let unreadable_disk = anyhow::Error::from(ConfigError::Read {
            path,
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        });
        assert_eq!(
            Failure::from_error(&unreadable_disk, &opts_in("/data")),
            Some(Failure::UnreadableSettings {
                path: PathBuf::from("/data/config.toml"),
                position: None,
                message: std::io::Error::from(std::io::ErrorKind::PermissionDenied).to_string(),
            }),
            "a settings file that exists but cannot be read is as unreadable as one that does not parse"
        );
        let not_the_files_fault =
            anyhow::Error::from(ConfigError::Write(std::io::Error::from(std::io::ErrorKind::PermissionDenied)));
        assert_eq!(Failure::from_error(&not_the_files_fault, &opts_in("/data")), None);
        assert_eq!(Failure::from_error(&anyhow::anyhow!("no audio input"), &opts_in("/data")), None);
    }

    #[test]
    fn an_unreadable_file_is_described_with_its_position_when_there_is_one() {
        let located = Failure::UnreadableSettings {
            path: PathBuf::from("/data/config.toml"),
            position: Some((12, 5)),
            message: "invalid type".into(),
        };
        assert_eq!(located.reason(Lang::En), "Line 12, column 5: invalid type");

        let unlocated = Failure::UnreadableSettings {
            path: PathBuf::from("/data/config.toml"),
            position: None,
            message: "invalid type".into(),
        };
        assert_eq!(unlocated.reason(Lang::En), "invalid type");
    }

    #[test]
    fn newer_data_states_both_versions_and_offers_no_restore() {
        let failure = Failure::NewerData { path: PathBuf::from("/data/dito.db"), found: 3, supported: SCHEMA_VERSION };
        assert_eq!(failure.reason(Lang::En), format!("Format version: 3 (this Dito opens up to {SCHEMA_VERSION})"));
        assert!(!failure.can_restore() && failure.hint(Lang::En).is_none());
    }

    #[test]
    fn a_settings_file_that_is_not_utf8_gets_the_unreadable_window_with_the_decoding_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, b"onboarded = true\n# caf\xe9\n").unwrap();
        let Err(error) = ConfigStore::open(&path) else { panic!("a file that is not UTF-8 must not load") };

        let failure = Failure::from_error(&anyhow::Error::from(error), &opts_in("/data")).expect("it has a window");

        let Failure::UnreadableSettings { position, message, .. } = &failure else { panic!("{failure:?}") };
        assert_eq!(*position, None);
        assert!(message.contains("UTF-8"), "{message}");
        assert!(failure.can_restore(), "the reset is on offer: the bytes can still be copied");
    }

    #[test]
    fn the_home_folder_is_written_as_a_tilde() {
        let home = Some(Path::new("/Users/ab"));
        assert_eq!(tilde(Path::new("/Users/ab/Library/x/config.toml"), home), "~/Library/x/config.toml");
        assert_eq!(tilde(Path::new("/Users/ab"), home), "~");
        assert_eq!(tilde(Path::new("/Users/abc/x"), home), "/Users/abc/x", "a longer name is not the home folder");
        assert_eq!(tilde(Path::new("/tmp/x"), home), "/tmp/x");
        assert_eq!(tilde(Path::new("/x"), Some(Path::new("/"))), "/x");
        assert_eq!(tilde(Path::new("/x"), None), "/x");
    }
}
