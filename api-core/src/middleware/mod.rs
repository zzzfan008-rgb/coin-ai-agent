//! PreToolCall Hook — authorization gate executed before every tool call.
//!
//! Phase 1A ships a built-in RBAC rule checker that mirrors the Casbin
//! policy shape (`sub, obj, act`). Phase 1B swaps `PermissionChecker`
//! for a real Casbin enforcer backed by PostgreSQL adapter without
//! changing the call site (`pre_tool_call_check`).

use std::collections::HashSet;

use once_cell::sync::Lazy;

use crate::api::handlers::UserContext;
use crate::error::{AppError, Result};

/// Static permission table: role -> allowed tool names.
///
/// Matching is by exact tool name; the wildcard "*" grants all tools.
/// NOTE: Phase 1B replaces this with Casbin policies loaded from DB,
/// keyed on (user_id / role, org_id, dept_id, tool_name).
static ROLE_POLICY: Lazy<HashSet<(&str, &str)>> = Lazy::new(|| {
    [
        // admin — every tool
        ("admin", "*"),
        // designer — skill + RAG tools
        ("designer", "*skill*"),
        ("designer", "*rag*"),
        // viewer — read-only RAG only
        ("viewer", "*rag*"),
    ]
    .into_iter()
    .collect()
});

/// Gate called immediately before a tool executes.
pub async fn pre_tool_call_check(
    user_ctx: &UserContext,
    tool_name: &str,
    parameters: &serde_json::Value,
) -> Result<()> {
    tracing::debug!(
        user_id = %user_ctx.user_id,
        role = %user_ctx.role,
        tool = %tool_name,
        "PreToolCall Hook"
    );

    PermissionChecker::default()
        .authorize(&user_ctx.role, tool_name, parameters)
}

/// Pluggable checker — the Casbin adapter seam.
#[derive(Default)]
pub struct PermissionChecker {
    // Phase 1B: holds `casbin::Enforcer` + Postgres policy watcher
}

impl PermissionChecker {
    pub fn authorize(
        &self,
        role: &str,
        tool_name: &str,
        _parameters: &serde_json::Value,
    ) -> Result<()> {
        if has_permission(role, tool_name) {
            return Ok(());
        }
        Err(AppError::Forbidden(format!(
            "role '{role}' is not allowed to invoke tool '{tool_name}'"
        )))
    }
}

fn has_permission(role: &str, tool_name: &str) -> bool {
    for (policy_role, pattern) in ROLE_POLICY.iter() {
        if *policy_role != role {
            continue;
        }
        match *pattern {
            "*" => return true,
            p if p.starts_with('*') && p.ends_with('*') => {
                let inner = &p[1..p.len() - 1];
                if tool_name.contains(inner) {
                    return true;
                }
            }
            p => {
                if p == tool_name {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_policy_behaviour() {
        assert!(has_permission("admin", "mcp_anything"));
        assert!(has_permission("designer", "skill_fabric_query"));
        assert!(has_permission("designer", "rag_search"));
        assert!(has_permission("viewer", "rag_search"));
        assert!(!has_permission("viewer", "skill_fabric_query"));
        assert!(!has_permission("viewer", "mcp_write"));
        assert!(!has_permission("unknown", "rag_search"));
    }
}
