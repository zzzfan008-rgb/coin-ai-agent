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
use tokio::process::Command;
use tokio::time::{sleep, Duration};

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

        // ── dreamina-cli ─────────────────────────────────────────────────────────
        ("dreamina-cli", "user_credit") => {
            let output = Command::new("dreamina")
                .arg("user_credit")
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            serde_json::json!({
                "stdout": stdout.trim(),
                "stderr": stderr.trim(),
                "exit_code": output.status.code(),
            })
        }

        ("dreamina-cli", "list_task") => {
            let gen_status = arguments
                .get("gen_status")
                .and_then(|v| v.as_str());
            let mut cmd = Command::new("dreamina");
            cmd.arg("list_task");
            if let Some(s) = gen_status {
                cmd.arg("--gen_status").arg(s);
            }
            let output = cmd
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).trim(),
                "stderr": String::from_utf8_lossy(&output.stderr).trim(),
                "exit_code": output.status.code(),
            })
        }

        ("dreamina-cli", "text2image") => {
            let prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if prompt.is_empty() {
                return Err(AppError::BadRequest("prompt is required".into()));
            }
            let ratio = arguments
                .get("ratio")
                .and_then(|v| v.as_str())
                .unwrap_or("1:1");
            let resolution = arguments
                .get("resolution_type")
                .and_then(|v| v.as_str())
                .unwrap_or("2k");

            // Submit task
            let output = Command::new("dreamina")
                .args(["text2image", "--prompt", prompt, "--ratio", ratio, "--resolution_type", resolution])
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Parse submit_id from response
            let submit_id = serde_json::from_str::<serde_json::Value>(stdout.trim())
                .ok()
                .and_then(|j| j.get("submit_id").and_then(|v| v.as_str().map(|s| s.to_string())))
                .unwrap_or_default();

            if submit_id.is_empty() {
                return Err(AppError::Internal(format!("Failed to parse submit_id: {}", stdout.trim())));
            }
            tracing::info!(submit_id=%submit_id, "dreamina text2image submitted");

            // Poll until success / fail / max_attempts
            let max_attempts = 24; // ~2 min with backoff
            for attempt in 1..=max_attempts {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    if attempt <= 3 { 5 } else { 10 }
                )).await;

                let poll_out = Command::new("dreamina")
                    .args(["query_result", "--submit_id", &submit_id])
                    .output()
                    .await
                    .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;

                let poll_stdout = String::from_utf8_lossy(&poll_out.stdout);
                let poll_json: serde_json::Value = serde_json::from_str(poll_stdout.trim())
                    .map_err(|e| AppError::Internal(format!("Invalid JSON from query_result: {e}")))?;

                let status = poll_json.get("gen_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                tracing::info!(submit_id=%submit_id, status=%status, attempt=%attempt, "dreamina polling");

                if status == "success" {
                    let images: Vec<String> = poll_json
                        .get("result_json")
                        .and_then(|r| r.get("images"))
                        .and_then(|imgs| imgs.as_array())
                        .map(|arr| {
                            arr.iter().filter_map(|img| {
                                img.get("image_url").and_then(|u| u.as_str()).map(|s| s.to_string())
                            }).collect()
                        })
                        .unwrap_or_default();

                    if !images.is_empty() {
                        return Ok(serde_json::json!({
                            "status": "success",
                            "submit_id": submit_id,
                            "images": images,
                            "message": "图片生成成功"
                        }));
                    }
                } else if status == "fail" {
                    return Ok(serde_json::json!({
                        "status": "fail",
                        "submit_id": submit_id,
                        "message": "图片生成失败"
                    }));
                }
                // else querying → continue polling
            }

            // Timeout
            serde_json::json!({
                "status": "timeout",
                "submit_id": submit_id,
                "message": "生成超时，请稍后手动查询结果"
            })
        }

        ("dreamina-cli", "image2image") => {
            let prompt = arguments.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim();
            let image_path = arguments.get("image_path").and_then(|v| v.as_str()).unwrap_or("").trim();
            if prompt.is_empty() || image_path.is_empty() {
                return Err(AppError::BadRequest("prompt and image_path are required".into()));
            }
            let ratio = arguments.get("ratio").and_then(|v| v.as_str()).unwrap_or("1:1");
            let resolution = arguments.get("resolution_type").and_then(|v| v.as_str()).unwrap_or("2k");

            let output = Command::new("dreamina")
                .args(["image2image", "--prompt", prompt, "--images", image_path, "--ratio", ratio, "--resolution_type", resolution])
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).trim(),
                "stderr": String::from_utf8_lossy(&output.stderr).trim(),
                "exit_code": output.status.code(),
            })
        }

        ("dreamina-cli", "image_upscale") => {
            let image_path = arguments.get("image_path").and_then(|v| v.as_str()).unwrap_or("").trim();
            if image_path.is_empty() {
                return Err(AppError::BadRequest("image_path is required".into()));
            }
            let resolution = arguments.get("resolution_type").and_then(|v| v.as_str()).unwrap_or("2k");
            let output = Command::new("dreamina")
                .args(["image_upscale", "--image", image_path, "--resolution_type", resolution])
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).trim(),
                "stderr": String::from_utf8_lossy(&output.stderr).trim(),
                "exit_code": output.status.code(),
            })
        }

        ("dreamina-cli", "text2video") => {
            let prompt = arguments.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim();
            if prompt.is_empty() {
                return Err(AppError::BadRequest("prompt is required".into()));
            }
            let duration = arguments.get("duration").and_then(|v| v.as_i64()).unwrap_or(5);
            // resolution_type is required for video commands
            let resolution = arguments.get("resolution_type").and_then(|v| v.as_str()).unwrap_or("720p");
            let duration_s = duration.to_string();
            let mut args: Vec<&str> = vec!["text2video", "--prompt", prompt, "--duration", &duration_s, "--video_resolution", resolution];
            if let Some(ratio) = arguments.get("ratio").and_then(|v| v.as_str()) {
                if !ratio.is_empty() {
                    args.push("--ratio");
                    args.push(ratio);
                }
            }
            let output = Command::new("dreamina").args(&args)
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).trim(),
                "stderr": String::from_utf8_lossy(&output.stderr).trim(),
                "exit_code": output.status.code(),
            })
        }

        ("dreamina-cli", "image2video") => {
            let prompt = arguments.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim();
            let image_path = arguments.get("image_path").and_then(|v| v.as_str()).unwrap_or("").trim();
            if prompt.is_empty() || image_path.is_empty() {
                return Err(AppError::BadRequest("prompt and image_path are required".into()));
            }
            let duration = arguments.get("duration").and_then(|v| v.as_i64()).unwrap_or(5);
            let duration_s = duration.to_string();
            let res_default = "720p";
            let mut args: Vec<&str> = vec!["image2video", "--prompt", prompt, "--image", image_path, "--duration", &duration_s, "--video_resolution", res_default];
            if let Some(res) = arguments.get("resolution_type").and_then(|v| v.as_str()) {
                if !res.is_empty() {
                    args.push("--video_resolution");
                    args.push(res);
                }
            }
            let output = Command::new("dreamina").args(&args)
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).trim(),
                "stderr": String::from_utf8_lossy(&output.stderr).trim(),
                "exit_code": output.status.code(),
            })
        }

        ("dreamina-cli", "frames2video") => {
            let prompt = arguments.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim();
            let first = arguments.get("first_frame_path").and_then(|v| v.as_str()).unwrap_or("").trim();
            let last = arguments.get("last_frame_path").and_then(|v| v.as_str()).unwrap_or("").trim();
            if prompt.is_empty() || first.is_empty() || last.is_empty() {
                return Err(AppError::BadRequest("prompt, first_frame_path, last_frame_path are required".into()));
            }
            let duration = arguments.get("duration").and_then(|v| v.as_i64()).unwrap_or(5);
            let duration_s = duration.to_string();
            let res_default = "720p";
            let mut args: Vec<&str> = vec!["frames2video", "--prompt", prompt, "--first", first, "--last", last, "--duration", &duration_s, "--video_resolution", res_default];
            if let Some(ratio) = arguments.get("ratio").and_then(|v| v.as_str()) {
                if !ratio.is_empty() {
                    args.push("--ratio");
                    args.push(ratio);
                }
            }
            let output = Command::new("dreamina").args(&args)
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).trim(),
                "stderr": String::from_utf8_lossy(&output.stderr).trim(),
                "exit_code": output.status.code(),
            })
        }

        ("dreamina-cli", "multimodal2video") => {
            let prompt = arguments.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim();
            let duration = arguments.get("duration").and_then(|v| v.as_i64()).unwrap_or(5);
            let duration_s = duration.to_string();
            let resolution = arguments.get("resolution_type").and_then(|v| v.as_str()).unwrap_or("720p");

            let mut args: Vec<&str> = vec!["multimodal2video", "--duration", &duration_s, "--video_resolution", resolution];
            if !prompt.is_empty() {
                args.push("--prompt");
                args.push(prompt);
            }
            // reference_paths: comma-separated; images → --image, videos → --video, audios → --audio
            if let Some(paths) = arguments.get("reference_paths").and_then(|v| v.as_str()) {
                for p in paths.split(',') {
                    let p = p.trim();
                    if !p.is_empty() {
                        let ext = p.rsplit('.').next().unwrap_or("").to_lowercase();
                        if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp") {
                            args.push("--image");
                        } else if matches!(ext.as_str(), "mp4" | "mov" | "avi" | "mkv" | "webm") {
                            args.push("--video");
                        } else if matches!(ext.as_str(), "mp3" | "wav" | "aac" | "flac" | "m4a") {
                            args.push("--audio");
                        } else {
                            args.push("--image"); // default to image
                        }
                        args.push(p);
                    }
                }
            }
            let output = Command::new("dreamina").args(&args)
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).trim(),
                "stderr": String::from_utf8_lossy(&output.stderr).trim(),
                "exit_code": output.status.code(),
            })
        }

        ("dreamina-cli", "multiframe2video") => {
            let prompt = arguments.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim();
            let image_paths_str = arguments.get("image_paths").and_then(|v| v.as_str()).unwrap_or("").trim();
            let duration = arguments.get("duration").and_then(|v| v.as_f64()).unwrap_or(3.0);
            let duration_s = duration.to_string();
            let resolution = arguments.get("resolution_type").and_then(|v| v.as_str()).unwrap_or("720p");

            if image_paths_str.is_empty() {
                return Err(AppError::BadRequest("image_paths is required".into()));
            }
            let image_paths: Vec<&str> = image_paths_str.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            if image_paths.len() < 2 {
                return Err(AppError::BadRequest("image_paths must contain at least 2 images".into()));
            }

            let mut args: Vec<&str> = vec![
                "multiframe2video",
                "--video_resolution", resolution,
                "--duration", &duration_s,
            ];
            let image_refs: Vec<&str> = image_paths.iter().map(|&s| s).collect();
            for path in &image_refs {
                args.push("--images");
                args.push(path);
            }
            if !prompt.is_empty() {
                args.push("--prompt");
                args.push(prompt);
            }
            let output = Command::new("dreamina").args(&args)
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let submit_id = if output.status.success() && !stdout.is_empty() {
                serde_json::from_str::<serde_json::Value>(&stdout)
                    .ok()
                    .and_then(|j| j.get("submit_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            } else {
                None
            };
            if let Some(submit_id) = submit_id {
                for _ in 0..5 {
                    sleep(Duration::from_secs(5)).await;
                    let q = Command::new("dreamina")
                        .args(["query_result", "--submit_id", &submit_id])
                        .output().await
                        .map_err(|e| AppError::Internal(format!("query_result error: {e}")))?;
                    let qout = String::from_utf8_lossy(&q.stdout).into_owned();
                    let qout_trimmed = qout.trim();
                    if let Ok(r) = serde_json::from_str::<serde_json::Value>(qout_trimmed) {
                        let status = r.get("gen_status").and_then(|v| v.as_str()).unwrap_or("querying");
                        if status == "success" {
                            return Ok(serde_json::json!({"status": "success", "submit_id": submit_id, "result": r}));
                        } else if status == "fail" {
                            return Ok(serde_json::json!({"status": "fail", "submit_id": submit_id, "message": "视频生成失败", "detail": r}));
                        }
                    }
                }
                for _ in 0..12 {
                    sleep(Duration::from_secs(10)).await;
                    let q = Command::new("dreamina")
                        .args(["query_result", "--submit_id", &submit_id])
                        .output().await
                        .map_err(|e| AppError::Internal(format!("query_result error: {e}")))?;
                    let qout = String::from_utf8_lossy(&q.stdout).into_owned();
                    let qout_trimmed = qout.trim();
                    if let Ok(r) = serde_json::from_str::<serde_json::Value>(qout_trimmed) {
                        let status = r.get("gen_status").and_then(|v| v.as_str()).unwrap_or("querying");
                        if status == "success" {
                            return Ok(serde_json::json!({"status": "success", "submit_id": submit_id, "result": r}));
                        } else if status == "fail" {
                            return Ok(serde_json::json!({"status": "fail", "submit_id": submit_id, "message": "视频生成失败", "detail": r}));
                        }
                    }
                }
                return Ok(serde_json::json!({"status": "timeout", "submit_id": submit_id, "message": "生成超时，请稍后用 submit_id 手动查询"}));
            } else {
                serde_json::json!({"stdout": stdout, "stderr": String::from_utf8_lossy(&output.stderr).trim(), "exit_code": output.status.code()})
            }
        }

        ("dreamina-cli", "session_create") => {
            let name = arguments.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
            let mut args = vec!["session", "create"];
            if !name.is_empty() { args.push(name); }
            let output = Command::new("dreamina").args(&args)
                .output().await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({"stdout": String::from_utf8_lossy(&output.stdout).trim(), "exit_code": output.status.code()})
        }

        ("dreamina-cli", "session_list") => {
            let output = Command::new("dreamina")
                .args(["session", "list"])
                .output().await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({"stdout": String::from_utf8_lossy(&output.stdout).trim(), "exit_code": output.status.code()})
        }

        ("dreamina-cli", "session_search") => {
            let name = arguments.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
            if name.is_empty() {
                return Err(AppError::BadRequest("name is required".into()));
            }
            let output = Command::new("dreamina")
                .args(["session", "search", name])
                .output().await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({"stdout": String::from_utf8_lossy(&output.stdout).trim(), "exit_code": output.status.code()})
        }

        ("dreamina-cli", "session_rename") => {
            let id = arguments.get("session_id").and_then(|v| v.as_str()).unwrap_or("").trim();
            let name = arguments.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
            if id.is_empty() || name.is_empty() {
                return Err(AppError::BadRequest("session_id and name are required".into()));
            }
            let output = Command::new("dreamina")
                .args(["session", "rename", id, name])
                .output().await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({"stdout": String::from_utf8_lossy(&output.stdout).trim(), "exit_code": output.status.code()})
        }

        ("dreamina-cli", "session_delete") => {
            let id = arguments.get("session_id").and_then(|v| v.as_str()).unwrap_or("").trim();
            if id.is_empty() {
                return Err(AppError::BadRequest("session_id is required".into()));
            }
            let output = Command::new("dreamina")
                .args(["session", "rm", id])
                .output().await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({"stdout": String::from_utf8_lossy(&output.stdout).trim(), "exit_code": output.status.code()})
        }

        ("dreamina-cli", "query_result") => {
            let submit_id = arguments.get("submit_id").and_then(|v| v.as_str()).unwrap_or("").trim();
            if submit_id.is_empty() {
                return Err(AppError::BadRequest("submit_id is required".into()));
            }
            let mut cmd = Command::new("dreamina");
            cmd.args(["query_result", "--submit_id", submit_id]);
            if let Some(dir) = arguments.get("download_dir").and_then(|v| v.as_str()) {
                if !dir.is_empty() {
                    cmd.arg("--download_dir").arg(dir);
                }
            }
            let output = cmd
                .output()
                .await
                .map_err(|e| AppError::Internal(format!("dreamina CLI error: {e}")))?;
            serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).trim(),
                "stderr": String::from_utf8_lossy(&output.stderr).trim(),
                "exit_code": output.status.code(),
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
