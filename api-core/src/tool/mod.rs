//! Tool engine — routes tool_calls to skill loader or MCP client.
//!
//! Tool naming convention:
//!   skill_xxx   → Skill Loader
//!   mcp_xxx     → MCP Client
//!   rag_xxx     → RAG (inline, handled in Agent Engine)

use serde_json::Value;

use crate::error::{AppError, Result as AppResult};

/// Execute a named tool with given JSON arguments.
pub async fn execute_tool(
    tool_name: &str,
    arguments: &Value,
) -> AppResult<String> {
    tracing::info!(tool = %tool_name, "Tool engine dispatch");

    let result = match tool_name {
        name if name.starts_with("skill_") => {
            let skill_name = name.strip_prefix("skill_").unwrap_or(name);
            execute_skill(skill_name, arguments).await
        }
        name if name.starts_with("mcp_") => {
            let mcp_tool = name.strip_prefix("mcp_").unwrap_or(name);
            execute_mcp_tool(mcp_tool, arguments).await
        }
        _ => Err(AppError::BadRequest(format!("Unknown tool namespace: {tool_name}"))),
    };

    result
}

async fn execute_skill(skill_name: &str, args: &Value) -> AppResult<String> {
    // Phase 1B: load skill SKILL.md, execute run.py via subprocess, parse JSON output
    tracing::info!(skill = %skill_name, "Executing skill (stub)");
    Ok(serde_json::json!({
        "status": "ok",
        "skill": skill_name,
        "args": args,
        "message": "Skill execution stub — Phase 1B"
    }).to_string())
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
