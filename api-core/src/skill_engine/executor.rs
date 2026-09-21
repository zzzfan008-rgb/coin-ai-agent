//! Skill executor — runs skill tool logic (Phase 1B: mock data).
//!
//! Execution path:
//!   route_tool_call() ← called from Agent Loop (tool/mod.rs dispatches skill_* here)
//!     → check_skill_permission()  ← inline permission gate
//!     → execute_skill()            ← mock / run.py dispatch
//!
//! The `SKILL_ENGINE` global must be initialised before this module is used.

use serde_json::Value;
use uuid::Uuid;

use crate::api::handlers::UserContext;
use crate::error::{AppError, Result};
use crate::skill_engine::{FASHION_STORE, FashionStore, SKILL_ENGINE, SkillMetadata};
use crate::skill_engine::color_theory;

/// Route an LLM tool_call to the correct skill executor.
///
/// Tool naming convention used by the LLM:
///   `<skill-id-kebab>_<tool-name-snake>`  e.g. `fabric-query_search_fabric`
/// We convert kebab → snake for parsing, then look up the skill.
pub async fn route_tool_call(
    tool_name: &str,
    arguments: &Value,
    user_ctx: &UserContext,
    _store: &FashionStore,
) -> Result<String> {
    // Unified PreToolCall gate — Casbin enforcement covering BOTH MCP and
    // skill tools before any routing/execution (fail-closed). The engine
    // already gated at PreToolCall; this keeps route_tool_call safe when
    // invoked directly. Allowed decisions are audited here exactly once
    // (denials were audited upstream by the engine gate).
    crate::rbac::enforce_tool(user_ctx, tool_name)?;
    crate::rbac::audit_tool_decision(user_ctx, tool_name, true);

    // ── MCP routing ──────────────────────────────────────────────────────────
    // Tool names with the `mcp:` prefix are forwarded to a registered MCP
    // server. Format:  mcp:<server-id>/<tool-name>
    // Example:         mcp:local-mock/fabric_search_db
    if let Some(rest) = tool_name.strip_prefix("mcp:") {
        if let Some((server_id, remote_tool)) = rest.split_once('/') {
            let raw = crate::mcp::MCP_MANAGER
                .invoke(server_id, remote_tool, arguments.clone())
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            return Ok(serde_json::json!({
                "status": "ok",
                "mcp_server": server_id,
                "tool": remote_tool,
                "result": serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw)),
            })
            .to_string());
        }
    }

    // Resolve skill/tool via the shared resolver (pure metadata, no data).
    let (skill_id, resolved_tool) =
        resolve_tool_owner(tool_name).ok_or_else(|| {
            AppError::NotFound(format!("Could not resolve tool name: {tool_name}"))
        })?;

    let skill_meta = {
        let engine_guard = SKILL_ENGINE
            .read()
            .map_err(|_| AppError::Internal("SKILL_ENGINE not initialised".into()))?;
        let engine = engine_guard.as_ref().ok_or_else(|| {
            AppError::Internal(
                "SKILL_ENGINE not initialised — call SkillEngine::load_from_dir()".into(),
            )
        })?;
        engine
            .get_skill(&skill_id)
            .ok_or_else(|| AppError::NotFound(format!("Skill '{skill_id}' not found")))?
            .clone()
    };

    let result = execute_skill(&skill_meta, &resolved_tool, arguments, user_ctx).await?;
    Ok(serde_json::json!({
        "status": "ok",
        "skill": skill_id,
        "tool": resolved_tool,
        "result": result,
    })
    .to_string())
}

/// Resolve a tool_name to its owning `(skill_id, tool_name)` using the live
/// SKILL_ENGINE. Pure metadata lookup — reads no business data.
///
/// The read guard is dropped before return (guards are not held across
/// awaits by callers).
pub fn resolve_tool_owner(tool_name: &str) -> Option<(String, String)> {
    let engine_guard = SKILL_ENGINE.read().ok()?;
    let engine = engine_guard.as_ref()?;

    // kebab-case skill id → same set, snake-cased for LLM names.
    let skill_id_lookup: std::collections::HashMap<String, String> = engine
        .loader
        .skill_ids()
        .iter()
        .map(|id| (id.replace('-', "_"), id.clone()))
        .collect();

    // Fully-qualified `<skill_snake>_<tool>` lookup.
    let full_name_lookup: std::collections::HashMap<String, (String, String)> = engine
        .loader
        .list()
        .into_iter()
        .flat_map(|m| {
            m.tools.iter().map(move |t| {
                let key = format!("{}_{}", m.id.replace('-', "_"), t.name);
                (key, (m.id.clone(), t.name.clone()))
            })
        })
        .collect();

    resolve_tool_name(tool_name, &skill_id_lookup, &full_name_lookup).ok()
}

/// Resolve a tool_name string to (skill_id, tool_name).
///
/// Supports:
///   `fabric_query_search_fabric`  → ("fabric-query", "search_fabric")
///   `style_inspiration_trend_analysis` → ("style-inspiration", "trend_analysis")
///   `search_fabric`             → tries to find by tool name; falls back to first skill with that tool
///   `skill_fabric_query_search_fabric` → strips prefix then same as first case
fn resolve_tool_name(
    tool_name: &str,
    skill_id_lookup: &std::collections::HashMap<String, String>,
    full_name_lookup: &std::collections::HashMap<String, (String, String)>,
) -> std::result::Result<(String, String), String> {
    let name = tool_name
        .trim_start_matches("skill_")
        .trim_start_matches("mcp_");

    // Try exact full-name match first.
    if let Some((sid, tname)) = full_name_lookup.get(name) {
        return Ok((sid.clone(), tname.clone()));
    }

    // Try longest-prefix skill match.
    let parts: Vec<&str> = name.split('_').collect();
    if parts.len() >= 2 {
        for end in (1..parts.len()).rev() {
            let skill_snake = parts[..end].join("_");
            if let Some(skill_id) = skill_id_lookup.get(&skill_snake) {
                let tool_name_out = parts[end..].join("_");
                return Ok((skill_id.clone(), tool_name_out));
            }
        }
    }

    Err(name.to_string())
}

/// Execute a skill tool and return structured JSON.
///
/// Phase 1B: fabric/color/style tools run real queries via [`FashionStore`];
/// palette/harmony math is computed locally (`color_theory`). Tools without a
/// data source (e.g. `trend_analysis`) still return canned responses.
pub async fn execute_skill(
    skill: &SkillMetadata,
    tool_name: &str,
    arguments: &Value,
    user_ctx: &UserContext,
) -> Result<Value> {
    let store = FASHION_STORE
        .get()
        .ok_or_else(|| AppError::Internal("FASHION_STORE not initialised".into()))?;
    let tool = skill
        .tools
        .iter()
        .find(|t| t.name == tool_name)
        .ok_or_else(|| AppError::NotFound(format!("Tool '{tool_name}' not found in skill '{}'", skill.id)))?;

    tracing::info!(
        skill = %skill.id,
        tool = %tool.name,
        "Executing skill tool"
    );

    let (org_id, dept_id) = parse_ctx_uuids(user_ctx)?;

    // ── Real implementations per skill+tool ───────────────────────────────────
    let result = match (skill.id.as_str(), tool.name.as_str()) {
        // ── fabric-query ────────────────────────────────────────────────────────
        ("fabric-query", "search_fabric") => {
            let query = arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let limit = arguments
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(10)
                .clamp(1, 100);

            let fabrics = if query.is_empty() {
                store.list_fabrics(org_id, dept_id, limit).await?
            } else {
                store.search_fabrics(org_id, dept_id, &query, limit).await?
            };

            serde_json::json!({
                "query": query,
                "fabrics": fabrics,
                "total": fabrics.len(),
            })
        }

        ("fabric-query", "filter_by_season") => {
            let season = arguments
                .get("season")
                .and_then(|v| v.as_str())
                .unwrap_or("all-season");
            let fabrics = store.filter_fabrics_by_season(org_id, dept_id, season).await?;
            serde_json::json!({
                "season": season,
                "fabrics": fabrics,
                "total": fabrics.len(),
            })
        }

        ("fabric-query", "get_applicable_styles") => {
            let fabric_id = arguments
                .get("fabric_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let fabric_uuid = Uuid::parse_str(fabric_id).map_err(|_| {
                AppError::BadRequest(format!("Invalid fabric_id: {fabric_id}"))
            })?;
            let applicable = store
                .get_applicable_styles(org_id, dept_id, fabric_uuid)
                .await?;
            serde_json::to_value(applicable)?
        }

        // ── color-matching ──────────────────────────────────────────────────────
        ("color-matching", "suggest_palette") => {
            let primary = arguments
                .get("primary_color")
                .and_then(|v| v.as_str())
                .unwrap_or("#6366F1");
            let scheme = arguments
                .get("palette_type")
                .and_then(|v| v.as_str())
                .unwrap_or("complementary");
            let palette = color_theory::build_palette(primary, scheme);
            serde_json::to_value(palette)?
        }

        ("color-matching", "color_harmony") => {
            let colors: Vec<String> = arguments
                .get("colors")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                .unwrap_or_else(|| vec!["#6366F1".to_string(), "#EF4444".to_string()]);
            let harmony = color_theory::analyze_harmony(&colors);
            serde_json::to_value(harmony)?
        }

        ("color-matching", "trend_colors") => {
            let season = arguments.get("season").and_then(|v| v.as_str());
            let category = arguments
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("apparel");
            let trends = store.get_trend_colors(org_id, dept_id, season).await?;
            serde_json::json!({
                "season": season,
                "category": category,
                "trends": trends,
                "total": trends.len(),
            })
        }

        // ── style-inspiration ─────────────────────────────────────────────────
        ("style-inspiration", "generate_style_ideas") => {
            let keywords: Vec<String> = arguments
                .get("keywords")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                .unwrap_or_default();
            let garment_type = arguments
                .get("garment_type")
                .and_then(|v| v.as_str());
            let count = arguments
                .get("count")
                .and_then(|v| v.as_i64())
                .unwrap_or(5)
                .clamp(1, 20);

            let ideas = store
                .generate_style_ideas(org_id, dept_id, &keywords, garment_type, count)
                .await?;

            serde_json::json!({
                "keywords": keywords,
                "garment_type": garment_type,
                "ideas": ideas,
                "total": ideas.len(),
            })
        }

        ("style-inspiration", "style_variations") => {
            let base_style = arguments
                .get("base_style")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let variation_type = arguments
                .get("variation_type")
                .and_then(|v| v.as_str())
                .unwrap_or("all");
            let count = arguments
                .get("count")
                .and_then(|v| v.as_i64())
                .unwrap_or(3)
                .clamp(1, 5);

            let variations = store
                .style_variations(org_id, dept_id, base_style, variation_type, count)
                .await?;

            serde_json::json!({
                "base_style": base_style,
                "variation_type": variation_type,
                "variations": variations,
                "total": variations.len(),
            })
        }

        // No curated data source yet — keep canned response.
        ("style-inspiration", "trend_analysis") => {
            let category = arguments.get("category").and_then(|v| v.as_str()).unwrap_or("all");
            let region = arguments.get("region").and_then(|v| v.as_str()).unwrap_or("global");
            serde_json::json!({
                "category": category, "region": region,
                "trends": [
                    {"id": "trend_001", "name": "柔软结构主义", "description": "在保持设计感的同时追求穿着舒适度，轮廓柔和但不松垮",
                     "heat_index": 92, "key_elements": ["圆润肩线", "垂褶细节", "弹性面料"]},
                    {"id": "trend_002", "name": "可持续时尚", "description": "环保面料与循环设计理念成为主流趋势",
                     "heat_index": 88, "key_elements": ["再生面料", "模块化设计", "天然染色"]},
                    {"id": "trend_003", "name": "数字美学", "description": "数字化设计语言与虚拟时装的灵感融合",
                     "heat_index": 75, "key_elements": ["几何图案", "霓虹色调", "科技面料"]}
                ],
                "note": "Canned response — 趋势数据源待接入"
            })
        }

        _ => {
            return Err(AppError::NotFound(format!(
                "Unknown skill/tool: '{}/{}'",
                skill.id, tool.name
            )));
        }
    };

    Ok(result)
}

/// Parse org/dept UUIDs out of the request's user context.
fn parse_ctx_uuids(user_ctx: &UserContext) -> Result<(Uuid, Uuid)> {
    let org_id = Uuid::parse_str(&user_ctx.org_id).map_err(|_| {
        AppError::BadRequest(format!("Invalid org_id in user_context: {}", user_ctx.org_id))
    })?;
    let dept_id = Uuid::parse_str(&user_ctx.dept_id).map_err(|_| {
        AppError::BadRequest(format!("Invalid dept_id in user_context: {}", user_ctx.dept_id))
    })?;
    Ok((org_id, dept_id))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_skill_id_lookup() {
        // Simulate the lookup map
        let skill_ids = vec!["fabric-query".to_string(), "color-matching".to_string(), "style-inspiration".to_string()];
        let lookup: std::collections::HashMap<String, String> = skill_ids
            .iter()
            .map(|id| (id.replace('-', "_"), id.clone()))
            .collect();
        assert_eq!(lookup.get("fabric_query"), Some(&"fabric-query".to_string()));
        assert_eq!(lookup.get("color_matching"), Some(&"color-matching".to_string()));
        assert_eq!(lookup.get("style_inspiration"), Some(&"style-inspiration".to_string()));
    }
}
