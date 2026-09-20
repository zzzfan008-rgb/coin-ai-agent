//! MCP Client — connects to MCP Server (HTTP/SSE) and invokes remote tools.
//!
//! MCP (Model Context Protocol) servers expose tools over HTTP + SSE streams.
//! This client handles:
//!   - Discovery: GET /tools
//!   - Invocation: POST /tools/{name}/invoke
//!   - Streaming results via SSE

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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

/// MCP client for a single server.
pub struct McpClient {
    http: Client,
    server: McpServerConfig,
}

impl McpClient {
    pub fn new(server: McpServerConfig) -> Self {
        Self {
            http: Client::new(),
            server,
        }
    }

    /// List available tools from the MCP server.
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let resp = self
            .http
            .get(format!("{}/tools", self.server.base_url))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]); // Stub: return empty on error
        }

        #[derive(Deserialize)]
        struct ToolsResponse {
            tools: Vec<McpTool>,
        }

        let tools: ToolsResponse = resp.json().await.unwrap_or(ToolsResponse { tools: vec![] });
        Ok(tools.tools)
    }

    /// Invoke a tool on the MCP server with given parameters.
    pub async fn invoke_tool(
        &self,
        tool_name: &str,
        parameters: serde_json::Value,
    ) -> Result<String> {
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
            return Err(anyhow::anyhow!(
                "MCP tool {} failed: {}",
                tool_name,
                resp.status()
            ));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::to_string(&result)?)
    }
}

/// MCP client manager — holds connections to all configured MCP servers.
pub struct McpClientManager {
    clients: Vec<McpClient>,
}

impl McpClientManager {
    pub fn new(servers: Vec<McpServerConfig>) -> Self {
        let clients = servers.into_iter().map(McpClient::new).collect();
        Self { clients }
    }

    pub async fn invoke(&self, server_id: &str, tool_name: &str, params: serde_json::Value) -> Result<String> {
        let client = self
            .clients
            .iter()
            .find(|c| c.server.id == server_id)
            .ok_or_else(|| anyhow::anyhow!("MCP server {server_id} not found"))?;

        client.invoke_tool(tool_name, params).await
    }
}
