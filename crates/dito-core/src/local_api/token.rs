//! The access token the API and MCP share. It is a credential, so it lives in the Keychain (ADR-0008) and is read once.

use std::io::Read;
use std::sync::{Arc, Mutex};

use crate::secrets::{SecretError, SecretStore};

const ACCOUNT: &str = "local_api.token";
const PREFIX: &str = "dito_";

pub(super) struct Token {
    secrets: Arc<dyn SecretStore>,
    current: Mutex<Option<String>>,
}

impl Token {
    /// Nothing is read or made until the token is first needed, so a Mac that never turns the service on never touches
    /// the Keychain for it.
    pub(super) fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets, current: Mutex::new(None) }
    }

    /// The saved token, or the first one, made and saved now.
    pub(super) fn get(&self) -> String {
        let mut current = self.current.lock().expect("token lock poisoned");
        current
            .get_or_insert_with(|| {
                self.secrets.get(ACCOUNT).filter(|token| !token.is_empty()).unwrap_or_else(|| {
                    let token = generate();
                    if let Err(e) = self.secrets.set(ACCOUNT, &token) {
                        tracing::warn!("could not save the Local API token: {e}");
                    }
                    token
                })
            })
            .clone()
    }

    /// An empty token would let anything in, so it is not accepted.
    pub(super) fn set(&self, token: &str) -> Result<(), SecretError> {
        let token = token.trim();
        if token.is_empty() {
            return Ok(());
        }
        self.secrets.set(ACCOUNT, token)?;
        *self.current.lock().expect("token lock poisoned") = Some(token.to_owned());
        Ok(())
    }

    pub(super) fn regenerate(&self) -> Result<String, SecretError> {
        let token = generate();
        self.set(&token)?;
        Ok(token)
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
    use crate::secrets::MemorySecrets;

    #[test]
    fn the_first_token_is_saved_and_read_back_by_the_next_launch() {
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let first = Token::new(secrets.clone()).get();
        assert!(first.starts_with(PREFIX) && first.len() == PREFIX.len() + 32, "{first}");
        assert_eq!(Token::new(secrets).get(), first);
    }

    #[test]
    fn nothing_is_read_or_saved_until_the_token_is_needed() {
        let secrets = Arc::new(MemorySecrets::default());
        let token = Token::new(secrets.clone());
        assert_eq!(secrets.get(ACCOUNT), None);
        let made = token.get();
        assert_eq!(secrets.get(ACCOUNT), Some(made));
    }

    #[test]
    fn a_token_can_be_replaced_but_not_emptied_and_regenerating_changes_it() {
        let token = Token::new(Arc::new(MemorySecrets::default()));
        token.set("  my-own-token ").unwrap();
        assert_eq!(token.get(), "my-own-token");
        token.set("   ").unwrap();
        assert_eq!(token.get(), "my-own-token");
        let regenerated = token.regenerate().unwrap();
        assert_ne!(regenerated, "my-own-token");
        assert_eq!(token.get(), regenerated);
    }
}
