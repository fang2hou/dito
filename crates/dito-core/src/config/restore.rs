//! Recovery from a `config.toml` that cannot be read: report where it went wrong and restart from the defaults without
//! losing the file.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::store::write_file;
use super::{Config, ConfigError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseProblem {
    /// Counted from 1, where the parser stopped, when it can say.
    pub position: Option<(usize, usize)>,
    /// Without the file excerpt that its `Display` adds.
    pub message: String,
}

/// A syntax error carries its position. A value of the wrong type is found after the file parsed into a table, which
/// no longer remembers where each value came from, so the text is parsed once more straight into the settings: that
/// pass reports the same problem together with its position.
pub fn parse_problem(path: &Path, error: &toml::de::Error) -> ParseProblem {
    let message = error.message().to_owned();
    let Ok(text) = std::fs::read_to_string(path) else { return ParseProblem { position: None, message } };
    let located = |e: &toml::de::Error| e.span().map(|span| position(&text, span.start));
    let position = located(error).or_else(|| match toml::from_str::<Config>(&text) {
        Err(again) if again.message() == message => located(&again),
        _ => None,
    });
    ParseProblem { position, message }
}

/// Counted from 1, in characters.
fn position(text: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = &text[..end];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |current| current.chars().count()) + 1;
    (line, column)
}

/// The unreadable file is kept as `<file>.broken.bak`, or `<file>.broken-2.bak`, `-3` and so on while earlier backups
/// exist: a backup is never replaced. Returns the backup, or `None` when there was no file left to keep.
pub fn restore_defaults(path: &Path) -> Result<Option<PathBuf>, ConfigError> {
    let backup = keep_a_copy(path)?;
    write_file(path, &Config::default())?;
    Ok(backup)
}

fn keep_a_copy(path: &Path) -> io::Result<Option<PathBuf>> {
    let mut original = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let (backup, mut copy) = (1..)
        .map(|n| backup_path(path, n))
        .find_map(|candidate| {
            // Creating the file claims the name in one step, so an earlier backup is never replaced.
            match OpenOptions::new().write(true).create_new(true).open(&candidate) {
                Ok(file) => Some(Ok((candidate, file))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(e)),
            }
        })
        .expect("the backup numbers have no end")?;
    // The backup is the only copy once the original is replaced: have it on disk before that happens.
    if let Err(e) = io::copy(&mut original, &mut copy).and_then(|_| copy.sync_all()) {
        let _ = std::fs::remove_file(&backup);
        return Err(e);
    }
    Ok(Some(backup))
}

fn backup_path(path: &Path, n: u32) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    if n == 1 {
        name.push(".broken.bak");
    } else {
        name.push(format!(".broken-{n}.bak"));
    }
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigStore;

    fn config_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("config.toml")
    }

    fn problem_with(text: &str) -> ParseProblem {
        let dir = tempfile::tempdir().unwrap();
        let path = config_in(&dir);
        std::fs::write(&path, text).unwrap();
        let Err(ConfigError::Invalid { source, .. }) = ConfigStore::open(&path) else {
            panic!("{text:?} should be unreadable");
        };
        parse_problem(&path, &source)
    }

    #[test]
    fn the_unreadable_bytes_survive_in_the_backup_and_the_new_file_holds_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_in(&dir);
        // Not text at all: the backup must be a byte copy, not a re-encoded one.
        let broken: &[u8] = b"general = [\r\n\xff\xfe\x00 not toml";
        std::fs::write(&path, broken).unwrap();

        let backup = restore_defaults(&path).unwrap().expect("there was a file to keep");

        assert_eq!(backup, dir.path().join("config.toml.broken.bak"));
        assert_eq!(std::fs::read(&backup).unwrap(), broken);
        assert_eq!(*ConfigStore::open(&path).unwrap().get(), Config::default());
        assert!(path.exists(), "the defaults are written, not left to the first save");
    }

    #[test]
    fn a_second_restore_never_replaces_the_first_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_in(&dir);

        std::fs::write(&path, "first = [").unwrap();
        let first = restore_defaults(&path).unwrap().unwrap();
        std::fs::write(&path, "second = [").unwrap();
        let second = restore_defaults(&path).unwrap().unwrap();

        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first = [");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second = [");
        assert_eq!(*ConfigStore::open(&path).unwrap().get(), Config::default());
    }

    #[test]
    fn a_file_that_is_already_gone_still_gets_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_in(&dir);

        assert_eq!(restore_defaults(&path).unwrap(), None);
        assert_eq!(*ConfigStore::open(&path).unwrap().get(), Config::default());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no backup of nothing");
    }

    #[test]
    fn a_syntax_error_reports_its_line_and_column() {
        let problem = problem_with("[general]\nlaunch_at_login = @\n");
        assert_eq!(problem.position, Some((2, 19)));
        assert!(!problem.message.is_empty() && !problem.message.contains('\n'), "{problem:?}");
    }

    #[test]
    fn a_value_of_the_wrong_type_is_located_too() {
        let problem = problem_with("onboarded = true\n\n[general]\nlaunch_at_login = \"fast\"\n");
        assert_eq!(problem.position, Some((4, 19)));
        assert_eq!(problem.message, "invalid type: string \"fast\", expected a boolean");
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        // Three Japanese characters before the offending `@`: nine bytes, but three columns.
        let problem = problem_with("title = \"\u{3042}\u{3044}\u{3046}\" @\n");
        assert_eq!(problem.position, Some((1, 15)));
    }

    #[test]
    fn a_file_that_is_not_utf8_cannot_be_opened_and_restore_keeps_every_byte_of_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_in(&dir);
        // Valid TOML up to a stray Latin-1 byte: the file cannot be decoded, so it is a read error, not a parse error.
        let undecodable: &[u8] = b"onboarded = true\n# caf\xe9\n";
        std::fs::write(&path, undecodable).unwrap();
        let Err(ConfigError::Read { source, .. }) = ConfigStore::open(&path) else {
            panic!("a file that is not UTF-8 is a read error");
        };
        assert_eq!(source.kind(), io::ErrorKind::InvalidData);

        let backup = restore_defaults(&path).unwrap().expect("there was a file to keep");

        assert_eq!(std::fs::read(&backup).unwrap(), undecodable);
        assert_eq!(*ConfigStore::open(&path).unwrap().get(), Config::default());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_read_is_left_alone_and_nothing_is_written_over_it() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = config_in(&dir);
        std::fs::write(&path, "onboarded = true\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        if File::open(&path).is_ok() {
            return; // Running as root: file permissions do not apply.
        }
        assert!(matches!(ConfigStore::open(&path), Err(ConfigError::Read { .. })));

        assert!(restore_defaults(&path).is_err(), "no copy, no reset");

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "onboarded = true\n", "the file is untouched");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no backup and no temporary file");
    }
}
