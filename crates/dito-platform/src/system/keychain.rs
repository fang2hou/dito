use dito_core::secrets::{SecretError, SecretStore};
use security_framework::passwords;

/// `errSecItemNotFound`.
const ITEM_NOT_FOUND: i32 = -25300;

/// API keys in the login keychain (generic passwords under one service name).
pub struct KeychainSecrets {
    service: String,
}

impl KeychainSecrets {
    pub fn new() -> Self {
        Self::with_service("com.fang2hou.dito")
    }

    pub fn with_service(service: &str) -> Self {
        Self { service: service.into() }
    }
}

impl Default for KeychainSecrets {
    fn default() -> Self {
        Self::new()
    }
}

fn backend_error(error: security_framework::base::Error) -> SecretError {
    SecretError::Backend(error.to_string())
}

impl SecretStore for KeychainSecrets {
    fn get(&self, account: &str) -> Option<String> {
        passwords::get_generic_password(&self.service, account).ok().and_then(|b| String::from_utf8(b).ok())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), SecretError> {
        passwords::set_generic_password(&self.service, account, value.as_bytes()).map_err(backend_error)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match passwords::delete_generic_password(&self.service, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(backend_error(e)),
        }
    }
}
