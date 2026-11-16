//! The connection check of the Local API page: one real request to the running service, the way a client would make it.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Api,
    Mcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckFailure {
    /// Nothing answered on the port.
    Unreachable,
    /// The service refused the token.
    Unauthorized,
    Other(u16),
}

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"dito-check","version":"1"}}}"#;

pub async fn check_connection(port: u16, token: &str, surface: Surface) -> Result<(), CheckFailure> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| CheckFailure::Unreachable)?;
    let base = format!("http://127.0.0.1:{port}");
    let sent = match surface {
        Surface::Api => client.get(format!("{base}/v1/models")).bearer_auth(token).send().await,
        Surface::Mcp => {
            client
                .post(format!("{base}/mcp"))
                .bearer_auth(token)
                .header("Accept", "application/json, text/event-stream")
                .header("Content-Type", "application/json")
                .body(INITIALIZE)
                .send()
                .await
        }
    };
    let response = sent.map_err(|_| CheckFailure::Unreachable)?;
    let status = response.status();
    if surface == Surface::Mcp
        && let Some(session) = response.headers().get("mcp-session-id")
    {
        // The check is not a client; do not leave its session open.
        let _ = client.delete(format!("{base}/mcp")).bearer_auth(token).header("mcp-session-id", session).send().await;
    }
    match status.as_u16() {
        200..=299 => Ok(()),
        401 => Err(CheckFailure::Unauthorized),
        other => Err(CheckFailure::Other(other)),
    }
}
