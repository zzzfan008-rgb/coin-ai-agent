//! Tool engine — routes tool_calls to skill engine or MCP client.
//!
//! Tool naming convention:
//!   skill_*   → Skill Engine (Phase 1B: routes to mock executor)
//!   mcp_*     → MCP Client
//!   rag_*     → RAG (handled inline in Agent Engine)

use serde_json::Value;

use crate::api::handlers::UserContext;
use crate::error::Result as AppResult;
use crate::skill_engine::FashionStore;

/// Execute a named tool with given JSON arguments and user context.
/// Used by the Agent Loop; the skill engine permission checks use `user_ctx`.
pub async fn execute_tool(
    tool_name: &str,
    arguments: &Value,
    user_ctx: &UserContext,
    store: &FashionStore,
) -> AppResult<String> {
    tracing::info!(tool = %tool_name, "Tool engine dispatch");

    let result = match tool_name {
        name if name.starts_with("skill_") => {
            crate::skill_engine::route_tool_call(name, arguments, user_ctx, store).await
        }
        name if name.starts_with("mcp_") => {
            let mcp_tool = name.strip_prefix("mcp_").unwrap_or(name);
            execute_mcp_tool(mcp_tool, arguments).await
        }
        // Bare or "mcp:"-style names are also handled by the skill router
        // (it forwards mcp: names to the MCP manager).
        _ => {
            crate::skill_engine::route_tool_call(tool_name, arguments, user_ctx, store).await
        }
    };

    result
}

async fn execute_mcp_tool(mcp_tool: &str, args: &Value) -> AppResult<String> {
    // Phase 1B: HTTP POST to MCP server /tools/{mcp_tool}/invoke
    tracing::info!(mcp_tool = %mcp_tool, "Executing MCP tool (stub)");
    Ok(serde_json::json!({
        "status": "ok",
        "mcp_tool": mcp_tool,
        "args": args,
        "message": "MCP tool stub — Phase 1B"
    }).to_string())
}
