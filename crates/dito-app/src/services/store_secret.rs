use crate::state::AppState;

/// An emptied value removes the secret instead of storing an empty one; a failed write is logged.
pub fn store_secret(st: &AppState, account: &str, value: &str) {
    let result = if value.is_empty() { st.secrets.delete(account) } else { st.secrets.set(account, value) };
    st.factory.invalidate_engines();
    if let Err(e) = result {
        tracing::warn!("could not store API key: {e}");
    }
}
