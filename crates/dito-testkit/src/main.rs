//! Standalone mock endpoint for offline development:
//! `cargo run -p dito-testkit --bin dito-mock-server -- 127.0.0.1:8787`
//!
//! Point Azure endpoint / OpenAI-compatible base URL at `http://127.0.0.1:8787` (any key is accepted).

use dito_testkit::MockServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:8787".into());
    let server = MockServer::start_on(&addr).await?;
    println!("dito mock server listening on {}", server.url);
    println!("  Azure endpoint / compat base URL: {} (compat: {}/v1)", server.url, server.url);
    tokio::signal::ctrl_c().await?;
    Ok(())
}
