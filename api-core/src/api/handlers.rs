//! HTTP handlers for api-core endpoints (aligned with api/core.yml).

use std::sync::Arc;

use sqlx::Row;
use axum::{
    extract::{Multipart, Path, Query, State, Request},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::agent::{AgentConfig, AgentEngine};
use crate::llm::messages::ChatMessage;
use crate::llm::tools::ToolDefinition;
use crate::session::{Message, Session};
use crate::AppServices;
use crate::AppState;

// ── Request / response schemas ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CoreChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_true")]
    pub stream: bool,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub extra_body: Option<ExtraBody>,
    pub user_context: UserContext,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct ExtraBody {
    pub session_id: Option<String>,
    pub skill_ids: Option<Vec<String>>,
    pub mcp_server_ids: Option<Vec<String>>,
    pub knowledge_collections: Option<Vec<String>>,
    /// Override the model for this request (e.g. "deepseek-chat").
    pub model: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserContext {
    pub user_id: String,
    pub org_id: String,
    pub dept_id: String,
    pub role: String,
    #[serde(default)]
    pub ip_address: Option<String>,
    /// Phase 1B: flat list of permission strings, e.g. ["skill:fabric-query", "tool:search_fabric"]
    #[serde(default)]
    pub extra_permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoreChatResponse {
    id: String,
    object: String,
    created: i64,
    model: String,
    choices: Vec<Value>,
    usage: Value,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub org_id: String,
    pub dept_id: String,
    pub user_id: String,
    pub title: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveMessageRequest {
    pub role: String,
    pub content: String,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
    pub token_count: Option<i32>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct MessageQuery {
    pub limit: Option<usize>,
    pub before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct SkillExecuteRequest {
    pub skill_id: String,
    pub tool_name: String,
    pub parameters: Value,
    pub user_context: UserContext,
}

#[derive(Debug, Serialize)]
pub struct SkillExecuteResponse {
    pub skill_id: String,
    pub tool_name: String,
    pub result: Value,
    pub tokens_used: i64,
    pub duration_ms: i64,
}

// ── Health ─────────────────────────────────────────────────────────────────────

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_ok = state.session_store.health_check().await.unwrap_or(false);
    let all_llm = state.llm_client.health_check_all().await;
    let default_llm = all_llm
        .get(&state.config.llm_provider)
        .copied()
        .unwrap_or(false);

    Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "services": {
            "database": if db_ok { "ok" } else { "down" },
            "llm": if default_llm { "ok" } else { "down" },
            "llm_providers": all_llm,
        }
    }))
}

// ── Chat ───────────────────────────────────────────────────────────────────────

pub async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<CoreChatRequest>,
) -> Response {
    if req.stream {
        return stream_chat(state, req).await;
    }

    // Take ownership of request fields (extra_body is consumed below).
    let CoreChatRequest {
        model: req_model,
        mut messages,
        stream: _,
        temperature: _,
        max_tokens: _,
        extra_body,
        user_context,
    } = req;

    let engine = AgentEngine::new(
        AgentConfig::default(),
        state.llm_client.clone(),
        state.session_store.clone(),
        Some(state.rag_retriever.clone()),
    );

    let model = extra_body
        .as_ref()
        .and_then(|b| b.model.clone())
        .or_else(|| {
            if req_model.is_empty() || req_model == "__default__" {
                None
            } else {
                Some(req_model)
            }
        });

    let collections = extra_body
        .as_ref()
        .and_then(|b| b.knowledge_collections.clone());

    // Optional session for persisting the assistant reply + token usage.
    let session_id = extra_body
        .as_ref()
        .and_then(|b| b.session_id.clone());

    // ── Intent routing (T-014) ─────────────────────────────────────────────
    // 用户未手动指定 skill_ids 时，对最后一条 user 消息做规则分类，
    // 自动路由到对应 skill（general 不强制 skill，走通用对话）。
    let manual_skill_ids = extra_body
        .as_ref()
        .and_then(|b| b.skill_ids.clone())
        .filter(|ids| !ids.is_empty());

    let last_user: String = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.clone())
        .unwrap_or_default();

    let intent = state.intent_router.classify(&last_user);

    let (skill_ids, route_source): (Vec<String>, &str) = match manual_skill_ids {
        Some(ids) => (ids, "manual"),
        None => match &intent.skill_id {
            Some(sid) => (vec![sid.clone()], "intent"),
            None => (vec![], "general"),
        },
    };

    // 路由命中 skill 时（无论手动指定还是意图自动分类），把该 skill 的
    // 描述作为 system 消息注入上下文，引导模型调用其工具并按专业角色回答。
    if route_source == "intent" || route_source == "manual" {
        if let Some(system_prompt) = skill_system_prompt(&skill_ids) {
            messages.insert(0, ChatMessage::system(&system_prompt));
        }
    }

    tracing::info!(
        input = %last_user.chars().take(80).collect::<String>(),
        intent = %intent.intent,
        skill = ?intent.skill_id,
        confidence = intent.confidence,
        matched = ?intent.matched_keywords,
        candidates = intent.candidates.len(),
        route = route_source,
        "Intent classification decision"
    );

    // Build LLM tool definitions from the effective skill ids.
    // qualified tool names: skill_<skill>_<tool>; empty → no tools exposed.
    let skill_tools: Option<Vec<ToolDefinition>> = if skill_ids.is_empty() {
        None
    } else {
        let guard = match crate::skill_engine::SKILL_ENGINE.read() {
            Ok(g) => g,
            Err(_) => {
                return crate::error::AppError::Internal("SKILL_ENGINE poisoned".into())
                    .into_response()
            }
        };
        let engine = match guard.as_ref() {
            Some(e) => e,
            None => {
                return crate::error::AppError::Internal("SKILL_ENGINE not initialised".into())
                    .into_response()
            }
        };
        Some(engine.registry.to_llm_tools_for_skills(&skill_ids))
    };

    let (reply, usage) = match engine
        .run_turn(
            &messages,
            model.as_deref(),
            skill_tools.as_deref(),
            &user_context,
            collections.as_deref(),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };

    let resolved_model = model
        .as_deref()
        .or_else(|| {
            if state.config.llm_provider == "deepseek" {
                Some(state.config.deepseek_model.as_str())
            } else {
                Some(state.config.minimax_model.as_str())
            }
        })
        .unwrap_or("unknown");

    // Persist the assistant reply with token usage when a session is given.
    // A persistence failure is logged but does not fail the request — the
    // caller already has their answer.
    if let Some(ref sid) = session_id {
        if let Ok(session_uuid) = Uuid::parse_str(sid) {
            if let Err(e) = state
                .session_store
                .save_message(
                    session_uuid,
                    "assistant",
                    &reply,
                    Some(resolved_model),
                    Some("stop"),
                    Some(usage.total_tokens as i32),
                    Some(usage.prompt_tokens as i32),
                    Some(usage.completion_tokens as i32),
                    None,
                )
                .await
            {
                tracing::warn!("Failed to persist assistant message tokens: {e}");
            }
        } else {
            tracing::warn!("Invalid session_id in extra_body: {sid}");
        }
    }

    let response = CoreChatResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: Utc::now().timestamp(),
        model: resolved_model.to_string(),
        choices: vec![serde_json::json!({
            "index": 0,
            "message": {"role": "assistant", "content": reply},
            "finish_reason": "stop"
        })],
        usage: serde_json::json!({
            "prompt_tokens": usage.prompt_tokens,
            "completion_tokens": usage.completion_tokens,
            "total_tokens": usage.total_tokens
        }),
    };

    Json(response).into_response()
}

pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<CoreChatRequest>,
) -> Response {
    stream_chat(state, req).await
}

/// Build the SSE response. The turn runs through the same agent loop as the
/// non-streaming path (skill prompt injection + tool definitions + tool-call
/// execution), so manual/intent skill selection behaves identically. The final
/// reply is emitted as a single SSE chunk (functional parity over word-by-word
/// streaming).
async fn stream_chat(state: Arc<AppServices>, req: CoreChatRequest) -> Response {
    let (tx, rx) = tokio::sync::broadcast::channel::<Bytes>(128);

    let CoreChatRequest {
        model: req_model,
        mut messages,
        extra_body,
        user_context,
        ..
    } = req;

    // Resolve model: extra_body.model overrides the top-level model field.
    let model = extra_body
        .as_ref()
        .and_then(|b| b.model.clone())
        .or_else(|| {
            if req_model.is_empty() || req_model == "__default__" {
                None
            } else {
                Some(req_model)
            }
        });

    let collections = extra_body
        .as_ref()
        .and_then(|b| b.knowledge_collections.clone());
    let session_id = extra_body.as_ref().and_then(|b| b.session_id.clone());

    // ── Skill routing (manual first, intent fallback) ───────────────────────
    let manual_skill_ids = extra_body
        .as_ref()
        .and_then(|b| b.skill_ids.clone())
        .filter(|ids| !ids.is_empty());

    let last_user: String = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.clone())
        .unwrap_or_default();

    let intent = state.intent_router.classify(&last_user);

    let (skill_ids, route_source): (Vec<String>, &str) = match manual_skill_ids {
        Some(ids) => (ids, "manual"),
        None => match &intent.skill_id {
            Some(sid) => (vec![sid.clone()], "intent"),
            None => (vec![], "general"),
        },
    };

    if route_source == "intent" || route_source == "manual" {
        if let Some(system_prompt) = skill_system_prompt(&skill_ids) {
            messages.insert(0, ChatMessage::system(&system_prompt));
        }
    }

    tracing::info!(
        input = %last_user.chars().take(80).collect::<String>(),
        intent = %intent.intent,
        skill = ?intent.skill_id,
        confidence = intent.confidence,
        route = route_source,
        stream = true,
        "Intent classification decision"
    );

    // Build LLM tool definitions from the effective skill ids.
    let skill_tools: Option<Vec<ToolDefinition>> = if skill_ids.is_empty() {
        None
    } else {
        let guard = crate::skill_engine::SKILL_ENGINE.read();
        match guard {
            Ok(g) => match g.as_ref() {
                Some(engine) => Some(engine.registry.to_llm_tools_for_skills(&skill_ids)),
                None => None,
            },
            Err(_) => None,
        }
    };

    let session_store = state.session_store.clone();
    let engine = AgentEngine::new(
        AgentConfig::default(),
        state.llm_client.clone(),
        state.session_store.clone(),
        Some(state.rag_retriever.clone()),
    );

    tokio::spawn(async move {
        let (reply, usage) = match engine
            .run_turn(
                &messages,
                model.as_deref(),
                skill_tools.as_deref(),
                &user_context,
                collections.as_deref(),
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                let payload =
                    serde_json::json!({"error": {"code": "LLM_ERROR", "message": e.to_string()}});
                let _ = tx.send(Bytes::from(format!("data: {payload}\n\n")));
                return;
            }
        };

        // Persist the assistant reply when a session is given (best-effort).
        if let Some(ref sid) = session_id {
            if let Ok(session_uuid) = Uuid::parse_str(sid) {
                let resolved = model.clone().unwrap_or_else(|| "unknown".into());
                let _ = session_store.save_message(
                    session_uuid,
                    "assistant",
                    &reply,
                    Some(&resolved),
                    Some("stop"),
                    Some(usage.total_tokens as i32),
                    Some(usage.prompt_tokens as i32),
                    Some(usage.completion_tokens as i32),
                    None,
                ).await;
            }
        }

        let chunk = serde_json::json!({
            "id": format!("chatcmpl-{}", Uuid::new_v4()),
            "object": "chat.completion.chunk",
            "model": model.clone().unwrap_or_else(|| "unknown".into()),
            "choices": [{
                "index": 0,
                "delta": {"role": "assistant", "content": reply},
                "finish_reason": "stop"
            }],
        });
        let _ = tx.send(Bytes::from(format!("data: {chunk}\n\n")));
        let _ = tx.send(Bytes::from_static(b"data: [DONE]\n\n"));
    });

    use std::pin::pin;
    use std::time::Duration;
    use tokio::time::interval;
    use futures::StreamExt as Fs; // T-022: for filter_map

    let idle_secs = state.config.sse_idle_timeout_secs;
    // T-022: keepalive interval = min(45s, idle/2) so at least one ping
    // lands before the 60s EventSource reconnect threshold.
    let keepalive_interval_secs = idle_secs.min(45).max(10) / 2;

    // Separate channel so keepalive ticks don't block data delivery.
    let (ka_tx, _ka_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Keepalive ticker: fires SSE comment events at keepalive_interval_secs.
    // The `:` comment format is silently dropped by EventSource but resets
    // its 60s reconnect timer, preventing spurious mid-stream reconnects.
    // We emit raw Bytes (SSE wire format) so Body::from_stream can consume
    // the stream directly without needing Sse::new (which requires Ok=Event).
    let ka_stream = Box::pin(async_stream::stream! {
        let mut ticker = pin!(interval(Duration::from_secs(keepalive_interval_secs)));
        loop {
            ticker.as_mut().tick().await;
            let _ = ka_tx.send(());
            // SSE comment: ": keepalive\n\n" — no data/event/id fields.
            yield Ok::<_, std::convert::Infallible>(Bytes::from_static(b": keepalive\n\n"));
        }
    });

    // Data events: forward non-empty broadcast chunks as SSE data events.
    // Fs::filter_map drops empty keepalive-ping slots and receiver errors.
    // Emit raw Bytes so the merged stream is TryStream<Ok = Bytes>.
    let data_stream = Box::pin(Fs::filter_map(BroadcastStream::new(rx), |r| async move {
        match r {
            Ok(bytes) if !bytes.is_empty() => {
                // SSE data event: "data: <content>\n\n"
                let sse = format!("data: {}\n\n", String::from_utf8_lossy(&bytes));
                Some(Ok::<_, std::convert::Infallible>(Bytes::from(sse)))
            }
            Ok(_) | Err(_) => None,
        }
    }));

    // Merge both streams. Both emit Bytes so merged is TryStream<Ok = Bytes>.
    let merged = tokio_stream::StreamExt::merge(data_stream, ka_stream);

    let mut response = axum::response::Response::new(
        axum::body::Body::from_stream(merged),
    );
    let headers = response.headers_mut();
    headers.insert(axum::http::header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
    headers.insert(axum::http::header::CACHE_CONTROL, "no-cache".parse().unwrap());
    headers.insert(axum::http::header::CONNECTION, "keep-alive".parse().unwrap());
    // T-022: disable proxy/nginx buffering for SSE
    headers.insert(
        axum::http::header::HeaderName::from_static("x-accel-buffering"),
        "no".parse().unwrap(),
    );
    // T-022: expose configured timeouts for gateway / debugging
    headers.insert(
        axum::http::header::HeaderName::from_static("x-sse-idle-timeout"),
        idle_secs.to_string().parse().unwrap(),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("x-sse-write-timeout"),
        state.config.sse_write_timeout_secs.to_string().parse().unwrap(),
    );
    response
}

fn active_model(state: &AppState) -> String {
    match state.config.llm_provider.as_str() {
        "deepseek" => state.config.deepseek_model.clone(),
        _ => state.config.minimax_model.clone(),
    }
}

// ── Sessions ───────────────────────────────────────────────────────────────────

pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<Session>, crate::error::AppError> {
    let org_id = parse_uuid("org_id", &req.org_id)?;
    let dept_id = parse_uuid("dept_id", &req.dept_id)?;
    let user_id = parse_uuid("user_id", &req.user_id)?;
    let session = state
        .session_store
        .create_session(org_id, dept_id, user_id, req.title.as_deref(), req.model.as_deref())
        .await?;
    Ok(Json(session))
}

pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Session>, crate::error::AppError> {
    Ok(Json(state.session_store.get_session(id).await?))
}

pub async fn get_messages(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<MessageQuery>,
) -> Result<Json<Value>, crate::error::AppError> {
    let messages = state
        .session_store
        .get_messages(id, q.limit.unwrap_or(50), q.before)
        .await?;
    Ok(Json(serde_json::json!({
        "messages": messages,
        "has_more": false,
        "next_cursor": Value::Null,
    })))
}

pub async fn save_message(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SaveMessageRequest>,
) -> Result<Json<Message>, crate::error::AppError> {
    let msg = state
        .session_store
        .save_message(
            id,
            &req.role,
            &req.content,
            req.model.as_deref(),
            req.finish_reason.as_deref(),
            req.token_count,
            req.input_tokens,
            req.output_tokens,
            req.metadata,
        )
        .await?;
    Ok(Json(msg))
}

// ── Skills ─────────────────────────────────────────────────────────────────────

pub async fn list_skills() -> Json<serde_json::Value> {
    let guard = crate::skill_engine::SKILL_ENGINE
        .read()
        .expect("SKILL_ENGINE poisoned");
    match guard.as_ref() {
        Some(engine) => {
            let skills: Vec<_> = engine
                .list_skills()
                .into_iter()
                .map(|s| serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "version": s.version,
                    "description": s.description,
                    "tools": s.tools
                }))
                .collect();
            Json(serde_json::json!({ "skills": skills }))
        }
        None => Json(serde_json::json!({ "skills": [], "error": "Skill engine not initialised" })),
    }
}

pub async fn execute_skill(
    Json(req): Json<SkillExecuteRequest>,
) -> Response {
    use crate::skill_engine::{check_skill_permission, execute_skill as exec_skill};

    let started = std::time::Instant::now();

    // Clone skill metadata then drop the read guard BEFORE any await,
    // because std::sync::RwLockReadGuard is !Send.
    let skill_meta = {
        let engine_guard = match crate::skill_engine::SKILL_ENGINE.read() {
            Ok(g) => g,
            Err(_) => {
                return crate::error::AppError::Internal("SKILL_ENGINE poisoned".into())
                    .into_response()
            }
        };
        let engine = match engine_guard.as_ref() {
            Some(e) => e,
            None => {
                return crate::error::AppError::Internal("SKILL_ENGINE not initialised".into())
                    .into_response()
            }
        };
        match engine.get_skill(&req.skill_id) {
            Some(s) => s.clone(),
            None => {
                return crate::error::AppError::NotFound(format!(
                    "Skill '{}' not found",
                    req.skill_id
                ))
                .into_response()
            }
        }
    }; // guard dropped here

    // Permission gate — returns 403 if user lacks skill:{id}
    if let Err(e) = check_skill_permission(&req.user_context, &skill_meta) {
        return e.into_response();
    }

    // Execute
    let result = match exec_skill(&skill_meta, &req.tool_name, &req.parameters, &req.user_context).await {
        Ok(r) => r,
        Err(e) => {
            return crate::error::AppError::SkillError(e.to_string()).into_response()
        }
    };

    Json(SkillExecuteResponse {
        skill_id: req.skill_id,
        tool_name: req.tool_name,
        result,
        tokens_used: 0,
        duration_ms: started.elapsed().as_millis() as i64,
    })
    .into_response()
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Build a skill-context system prompt from the routed skills' metadata.
/// Returns None if any skill can't be resolved (read guard dropped before return).
fn skill_system_prompt(skill_ids: &[String]) -> Option<String> {
    let guard = crate::skill_engine::SKILL_ENGINE.read().ok()?;
    let engine = guard.as_ref()?;
    let mut sections = Vec::new();
    for sid in skill_ids {
        if let Some(meta) = engine.get_skill(sid) {
            let detail = meta
                .long_description
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| meta.description.clone());
            sections.push(format!(
                "- Skill `{}` ({}):\n{}",
                meta.id, meta.name, detail
            ));
        }
    }
    if sections.is_empty() {
        return None;
    }
    Some(format!(
        "当前对话已启用以下专业技能。请优先调用其工具获取真实数据，并按照对应 skill 的专业角色与规范回答用户问题：\n\n{}",
        sections.join("\n\n")
    ))
}

fn parse_uuid(field: &str, raw: &str) -> Result<Uuid, crate::error::AppError> {
    Uuid::parse_str(raw)
        .map_err(|_| crate::error::AppError::BadRequest(format!("Invalid {field}")))
}

// ══════════════════════════════════════════════════════════════════════════════
// Knowledge Base — RAG search & document upload (T-015)
// ══════════════════════════════════════════════════════════════════════════════

// ── Schemas ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct KnowledgeSearchRequest {
    pub query: String,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub dept_id: Option<String>,
    #[serde(default = "default_knowledge_top_k")]
    pub top_k: usize,
}

fn default_knowledge_top_k() -> usize {
    5
}

#[derive(Debug, Serialize)]
pub struct KnowledgeSearchResponse {
    pub query: String,
    pub results: Vec<KnowledgeHit>,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeHit {
    pub text: String,
    pub score: f32,
    pub doc_id: String,
    pub title: String,
    pub chunk_index: i32,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeDocumentResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub dept_id: Uuid,
    pub title: String,
    pub file_type: String,
    pub file_size: i64,
    pub status: String,
}

// ── POST /internal/knowledge/search ──────────────────────────────────────────

/// Embed the query and run an ANN search scoped to the caller's org+dept.
pub async fn knowledge_search(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<KnowledgeSearchRequest>,
) -> Result<Json<KnowledgeSearchResponse>, crate::error::AppError> {
    if req.query.trim().is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "Query must not be empty".into(),
        ));
    }

    // Identity: gateway headers first, request fields for internal calls.
    let (h_org, h_dept, _) = header_identity(&headers);
    let org_id = h_org.or(req.org_id).ok_or_else(|| {
        crate::error::AppError::BadRequest("org_id required".into())
    })?;
    let dept_id = h_dept.or(req.dept_id).ok_or_else(|| {
        crate::error::AppError::BadRequest("dept_id required".into())
    })?;

    // Validate UUIDs up front for clearer error messages.
    parse_uuid("org_id", &org_id)?;
    parse_uuid("dept_id", &dept_id)?;

    let top_k = req.top_k.clamp(1, 50);

    let vector = state
        .rag_retriever
        .embed_text(&req.query)
        .await
        .map_err(|e| crate::error::AppError::LlmError(e.to_string()))?;

    let hits = state
        .rag_retriever
        .search(&vector, &org_id, &dept_id, top_k)
        .await
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    let results: Vec<KnowledgeHit> = hits
        .into_iter()
        .map(|h| {
            let p = &h.payload;
            KnowledgeHit {
                text: p.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                score: h.score,
                doc_id: p.get("doc_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                title: p.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                chunk_index: p.get("chunk_index").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            }
        })
        .collect();

    Ok(Json(KnowledgeSearchResponse {
        query: req.query,
        results,
    }))
}

// ── POST /api/knowledge/documents ────────────────────────────────────────────

/// Extract identity injected by the API gateway from the verified JWT.
/// Returns (org_id, dept_id, user_id) when present.
pub fn header_identity(
    headers: &axum::http::HeaderMap,
) -> (Option<String>, Option<String>, Option<String>) {
    let get = |key: &str| {
        headers
            .get(key)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    (get("x-auth-org-id"), get("x-auth-dept-id"), get("x-auth-user-id"))
}

/// Accept a multipart file upload, persist a DB record, and kick off
/// background indexing (parse → chunk → embed → Qdrant).
pub async fn upload_knowledge_document(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<KnowledgeDocumentResponse>, crate::error::AppError> {
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut original_filename = String::new();
    let mut org_id_raw = String::new();
    let mut dept_id_raw = String::new();
    let mut title = String::new();
    let mut uploaded_by_raw = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| crate::error::AppError::BadRequest(format!("Invalid multipart: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                original_filename = field
                    .file_name()
                    .unwrap_or("uploaded")
                    .to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| crate::error::AppError::BadRequest(format!("Read file: {e}")))?;
                if data.len() > 50 * 1024 * 1024 {
                    return Err(crate::error::AppError::BadRequest(
                        "File exceeds 50MB limit".into(),
                    ));
                }
                file_bytes = data.to_vec();
            }
            "org_id" => org_id_raw = field.text().await.unwrap_or_default(),
            "dept_id" => dept_id_raw = field.text().await.unwrap_or_default(),
            "title" => title = field.text().await.unwrap_or_default(),
            "uploaded_by" => uploaded_by_raw = field.text().await.unwrap_or_default(),
            _ => {} // ignore unknown fields
        }
    }

    // Validate inputs.
    if file_bytes.is_empty() {
        return Err(crate::error::AppError::BadRequest("File is empty".into()));
    }

    // Trust gateway-injected identity over multipart fields (the latter are
    // only honoured on direct/internal calls without X-Auth-* headers).
    let (h_org, h_dept, h_user) = header_identity(&headers);
    if let Some(v) = h_org {
        org_id_raw = v;
    }
    if let Some(v) = h_dept {
        dept_id_raw = v;
    }
    if let Some(v) = h_user {
        uploaded_by_raw = v;
    }

    let org_id = parse_uuid("org_id", &org_id_raw)?;
    let dept_id = parse_uuid("dept_id", &dept_id_raw)?;
    let uploaded_by = if uploaded_by_raw.is_empty() {
        None
    } else {
        Some(parse_uuid("uploaded_by", &uploaded_by_raw)?)
    };

    // Derive file type from extension.
    let file_type = std::path::Path::new(&original_filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .filter(|e| matches!(e.as_str(), "pdf" | "docx" | "txt" | "md"))
        .ok_or_else(|| {
            crate::error::AppError::BadRequest(
                "Unsupported file type — allowed: pdf, docx, txt, md".into(),
            )
        })?;

    if title.is_empty() {
        // Fall back to filename without extension.
        title = std::path::Path::new(&original_filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();
    }

    let file_size = file_bytes.len() as i64;

    // Persist the uploaded file to a local directory.
    // NOTE: In production this will be replaced by a MinIO put_object call;
    // the file_path column stores the object key.
    let doc_id = Uuid::new_v4();
    let upload_dir = std::path::Path::new("uploads/knowledge");
    tokio::fs::create_dir_all(upload_dir)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Create upload dir: {e}")))?;
    let local_path = upload_dir.join(format!("{doc_id}.{file_type}"));
    tokio::fs::write(&local_path, &file_bytes)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Write file: {e}")))?;

    let file_path = local_path.to_string_lossy().to_string();

    // Insert DB record (status=pending).
    let row = sqlx::query(
        r#"INSERT INTO knowledge_documents
               (id, org_id, dept_id, title, file_type, file_path, file_size, uploaded_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id, org_id, dept_id, title, file_type, file_size, status"#,
    )
    .bind(doc_id)
    .bind(org_id)
    .bind(dept_id)
    .bind(&title)
    .bind(&file_type)
    .bind(&file_path)
    .bind(file_size)
    .bind(uploaded_by)
    .fetch_one(state.session_store.pool())
    .await?;

    // Spawn background indexing. The task owns its own clones so the HTTP
    // request can return immediately.
    let pool = state.session_store.pool().clone();
    let retriever = state.rag_retriever.clone();
    let doc_id_str = doc_id.to_string();
    let org_id_str = org_id.to_string();
    let dept_id_str = dept_id.to_string();
    let title_clone = title.clone();
    let bytes_clone = file_bytes.clone();
    let ftype_clone = file_type.clone();

    tokio::spawn(async move {
        match crate::rag::DocumentIndexer::index_document(
            &retriever,
            &doc_id_str,
            &org_id_str,
            &dept_id_str,
            &title_clone,
            &bytes_clone,
            &ftype_clone,
            &pool,
        )
        .await
        {
            Ok(n) => tracing::info!(%doc_id_str, chunks = n, "Background indexing finished"),
            Err(e) => tracing::error!(%doc_id_str, "Background indexing failed: {e}"),
        }
    });

    Ok(Json(KnowledgeDocumentResponse {
        id: row.get("id"),
        org_id: row.get("org_id"),
        dept_id: row.get("dept_id"),
        title: row.get("title"),
        file_type: row.get("file_type"),
        file_size: row.get("file_size"),
        status: row.get("status"),
    }))
}

// ── DELETE /api/knowledge/documents/:id ─────────────────────────────────────

/// Soft-delete a knowledge document and remove its vectors from Qdrant.
///
/// Scoped by org + dept from the (trusted internal) query params
/// `org_id` / `dept_id` so a caller cannot delete another tenant's document
/// by guessing a UUID. Sets `status=deleted`, `deleted_at=NOW()` and
/// deletes every Qdrant point whose payload `doc_id` matches.
pub async fn delete_knowledge_document(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(doc_id): Path<Uuid>,
    Query(q): Query<KnowledgeDocDeleteQuery>,
) -> Result<Json<Value>, crate::error::AppError> {
    let (h_org, h_dept, _) = header_identity(&headers);
    let org_id = h_org
        .as_deref()
        .map(Uuid::parse_str)
        .and_then(Result::ok)
        .or(q.org_id)
        .ok_or_else(|| crate::error::AppError::BadRequest("org_id required".into()))?;
    let dept_id = h_dept
        .as_deref()
        .map(Uuid::parse_str)
        .and_then(Result::ok)
        .or(q.dept_id)
        .ok_or_else(|| crate::error::AppError::BadRequest("dept_id required".into()))?;

    let res = sqlx::query(
        r#"UPDATE knowledge_documents
           SET status = 'deleted', deleted_at = NOW()
           WHERE id = $1 AND org_id = $2 AND dept_id = $3
             AND deleted_at IS NULL"#,
    )
    .bind(doc_id)
    .bind(org_id)
    .bind(dept_id)
    .execute(state.session_store.pool())
    .await?;

    if res.rows_affected() == 0 {
        return Err(crate::error::AppError::NotFound(format!(
            "knowledge document {doc_id} not found in this org/dept"
        )));
    }

    // Remove vectors (best effort — doc is already marked deleted).
    if let Err(e) = state
        .rag_retriever
        .delete_document_vectors(&doc_id.to_string())
        .await
    {
        tracing::warn!(%doc_id, "Qdrant vector deletion failed: {e}");
    }

    Ok(Json(serde_json::json!({
        "id": doc_id.to_string(),
        "status": "deleted",
    })))
}

#[derive(Debug, Deserialize)]
pub struct KnowledgeDocDeleteQuery {
    #[serde(default)]
    org_id: Option<Uuid>,
    #[serde(default)]
    dept_id: Option<Uuid>,
}

// ── GET /api/knowledge/documents ─────────────────────────────────────────────

/// List knowledge documents scoped to the caller's org + dept.
/// Field names are mapped to the frontend `KnowledgeDocument` contract
/// (filename=title, error=error_message, created_at=uploaded_at, ...).
pub async fn list_knowledge_documents(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<KnowledgeDocDeleteQuery>,
) -> Result<Json<Value>, crate::error::AppError> {
    let (h_org, h_dept, _) = header_identity(&headers);
    let org = h_org
        .as_deref()
        .map(Uuid::parse_str)
        .and_then(Result::ok)
        .or(q.org_id)
        .ok_or_else(|| crate::error::AppError::BadRequest("org_id required".into()))?;
    let dept = h_dept
        .as_deref()
        .map(Uuid::parse_str)
        .and_then(Result::ok)
        .or(q.dept_id)
        .ok_or_else(|| crate::error::AppError::BadRequest("dept_id required".into()))?;

    let rows = sqlx::query(
        r#"SELECT id, org_id, dept_id, title, file_type, status, chunk_count,
                  uploaded_by, error_message, uploaded_at, processed_at
           FROM knowledge_documents
           WHERE org_id = $1 AND dept_id = $2 AND deleted_at IS NULL
           ORDER BY uploaded_at DESC"#,
    )
    .bind(org)
    .bind(dept)
    .fetch_all(state.session_store.pool())
    .await?;

    let documents: Vec<Value> = rows
        .iter()
        .map(|r| {
            let uploaded_at: chrono::DateTime<chrono::Utc> = r.get("uploaded_at");
            let processed_at: Option<chrono::DateTime<chrono::Utc>> =
                r.try_get("processed_at").ok();
            serde_json::json!({
                "id": r.get::<Uuid, _>("id").to_string(),
                "org_id": r.get::<Uuid, _>("org_id").to_string(),
                "dept_id": r.get::<Uuid, _>("dept_id").to_string(),
                "filename": r.get::<String, _>("title"),
                "file_type": r.get::<String, _>("file_type"),
                "status": r.get::<String, _>("status"),
                "chunk_count": r.get::<i32, _>("chunk_count"),
                "uploaded_by": r.try_get::<Uuid, _>("uploaded_by").ok().map(|u| u.to_string()),
                "error": r.try_get::<String, _>("error_message").ok(),
                "created_at": uploaded_at,
                "updated_at": processed_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "documents": documents,
        "total": documents.len(),
    })))
}

// ── POST /internal/intent/classify ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct IntentClassifyRequest {
    pub message: String,
}

/// Classify a message: intent, skill routing, confidence and candidates.
pub async fn intent_classify(
    State(state): State<AppState>,
    Json(req): Json<IntentClassifyRequest>,
) -> Result<Json<crate::intent::IntentResult>, crate::error::AppError> {
    Ok(Json(state.intent_router.classify(&req.message)))
}

// ── GET /api/mcp/servers/:id/tools ───────────────────────────────────────────

/// List the tools exposed by a registered MCP server.
pub async fn mcp_server_tools(
    State(_state): State<AppState>,
    Path(server_id): Path<String>,
) -> Result<Json<serde_json::Value>, crate::error::AppError> {
    let tools = crate::mcp::MCP_MANAGER
        .list_tools(&server_id)
        .await
        .map_err(|e| crate::error::AppError::NotFound(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "server_id": server_id,
        "tools": tools,
    })))
}

// ── GET /api/mcp/servers ─────────────────────────────────────────────────────

/// List all registered MCP server IDs plus their health status.
pub async fn mcp_servers(
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    let health = crate::mcp::MCP_MANAGER.health_check().await;
    let servers: Vec<serde_json::Value> = health
        .into_iter()
        .map(|(id, healthy)| serde_json::json!({ "id": id, "healthy": healthy }))
        .collect();
    Json(serde_json::json!({ "servers": servers }))
}

// ══════════════════════════════════════════════════════════════════════════════
// CLIP 以图搜图 — image upload, list, and similarity search (T-019)
// ══════════════════════════════════════════════════════════════════════════════

use axum::extract::FromRequest;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StyleImageRecord {
    pub id: Uuid,
    pub org_id: Uuid,
    pub dept_id: Uuid,
    pub style_id: Option<Uuid>,
    pub image_path: String,
    pub file_type: Option<String>,
    pub uploaded_by: Option<Uuid>,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SimilarImagesJsonRequest {
    /// Raw base64, optionally as a `data:image/...;base64,` URL.
    pub image_base64: String,
    pub org_id: String,
    pub dept_id: String,
    #[serde(default = "default_knowledge_top_k")]
    pub top_k: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SimilarImageHit {
    pub image_path: String,
    pub style_id: Option<String>,
    pub style_name: Option<String>,
    pub similarity: f32,
}

// ── POST /internal/images/similar ────────────────────────────────────────────

/// Accepts either a multipart upload (`file` + org_id/dept_id/top_k) or a JSON
/// body with base64 image data. CLIP-encodes the image, searches Qdrant
/// filtered by org+dept, and enriches hits with the style name.
pub async fn find_similar_images(
    State(state): State<AppState>,
    request: Request,
) -> Result<Json<Value>, crate::error::AppError> {
    let content_type = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    // Gateway-injected identity (preferred over multipart/JSON fields).
    let (h_org, h_dept, _h_user) = header_identity(request.headers());

    let (image_bytes, org_id_raw, dept_id_raw, top_k) =
        if content_type.starts_with("multipart/form-data") {
            let mut multipart = Multipart::from_request(request, &state)
                .await
                .map_err(|e| crate::error::AppError::BadRequest(format!("Invalid multipart: {e}")))?;

            let mut file_bytes: Vec<u8> = Vec::new();
            let mut org = String::new();
            let mut dept = String::new();
            let mut k = 5usize;

            while let Some(field) = multipart.next_field().await.map_err(|e| {
                crate::error::AppError::BadRequest(format!("Multipart field: {e}"))
            })? {
                match field.name().unwrap_or("") {
                    "file" => {
                        file_bytes = field
                            .bytes()
                            .await
                            .map_err(|e| {
                                crate::error::AppError::BadRequest(format!("Read file: {e}"))
                            })?
                            .to_vec();
                        if file_bytes.len() > 10 * 1024 * 1024 {
                            return Err(crate::error::AppError::BadRequest(
                                "Image exceeds 10MB limit".into(),
                            ));
                        }
                    }
                    "org_id" => org = field.text().await.unwrap_or_default(),
                    "dept_id" => dept = field.text().await.unwrap_or_default(),
                    "top_k" => {
                        k = field
                            .text()
                            .await
                            .ok()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(5);
                    }
                    _ => {}
                }
            }
            (file_bytes, org, dept, k)
        } else {
            // JSON body — buffer then parse.
            let (parts, body) = request.into_parts();
            let bytes = axum::body::to_bytes(body, 10 * 1024 * 1024)
                .await
                .map_err(|e| crate::error::AppError::BadRequest(format!("Read body: {e}")))?;
            let _ = parts;
            let req: SimilarImagesJsonRequest = serde_json::from_slice(&bytes)?;
            let decoded = crate::images::clip::base64_decode(&req.image_base64).map_err(|e| {
                crate::error::AppError::BadRequest(format!("Invalid image_base64: {e}"))
            })?;
            (decoded, req.org_id, req.dept_id, req.top_k)
        };

    if image_bytes.is_empty() {
        return Err(crate::error::AppError::BadRequest("Image is empty".into()));
    }
    let org_id_raw = h_org.unwrap_or(org_id_raw);
    let dept_id_raw = h_dept.unwrap_or(dept_id_raw);
    parse_uuid("org_id", &org_id_raw)?;
    parse_uuid("dept_id", &dept_id_raw)?;
    let top_k = top_k.clamp(1, 50);

    let hits = state
        .image_search
        .search_similar(&image_bytes, &org_id_raw, &dept_id_raw, top_k)
        .await
        .map_err(|e| crate::error::AppError::LlmError(e.to_string()))?;

    // Enrich with style names in one query.
    let style_ids: Vec<Uuid> = hits
        .iter()
        .filter_map(|h| h.style_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()))
        .collect();
    let mut name_map = std::collections::HashMap::new();
    if !style_ids.is_empty() {
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, name FROM styles WHERE id = ANY($1)",
        )
        .bind(&style_ids)
        .fetch_all(state.session_store.pool())
        .await?;
        name_map.extend(rows);
    }

    let results: Vec<SimilarImageHit> = hits
        .into_iter()
        .map(|h| {
            let style_name = h
                .style_id
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
                .and_then(|u| name_map.get(&u).cloned());
            SimilarImageHit {
                image_path: h.image_path,
                style_id: h.style_id,
                style_name,
                similarity: h.score,
            }
        })
        .collect();

    Ok(Json(serde_json::json!({ "results": results })))
}

// ── POST /api/styles/:id/images ──────────────────────────────────────────────

/// Upload an image for a style; stores the file, inserts the DB row, and
/// spawns CLIP encoding + Qdrant upsert.
pub async fn upload_style_image(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(style_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<Value>, crate::error::AppError> {
    // Tenant scope comes from the gateway-injected identity when present.
    let (h_org, h_dept, h_user) = header_identity(&headers);

    // The style must exist AND belong to the caller's org/dept — otherwise a
    // user could attach images to another department's styles.
    let mut q = sqlx::QueryBuilder::new("SELECT org_id, dept_id FROM styles WHERE id = ");
    q.push_bind(style_id);
    if let Some(o) = &h_org {
        if let Ok(oid) = Uuid::parse_str(o) {
            q.push(" AND org_id = ").push_bind(oid);
        }
    }
    if let Some(d) = &h_dept {
        if let Ok(did) = Uuid::parse_str(d) {
            q.push(" AND dept_id = ").push_bind(did);
        }
    }
    let style_row: (Uuid, Uuid) = q
        .build_query_as()
        .fetch_optional(state.session_store.pool())
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound("Style not found".into()))?;
    let (org_id, dept_id) = style_row;

    let mut file_bytes: Vec<u8> = Vec::new();
    let mut original_filename = String::new();
    let mut uploaded_by: Option<Uuid> =
        h_user.as_deref().map(Uuid::parse_str).and_then(Result::ok);

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| crate::error::AppError::BadRequest(format!("Invalid multipart: {e}")))?
    {
        match field.name().unwrap_or("") {
            "file" => {
                original_filename = field.file_name().unwrap_or("uploaded").to_string();
                file_bytes = field
                    .bytes()
                    .await
                    .map_err(|e| crate::error::AppError::BadRequest(format!("Read file: {e}")))?
                    .to_vec();
                if file_bytes.len() > 10 * 1024 * 1024 {
                    return Err(crate::error::AppError::BadRequest(
                        "Image exceeds 10MB limit".into(),
                    ));
                }
            }
            "uploaded_by" => {
                // Only honour a form-supplied uploader when no gateway header.
                if h_user.is_none() {
                    let raw = field.text().await.unwrap_or_default();
                    uploaded_by = Uuid::parse_str(&raw).ok();
                }
            }
            _ => {}
        }
    }

    if file_bytes.is_empty() {
        return Err(crate::error::AppError::BadRequest("File is empty".into()));
    }

    let file_type = std::path::Path::new(&original_filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .filter(|e| matches!(e.as_str(), "jpg" | "jpeg" | "png" | "webp"))
        .ok_or_else(|| {
            crate::error::AppError::BadRequest(
                "Unsupported image type — allowed: jpg, jpeg, png, webp".into(),
            )
        })?;

    let image_id = Uuid::new_v4();
    let image_path = crate::images::indexer::ImageIndexer::store_image(
        &org_id.to_string(),
        image_id,
        &file_type,
        &file_bytes,
    )
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("Store image: {e}")))?;

    let record: StyleImageRecord = sqlx::query_as(
        r#"INSERT INTO style_images
               (id, org_id, dept_id, style_id, image_path, file_type, uploaded_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING id, org_id, dept_id, style_id, image_path, file_type,
                     uploaded_by, created_at"#,
    )
    .bind(image_id)
    .bind(org_id)
    .bind(dept_id)
    .bind(style_id)
    .bind(&image_path)
    .bind(&file_type)
    .bind(uploaded_by)
    .fetch_one(state.session_store.pool())
    .await?;

    // Background CLIP encode + Qdrant upsert (best-effort).
    let image_search = state.image_search.clone();
    let bytes_clone = file_bytes.clone();
    let org_str = org_id.to_string();
    let dept_str = dept_id.to_string();
    let path_clone = image_path.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::images::indexer::ImageIndexer::index_image(
            &image_search.clip(),
            image_search.store(),
            image_id,
            &path_clone,
            Some(style_id),
            &org_str,
            &dept_str,
            &bytes_clone,
        )
        .await
        {
            tracing::error!(%image_id, "Image indexing failed: {e}");
        }
    });

    Ok(Json(serde_json::to_value(&record)?))
}

// ── GET /api/styles/:id/images ───────────────────────────────────────────────

/// List images attached to a style, newest first.
///
/// Tenant filtered: `org_id` + `dept_id` query params are required (core is
/// an internal trusted hop; the gateway injects them from JWT claims), so
/// images cannot be enumerated across tenants by style UUID.
pub async fn list_style_images(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(style_id): Path<Uuid>,
    Query(q): Query<StyleImagesListQuery>,
) -> Result<Json<Value>, crate::error::AppError> {
    // Gateway-injected identity wins; query params only for internal calls.
    let (h_org, h_dept, _) = header_identity(&headers);
    let org = h_org
        .as_deref()
        .map(Uuid::parse_str)
        .and_then(Result::ok)
        .or(q.org_id)
        .ok_or_else(|| crate::error::AppError::BadRequest("org_id required".into()))?;
    let dept = h_dept
        .as_deref()
        .map(Uuid::parse_str)
        .and_then(Result::ok)
        .or(q.dept_id)
        .ok_or_else(|| crate::error::AppError::BadRequest("dept_id required".into()))?;

    let images: Vec<StyleImageRecord> = sqlx::query_as(
        r#"SELECT id, org_id, dept_id, style_id, image_path, file_type,
                  uploaded_by, created_at
           FROM style_images
           WHERE style_id = $1 AND org_id = $2 AND dept_id = $3
           ORDER BY created_at DESC"#,
    )
    .bind(style_id)
    .bind(org)
    .bind(dept)
    .fetch_all(state.session_store.pool())
    .await?;

    Ok(Json(serde_json::json!({ "images": images, "total": images.len() })))
}

#[derive(Debug, Deserialize)]
pub struct StyleImagesListQuery {
    #[serde(default)]
    org_id: Option<Uuid>,
    #[serde(default)]
    dept_id: Option<Uuid>,
}
