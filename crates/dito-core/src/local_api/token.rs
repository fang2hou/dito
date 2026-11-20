//! The access token the API and MCP share. It is kept in an owner-only file in Dito's data directory (ADR-0008).

use std::fs::{OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const PREFIX: &str = "dito_";
const OWNER_ONLY: u32 = 0o600;
static TEMP_NUMBER: AtomicU64 = AtomicU64::new(0);

pub(super) struct Token {
    path: PathBuf,
    current: Mutex<Option<String>>,
}

impl Token {
    /// Nothing is read or made until the token is first needed.
    pub(super) fn new(path: PathBuf) -> Self {
        Self { path, current: Mutex::new(None) }
    }

    /// The saved token, or the first one, made and saved now.
    pub(super) fn get(&self) -> String {
        let mut current = self.current.lock().expect("token lock poisoned");
        current.get_or_insert_with(|| self.read().unwrap_or_else(|e| self.replace(&e))).clone()
    }

    /// An empty token would let anything in, so it is not accepted.
    pub(super) fn set(&self, token: &str) -> io::Result<()> {
        let token = token.trim();
        if token.is_empty() {
            return Ok(());
        }
        write_atomic(&self.path, token)?;
        *self.current.lock().expect("token lock poisoned") = Some(token.to_owned());
        Ok(())
    }

    pub(super) fn regenerate(&self) -> io::Result<String> {
        let token = generate();
        self.set(&token)?;
        Ok(token)
    }

    fn read(&self) -> io::Result<String> {
        let stored = std::fs::read_to_string(&self.path)?;
        let token = stored.trim();
        if token.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the token file is empty"));
        }
        restrict_permissions(&self.path)?;
        Ok(token.to_owned())
    }

    fn replace(&self, read_error: &io::Error) -> String {
        if read_error.kind() != io::ErrorKind::NotFound {
            tracing::warn!("could not read the Local API token, replacing it: {read_error}");
        }
        let token = generate();
        if let Err(e) = write_atomic(&self.path, &token) {
            tracing::warn!("could not save the Local API token: {e}");
        }
        token
    }
}

fn restrict_permissions(path: &Path) -> io::Result<()> {
    let metadata = std::fs::metadata(path)?;
    if metadata.permissions().mode() & 0o777 != OWNER_ONLY {
        std::fs::set_permissions(path, Permissions::from_mode(OWNER_ONLY))?;
    }
    Ok(())
}

fn write_atomic(path: &Path, token: &str) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::other("the token path has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let (temporary, mut file) = create_temporary(parent)?;
    let result = (|| {
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn create_temporary(parent: &Path) -> io::Result<(PathBuf, std::fs::File)> {
    loop {
        let number = TEMP_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".local_api.token.{}.{}.tmp", std::process::id(), number));
        match OpenOptions::new().write(true).create_new(true).mode(OWNER_ONLY).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

fn generate() -> String {
    let mut bytes = [0u8; 16];
    // The system source, so no random-number dependency is needed for one token.
    let filled = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if filled.is_err() {
        // A Mac always has /dev/urandom; this only keeps a broken sandbox from handing out a constant token.
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        bytes.copy_from_slice(&(nanos ^ u128::from(std::process::id()) << 64).to_le_bytes());
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{PREFIX}{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("local_api.token")
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn the_first_token_is_saved_and_read_back_by_the_next_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = token_path(&dir);
        let first = Token::new(path.clone()).get();
        assert!(first.starts_with(PREFIX) && first.len() == PREFIX.len() + 32, "{first}");
        assert_eq!(Token::new(path).get(), first);
    }

    #[test]
    fn nothing_is_created_until_the_token_is_needed() {
        let dir = tempfile::tempdir().unwrap();
        let path = token_path(&dir);
        let token = Token::new(path.clone());
        assert!(!path.exists());
        token.get();
        assert!(path.is_file());
    }

    #[test]
    fn the_token_file_is_owner_only_at_creation_and_after_wider_permissions_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = token_path(&dir);
        let token = Token::new(path.clone()).get();
        assert_eq!(mode(&path), OWNER_ONLY);

        std::fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();
        assert_eq!(Token::new(path.clone()).get(), token);
        assert_eq!(mode(&path), OWNER_ONLY);
    }

    #[test]
    fn a_token_can_be_replaced_but_not_emptied_and_regenerating_changes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = token_path(&dir);
        let token = Token::new(path.clone());
        token.set("  my-own-token ").unwrap();
        assert_eq!(token.get(), "my-own-token");
        token.set("   ").unwrap();
        assert_eq!(token.get(), "my-own-token");
        let regenerated = token.regenerate().unwrap();
        assert_ne!(regenerated, "my-own-token");
        assert_eq!(token.get(), regenerated);
        assert_eq!(Token::new(path).get(), regenerated);
    }

    #[test]
    fn an_empty_or_unreadable_file_is_replaced() {
        for contents in [b"".as_slice(), &[0xff]] {
            let dir = tempfile::tempdir().unwrap();
            let path = token_path(&dir);
            std::fs::write(&path, contents).unwrap();
            let token = Token::new(path.clone()).get();
            assert!(token.starts_with(PREFIX) && !token.is_empty());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), token);
            assert_eq!(mode(&path), OWNER_ONLY);
        }
    }
}
