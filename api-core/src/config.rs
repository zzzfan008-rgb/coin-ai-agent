//! Configuration loading from .env file and environment variables.

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub app_env: String,
    pub log_level: String,

    // Database
    pub database_url: String,
    pub db_max_connections: u32,

    // Redis
    pub redis_url: String,

    // Qdrant
    pub qdrant_url: String,

    // Core service
    pub core_port: u16,

    // LLM
    pub llm_provider: String, // "minimax" | "deepseek" | "qwen"
    pub llm_timeout_secs: u64,

    // MiniMax
    pub minimax_api_key: String,
    pub minimax_model: String,
    pub minimax_base_url: String,

    // DeepSeek
    pub deepseek_api_key: String,
    pub deepseek_model: String,
    pub deepseek_base_url: String,

    // Qwen / Alibaba DashScope (OpenAI-compatible)
    pub qwen_api_key: String,
    pub qwen_model: String,
    pub qwen_base_url: String,

    // MiniIO
    pub minio_endpoint: String,
    pub minio_bucket: String,
    pub minio_access_key: String,
    pub minio_secret_key: String,

    // SSE timeouts (T-022)
    /// Max seconds without data before sending a keepalive comment.
    /// Keepalive interval = min(45s, SSE_IDLE_TIMEOUT / 2).
    pub sse_idle_timeout_secs: u64,
    /// Max seconds for a complete streaming turn.
    pub sse_write_timeout_secs: u64,
}

impl AppConfig {
    /// Load config. Loads `.env` from CWD if present; real env vars take priority.
    pub fn load() -> anyhow::Result<Self> {
        // Ignore error (file may not exist)
        let _ = dotenvy::dotenv();

        Ok(Self {
            app_env: env_var("APP_ENV", "development"),
            log_level: env_var("LOG_LEVEL", "info"),

            database_url: env_var(
                "DATABASE_URL",
                "postgres://fashion_ai:***@localhost:5433/fashion_ai?sslmode=disable",
            ),
            db_max_connections: env_parse("DB_MAX_CONNECTIONS", 10),

            redis_url: env_var("REDIS_URL", "redis://localhost:6379/0"),

            qdrant_url: env_var("QDRANT_URL", "http://localhost:6333"),

            core_port: env_parse("CORE_PORT", 8081),

            llm_provider: env_var("LLM_PROVIDER", "minimax"),
            llm_timeout_secs: env_parse("LLM_TIMEOUT_SECS", 60),

            minimax_api_key: env_var("MINIMAX_API_KEY", ""),
            minimax_model: env_var("MINIMAX_MODEL", "MiniMax-Text-01"),
            minimax_base_url: env_var("MINIMAX_BASE_URL", "https://api.minimaxi.chat/v1"),

            deepseek_api_key: env_var("DEEPSEEK_API_KEY", ""),
            deepseek_model: env_var("DEEPSEEK_MODEL", "deepseek-chat"),
            deepseek_base_url: env_var("DEEPSEEK_BASE_URL", "https://api.deepseek.com/v1"),

            qwen_api_key: first_var(&["DASHSCOPE_API_KEY", "QWEN_API_KEY"], ""),
            qwen_model: env_var("QWEN_MODEL", "qwen-plus"),
            qwen_base_url: env_var(
                "QWEN_BASE_URL",
                "https://maas.qianwenaiapi.com/compatible-mode/v1",
            ),

            minio_endpoint: env_var("MINIO_ENDPOINT", "http://localhost:9000"),
            minio_bucket: env_var("MINIO_BUCKET", "fashion-ai"),
            minio_access_key: env_var("MINIO_ROOT_USER", "minioadmin"),
            minio_secret_key: env_var("MINIO_ROOT_PASSWORD", "minioadmin"),

            sse_idle_timeout_secs: env_parse("SSE_IDLE_TIMEOUT", 120u64),
            sse_write_timeout_secs: env_parse("SSE_WRITE_TIMEOUT", 300u64),
        })
    }
}

fn env_var(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Return the first present, non-empty variable among `keys`.
fn first_var(keys: &[&str], default: &str) -> String {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    default.to_string()
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
