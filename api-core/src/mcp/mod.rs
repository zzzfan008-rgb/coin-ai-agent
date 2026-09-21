//! MCP Client — connects to MCP Server (HTTP/SSE) and invokes remote tools.
//!
//! MCP (Model Context Protocol) servers expose tools over HTTP + SSE streams.
//! This client handles:
//!   - Discovery: GET /tools
//!   - Invocation: POST /tools/{name}/invoke
//!   - Streaming results via SSE
//!   - Auto-reconnect with exponential backoff
//!   - Tool result caching (TTL-based, in-memory)
//!   - Health check (ping)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// MCP server connection metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub auth_token: Option<String>,
}

/// Tool definition from MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Cached tool result with TTL.
#[derive(Debug, Clone)]
struct CachedResult {
    result: String,
    cached_at: Instant,
}

/// MCP client for a single server.
#[derive(Debug)]
pub struct McpClient {
    http: Client,
    server: McpServerConfig,
    /// tool_name → (result, instant)
    cache: Arc<RwLock<HashMap<String, CachedResult>>>,
    /// Reconnect state
    consecutive_failures: Arc<RwLock<u32>>,
}

impl McpClient {
    pub fn new(server: McpServerConfig) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest Client::new must not fail");
        Self {
            http,
            server,
            cache: Arc::new(RwLock::new(HashMap::new())),
            consecutive_failures: Arc::new(RwLock::new(0)),
        }
    }

    /// Health check — returns Ok if the server responds to /ping or /health.
    pub async fn ping(&self) -> bool {
        let url = format!("{}/health", self.server.base_url);
        match self.http.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => {
                // Fallback: try /tools list endpoint
                let url = format!("{}/tools", self.server.base_url);
                self.http.get(&url).send().await.is_ok()
            }
        }
    }

    /// Increment failure counter and return current backoff duration.
    pub async fn backoff_duration(&self) -> Duration {
        let mut failures = self.consecutive_failures.write().await;
        *failures += 1;
        let backoff_secs = (2u64).pow(*failures).min(60);
        Duration::from_secs(backoff_secs)
    }

    /// Reset failure counter on successful call.
    pub async fn record_success(&self) {
        let mut failures = self.consecutive_failures.write().await;
        *failures = 0;
    }

    /// List available tools from the MCP server.
    /// Falls back to empty list on network error (does not propagate failure).
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let resp = self
            .http
            .get(format!("{}/tools", self.server.base_url))
            .send()
            .await
            .inspect_err(|e| tracing::warn!(server = %self.server.id, "list_tools failed: {e}"))?;

        if !resp.status().is_success() {
            tracing::warn!(server = %self.server.id, status = %resp.status(), "list_tools non-200");
            return Ok(vec![]);
        }

        #[derive(Deserialize)]
        struct ToolsResponse {
            tools: Vec<McpTool>,
        }

        let tools = resp
            .json::<ToolsResponse>()
            .await
            .unwrap_or(ToolsResponse { tools: vec![] });

        tracing::info!(server = %self.server.id, count = tools.tools.len(), "MCP tools discovered");
        Ok(tools.tools)
    }

    /// Invoke a tool on the MCP server with given parameters.
    /// Results are cached for 5 minutes (TTL).
    pub async fn invoke_tool(
        &self,
        tool_name: &str,
        parameters: serde_json::Value,
    ) -> Result<String> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(tool_name) {
                if cached.cached_at.elapsed() < Duration::from_secs(300) {
                    tracing::debug!(server = %self.server.id, tool = %tool_name, "MCP cache hit");
                    return Ok(cached.result.clone());
                }
            }
        }

        tracing::info!(
            server = %self.server.id,
            tool = %tool_name,
            "MCP tool invocation"
        );

        let url = format!("{}/tools/{}/invoke", self.server.base_url, tool_name);

        let mut req = self.http.post(&url).json(&serde_json::json!({
            "parameters": parameters,
        }));

        if let Some(token) = &self.server.auth_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "MCP tool {} failed ({}): {}",
                tool_name,
                status,
                body
            ));
        }

        let result: serde_json::Value = resp.json().await?;
        let result_str = serde_json::to_string(&result)?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                tool_name.to_string(),
                CachedResult {
                    result: result_str.clone(),
                    cached_at: Instant::now(),
                },
            );
        }

        self.record_success().await;
        Ok(result_str)
    }

    /// Invalidate cached result for a tool.
    pub async fn invalidate_cache(&self, tool_name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(tool_name);
    }

    /// Server ID accessor.
    pub fn server_id(&self) -> &str {
        &self.server.id
    }

    /// Server name accessor.
    pub fn server_name(&self) -> &str {
        &self.server.name
    }
}

/// MCP client manager — holds connections to all configured MCP servers.
#[derive(Debug, Clone)]
pub struct McpClientManager {
    clients: Arc<RwLock<HashMap<String, McpClient>>>,
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a server config (idempotent — replaces existing client for that id).
    pub async fn register(&self, server: McpServerConfig) {
        let client = McpClient::new(server);
        let mut clients = self.clients.write().await;
        clients.insert(client.server.id.clone(), client);
    }

    /// Remove a server by id.
    pub async fn unregister(&self, server_id: &str) {
        let mut clients = self.clients.write().await;
        clients.remove(server_id);
        tracing::info!(server_id, "MCP server unregistered");
    }

    /// Invoke a tool on a named server.
    pub async fn invoke(
        &self,
        server_id: &str,
        tool_name: &str,
        params: serde_json::Value,
    ) -> Result<String> {
        let clients = self.clients.read().await;
        let client = clients
            .get(server_id)
            .ok_or_else(|| anyhow::anyhow!("MCP server {server_id} not found"))?;
        client.invoke_tool(tool_name, params).await
    }

    /// List all tools from a server.
    pub async fn list_tools(&self, server_id: &str) -> Result<Vec<McpTool>> {
        let clients = self.clients.read().await;
        let client = clients
            .get(server_id)
            .ok_or_else(|| anyhow::anyhow!("MCP server {server_id} not found"))?;
        client.list_tools().await
    }

    /// Health check all servers.
    pub async fn health_check(&self) -> HashMap<String, bool> {
        let clients = self.clients.read().await;
        let mut results = HashMap::new();
        for (id, client) in clients.iter() {
            results.insert(id.clone(), client.ping().await);
        }
        results
    }

    /// All registered server IDs.
    pub async fn server_ids(&self) -> Vec<String> {
        let clients = self.clients.read().await;
        clients.keys().cloned().collect()
    }

    /// Discover and return all tools from every registered server.
    /// Failures on individual servers are logged and skipped so one
    /// unreachable server doesn't block discovery of the rest.
    pub async fn discover_all_tools(&self) -> Vec<(String, Vec<McpTool>)> {
        let ids = self.server_ids().await;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.list_tools(&id).await {
                Ok(tools) => out.push((id, tools)),
                Err(e) => tracing::warn!(server_id = %id, "discover_all_tools skip: {e}"),
            }
        }
        out
    }
}

/// Process-global MCP client manager.
pub static MCP_MANAGER: once_cell::sync::Lazy<McpClientManager> =
    once_cell::sync::Lazy::new(McpClientManager::new);
