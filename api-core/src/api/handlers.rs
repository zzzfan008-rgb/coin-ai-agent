//! HTTP handlers for api-core endpoints (aligned with api/core.yml).

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::agent::{AgentConfig, AgentEngine};
use crate::llm::messages::ChatMessage;
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
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserContext {
    pub user_id: String,
    pub org_id: String,
    pub dept_id: String,
    pub role: String,
    #[serde(default)]
    pub ip_address: Option<String>,
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
    let llm_ok = state.llm_client.health_check().await;

    Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "services": {
            "database": if db_ok { "ok" } else { "down" },
            "llm": if llm_ok { "ok" } else { "down" },
        }
    }))
}

// ── Chat ───────────────────────────────────────────────────────────────────────

pub async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<CoreChatRequest>,
) -> Result<Response, crate::error::AppError> {
    if req.stream {
        return Ok(stream_chat(state, req).await);
    }

    let engine = AgentEngine::new(
        AgentConfig::default(),
        state.llm_client.clone(),
        state.session_store.clone(),
        None,
    );

    let collections = req
        .extra_body
        .as_ref()
        .and_then(|b| b.knowledge_collections.clone());
    let reply = engine
        .run_turn(&req.messages, None, &req.user_context, collections.as_deref())
        .await?;

    let response = CoreChatResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: Utc::now().timestamp(),
        model: active_model(&state),
        choices: vec![serde_json::json!({
            "index": 0,
            "message": {"role": "assistant", "content": reply},
            "finish_reason": "stop"
        })],
        usage: serde_json::json!({"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}),
    };

    Ok(Json(response).into_response())
}

pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<CoreChatRequest>,
) -> Result<Response, crate::error::AppError> {
    Ok(stream_chat(state, req).await)
}

/// Build the SSE response. LLM chunks are pushed through a broadcast channel
/// so the HTTP connection consumes already-framed `data: ...\n\n` bytes.
async fn stream_chat(state: Arc<AppServices>, req: CoreChatRequest) -> Response {
    let (tx, rx) = tokio::sync::broadcast::channel::<Bytes>(128);

    let llm = state.llm_client.clone();
    let messages = req.messages;
    let temperature = req.temperature;
    let max_tokens = req.max_tokens;

    tokio::spawn(async move {
        let stream = match llm
            .chat_streaming(&messages, temperature, max_tokens, None)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                let payload = serde_json::json!({"error": {"code": "LLM_ERROR", "message": e.to_string()}});
                let _ = tx.send(Bytes::from(format!("data: {payload}\n\n")));
                return;
            }
        };

        futures::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    let data = serde_json::to_string(&chunk).unwrap_or_default();
                    if tx.send(Bytes::from(format!("data: {data}\n\n"))).is_err() {
                        break; // client gone
                    }
                }
                Err(e) => {
                    let payload = serde_json::json!({"error": {"code": "LLM_ERROR", "message": e.to_string()}});
                    let _ = tx.send(Bytes::from(format!("data: {payload}\n\n")));
                    break;
                }
            }
        }
        let _ = tx.send(Bytes::from_static(b"data: [DONE]\n\n"));
    });

    let body_stream = BroadcastStream::new(rx)
        .filter_map(|r| async { r.ok().map(Ok::<Bytes, BroadcastStreamRecvError>) });

    let mut response = axum::body::Body::from_stream(body_stream).into_response();
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert("Content-Type", "text/event-stream".parse().unwrap());
    headers.insert("Cache-Control", "no-cache".parse().unwrap());
    headers.insert("Connection", "keep-alive".parse().unwrap());
    headers.insert("X-Accel-Buffering", "no".parse().unwrap());
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

pub async fn list_skills() -> Json<Value> {
    // Phase 1B: delegate to SkillRegistry loaded from skills/
    Json(serde_json::json!({ "skills": [] }))
}

pub async fn execute_skill(
    Json(req): Json<SkillExecuteRequest>,
) -> Result<Json<SkillExecuteResponse>, crate::error::AppError> {
    let started = std::time::Instant::now();
    let result = serde_json::json!({
        "status": "stub",
        "skillId": req.skill_id,
        "toolName": req.tool_name,
        "message": "Skill execution stub — Phase 1B"
    });
    Ok(Json(SkillExecuteResponse {
        skill_id: req.skill_id,
        tool_name: req.tool_name,
        result,
        tokens_used: 0,
        duration_ms: started.elapsed().as_millis() as i64,
    }))
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn parse_uuid(field: &str, raw: &str) -> Result<Uuid, crate::error::AppError> {
    Uuid::parse_str(raw)
        .map_err(|_| crate::error::AppError::BadRequest(format!("Invalid {field}")))
}
