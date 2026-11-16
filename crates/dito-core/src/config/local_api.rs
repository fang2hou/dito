use serde::{Deserialize, Serialize};

pub const DEFAULT_LOCAL_API_PORT: u16 = 17841;

/// The loopback service of the Local API tool. The access token is a secret and lives in the Keychain, not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalApiSettings {
    /// Off until the user switches it on: nothing listens by default.
    pub enabled: bool,
    pub port: u16,
    /// The OpenAI-compatible API answers without checking the token.
    pub api_any_token: bool,
    /// MCP clients run on this Mac and connect with the URL alone, so MCP defaults to any token.
    pub mcp_any_token: bool,
}

impl Default for LocalApiSettings {
    fn default() -> Self {
        Self { enabled: false, port: DEFAULT_LOCAL_API_PORT, api_any_token: false, mcp_any_token: true }
    }
}
