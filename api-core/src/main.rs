//! Fashion AI Platform — Core Inference Service (codex-rs fork)
//!
//! Entry point. Wires together:
//!   - Axum HTTP server
//!   - PostgreSQL session store
//!   - LLM client (MiniMax / DeepSeek)
//!   - Agent loop, tool engine, skill loader, MCP client
//!   - PreToolCall permission hook

// Reserved scaffold modules (skill runtime, MCP manager) are exercised in
// Phase 1B; allow their currently-unreachable items.
#![allow(dead_code)]

mod config;
mod error;
mod api;
mod llm;
mod agent;
mod tool;
mod skill;
mod skill_engine;
mod mcp;
mod session;
mod middleware;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::AppConfig;
use crate::llm::LlmClient;
use crate::session::SessionStore;

pub type AppState = Arc<AppServices>;

pub struct AppServices {
    pub config: AppConfig,
    pub session_store: Arc<SessionStore>,
    pub llm_client: Arc<LlmClient>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // ── Logging ──────────────────────────────────────────────────────────────
    let config = AppConfig::load()?;
    let filter = EnvFilter::try_new(&config.log_level)
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();

    tracing::info!("Starting Fashion AI Core Service (env={})", config.app_env);

    // ── Service dependencies ────────────────────────────────────────────────
    let session_store = Arc::new(SessionStore::new(&config.database_url).await?);
    if let Err(e) = session_store.migrate().await {
        tracing::warn!("Database connectivity check failed: {e}");
    }

    let llm_client = Arc::new(LlmClient::new(&config)?);
    tracing::info!("LLM client ready (provider={})", config.llm_provider);

    // Probe the default LLM on startup; if unreachable, log but don't block
    // (the provider may become available later).
    let llm_health = llm_client.health_check().await;
    if llm_health {
        tracing::info!("LLM health check: OK ({})", config.llm_provider);
    } else {
        tracing::warn!(
            "LLM health check: FAILED ({}) — requests may fail until the provider is reachable",
            config.llm_provider
        );
    }

    // ── SKILL Engine ────────────────────────────────────────────────────────────
    let project_root = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let skills_dir = project_root.join("skills");
    tracing::info!("Loading skills from {:?}", skills_dir);

    let engine = crate::skill_engine::SkillEngine::load_from_dir(&skills_dir)
        .map_err(|e| anyhow::anyhow!("Failed to load skill engine: {e}"))?;

    let skill_ids: Vec<_> = engine.loader.skill_ids();
    tracing::info!("Skill engine loaded: {} skills ({:?})", skill_ids.len(), skill_ids);

    {
        let mut global = crate::skill_engine::SKILL_ENGINE
            .write()
            .expect("SKILL_ENGINE poisoned");
        *global = Some(engine);
    }

    let services = Arc::new(AppServices {
        config,
        session_store,
        llm_client,
    });

    // ── Router ──────────────────────────────────────────────────────────────
    let port = services.config.core_port;
    let app = Router::new()
        .merge(api::routes())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(services);

    // ── Listen ──────────────────────────────────────────────────────────────
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C handler");
    tracing::info!("Shutdown signal received");
}
