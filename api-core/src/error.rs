//! Unified error types for api-core.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("LLM error: {0}")]
    LlmError(String),

    #[error("Database error: {0}")]
    DbError(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Skill execution error: {0}")]
    SkillError(String),

    #[error("Casbin authorization error: {0}")]
    CasbinError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg.clone()),
            AppError::LlmError(msg) => (StatusCode::BAD_GATEWAY, "LLM_ERROR", msg.clone()),
            AppError::DbError(e) => {
                tracing::error!("Database error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", "Database error".to_string())
            }
            AppError::RedisError(e) => {
                tracing::error!("Redis error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "REDIS_ERROR", "Cache error".to_string())
            }
            AppError::HttpError(e) => {
                tracing::error!("HTTP error: {e}");
                (StatusCode::BAD_GATEWAY, "HTTP_ERROR", "Upstream error".to_string())
            }
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg.clone())
            }
            AppError::SkillError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "SKILL_ERROR", msg.clone()),
            AppError::CasbinError(msg) => (StatusCode::FORBIDDEN, "CASBIN_ERROR", msg.clone()),
            AppError::SerializationError(e) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "SERIALIZATION_ERROR", e.to_string())
            }
        };

        let body = Json(json!({
            "error": {
                "code": code,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
