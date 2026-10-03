//! Route regression tests — MCP endpoints (P0-2 / T-022).
//!
//! The test process starts with an empty MCP manager, which is itself a
//! real state the production code must handle:
//!   - GET /api/mcp/servers → 200 with an empty server list
//!   - GET /api/mcp/servers/:id/tools for an unknown server → 404

mod common;

#[tokio::test]
async fn mcp_servers_returns_empty_list_when_none_registered() {
    let base = common::spawn_app(common::test_state().await).await;

    let resp = common::client()
        .get(format!("{base}/api/mcp/servers"))
        .send()
        .await
        .expect("GET /api/mcp/servers");
    assert_eq!(resp.status().as_u16(), 200);

    let body: serde_json::Value = resp.json().await.expect("servers JSON");
    let servers = body["servers"]
        .as_array()
        .unwrap_or_else(|| panic!("servers must be an array, got: {body}"));
    assert!(
        servers.is_empty(),
        "no MCP servers registered, but list is non-empty: {body}"
    );
}

#[tokio::test]
async fn mcp_server_tools_unknown_server_returns_404() {
    let base = common::spawn_app(common::test_state().await).await;

    let resp = common::client()
        .get(format!("{base}/api/mcp/servers/no-such-server/tools"))
        .send()
        .await
        .expect("GET /api/mcp/servers/:id/tools");
    assert_eq!(resp.status().as_u16(), 404);

    let text = resp.text().await.unwrap_or_default();
    assert!(
        text.contains("not found"),
        "404 body must say the server was not found, got: {text}"
    );
}
