use std::collections::HashMap;
use std::sync::Mutex;

use crate::config::ProviderKind;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("{0}")]
    Backend(String),
}

/// Where API keys live. The app uses the macOS keychain; tests use [`MemorySecrets`].
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Option<String>;
    fn set(&self, account: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Asr,
    Polish,
}

/// Keychain account name for the key of one role + provider kind.
pub fn account(role: Role, kind: ProviderKind) -> String {
    let r = match role {
        Role::Asr => "asr",
        Role::Polish => "polish",
    };
    format!("{r}.{}", kind.id())
}

#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, account: &str) -> Option<String> {
        self.0.lock().expect("secrets mutex poisoned").get(account).cloned()
    }
    fn set(&self, account: &str, value: &str) -> Result<(), SecretError> {
        self.0.lock().expect("secrets mutex poisoned").insert(account.into(), value.into());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.0.lock().expect("secrets mutex poisoned").remove(account);
        Ok(())
    }
}
