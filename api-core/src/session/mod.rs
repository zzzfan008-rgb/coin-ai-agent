//! Session store — PostgreSQL persistence for sessions and messages.
//!
//! The physical schema is owned by T-003 (`migrations/001_init_schema.sql`,
//! applied via `sqlx migrate run`). Queries here are written against that
//! canonical schema; this module does not create or alter tables.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use uuid::Uuid;

use crate::error::{AppError, Result};

pub struct SessionStore {
    pool: PgPool,
}

impl SessionStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Verify connectivity. Schema provisioning is T-003's job
    /// (`migrations/001_init_schema.sql`).
    pub async fn migrate(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    // ── Sessions ───────────────────────────────────────────────────────────────

    pub async fn create_session(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        user_id: Uuid,
        title: Option<&str>,
        model: Option<&str>,
    ) -> Result<Session> {
        let row = sqlx::query(
            r#"INSERT INTO sessions (org_id, dept_id, user_id, title, model)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id, org_id, dept_id, user_id, title, model,
                         is_archived, created_at, updated_at"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(user_id)
        .bind(title)
        .bind(model)
        .fetch_one(&self.pool)
        .await?;

        Ok(Session::from_row(&row))
    }

    pub async fn get_session(&self, session_id: Uuid) -> Result<Session> {
        let row = sqlx::query(
            r#"SELECT id, org_id, dept_id, user_id, title, model,
                      is_archived, created_at, updated_at
               FROM sessions WHERE id = $1"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session {session_id} not found")))?;

        Ok(Session::from_row(&row))
    }

    pub async fn touch_session(&self, session_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"UPDATE sessions
               SET updated_at = NOW(), last_message_at = NOW()
               WHERE id = $1"#,
        )
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── Messages ───────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub async fn save_message(
        &self,
        session_id: Uuid,
        role: &str,
        content: &str,
        model: Option<&str>,
        finish_reason: Option<&str>,
        token_count: Option<i32>,
        input_tokens: Option<i32>,
        output_tokens: Option<i32>,
        metadata: Option<serde_json::Value>,
    ) -> Result<Message> {
        let row = sqlx::query(
            r#"INSERT INTO messages
                   (session_id, role, content, model, finish_reason,
                    token_count, input_tokens, output_tokens, metadata)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               RETURNING id, session_id, role, content, model, finish_reason,
                         token_count, input_tokens, output_tokens, metadata, created_at"#,
        )
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(model)
        .bind(finish_reason)
        .bind(token_count)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(metadata.unwrap_or_else(|| serde_json::json!({})))
        .fetch_one(&self.pool)
        .await?;

        Ok(Message::from_row(&row))
    }

    pub async fn get_messages(
        &self,
        session_id: Uuid,
        limit: usize,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>> {
        let limit = limit as i64;

        let rows = if let Some(before_dt) = before {
            sqlx::query(
                r#"SELECT id, session_id, role, content, model, finish_reason,
                          token_count, input_tokens, output_tokens, metadata, created_at
                   FROM messages
                   WHERE session_id = $1 AND created_at < $2
                   ORDER BY created_at DESC
                   LIMIT $3"#,
            )
            .bind(session_id)
            .bind(before_dt)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"SELECT id, session_id, role, content, model, finish_reason,
                          token_count, input_tokens, output_tokens, metadata, created_at
                   FROM messages
                   WHERE session_id = $1
                   ORDER BY created_at DESC
                   LIMIT $2"#,
            )
            .bind(session_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        let mut messages: Vec<Message> = rows.iter().map(Message::from_row).collect();
        messages.reverse();
        Ok(messages)
    }

    pub async fn health_check(&self) -> Result<bool> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(true)
    }
}

// ── Domain types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: Uuid,
    pub org_id: Uuid,
    pub dept_id: Uuid,
    pub user_id: Uuid,
    pub title: Option<String>,
    pub model: Option<String>,
    pub is_archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Session {
    fn from_row(row: &sqlx::postgres::PgRow) -> Self {
        Self {
            id: row.get("id"),
            org_id: row.get("org_id"),
            dept_id: row.get("dept_id"),
            user_id: row.get("user_id"),
            title: row.get("title"),
            model: row.get("model"),
            is_archived: row.get("is_archived"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,
    pub content: String,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
    pub token_count: Option<i32>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl Message {
    fn from_row(row: &sqlx::postgres::PgRow) -> Self {
        Self {
            id: row.get("id"),
            session_id: row.get("session_id"),
            role: row.get("role"),
            content: row.get("content"),
            model: row.get("model"),
            finish_reason: row.get("finish_reason"),
            token_count: row.get("token_count"),
            input_tokens: row.get("input_tokens"),
            output_tokens: row.get("output_tokens"),
            metadata: row.get("metadata"),
            created_at: row.get("created_at"),
        }
    }
}
