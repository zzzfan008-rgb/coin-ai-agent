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
