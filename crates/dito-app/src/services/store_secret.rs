use dito_core::secrets::SecretError;

use crate::state::AppState;

/// An emptied value removes the secret instead of storing an empty one. The cached engines are dropped either way,
/// since one may hold the old key. A failed write is logged and returned.
pub fn try_store_secret(st: &AppState, account: &str, value: &str) -> Result<(), SecretError> {
    let result = if value.is_empty() { st.secrets.delete(account) } else { st.secrets.set(account, value) };
    st.factory.invalidate_engines();
    result.inspect_err(|e| tracing::warn!("could not store API key: {e}"))
}

/// [`try_store_secret`] for a key whose failure the caller has nothing to say about.
pub fn store_secret(st: &AppState, account: &str, value: &str) {
    let _ = try_store_secret(st, account, value);
}
