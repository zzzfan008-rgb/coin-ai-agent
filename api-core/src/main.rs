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
mod rag;
mod images;
mod session;
mod middleware;
mod rbac;
mod intent;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::AppConfig;
use crate::llm::LlmClient;
use crate::rag::{RagConfig, RagRetriever};
use crate::images::ImageSearchService;
use crate::rbac::RbacService;
use crate::session::SessionStore;

pub type AppState = Arc<AppServices>;

pub struct AppServices {
    pub config: AppConfig,
    pub session_store: Arc<SessionStore>,
    pub llm_client: Arc<LlmClient>,
    pub rbac: Arc<RbacService>,
    pub rag_retriever: Arc<RagRetriever>,
    pub intent_router: Arc<crate::intent::IntentRouter>,
    pub image_search: Arc<ImageSearchService>,
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

    // Publish pool for tool-level audit writes (rbac enforce_tool).
    crate::rbac::set_audit_pool(session_store.pool().clone());

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

    // ── RBAC (Casbin) ─────────────────────────────────────────────────────────
    let rbac = Arc::new(RbacService::new().await?);
    // Publish the process-global used by detached agent-loop functions.
    // RbacService is backed by an Arc, so this clone is cheap.
    crate::rbac::RBAC_SERVICE
        .set((*rbac).clone())
        .map_err(|_| anyhow::anyhow!("RBAC_SERVICE already initialised"))?;

    // ── Fashion DB store (global for detached executors) ─────────────────────
    let fashion_store = crate::skill_engine::FashionStore::new(session_store.pool().clone());
    crate::skill_engine::FASHION_STORE
        .set(fashion_store)
        .map_err(|_| anyhow::anyhow!("FASHION_STORE already initialised"))?;

    // ── RAG (embedding + Qdrant) ────────────────────────────────────────────
    let rag_config = RagConfig::from_app_config(&config);
    let rag_retriever = Arc::new(RagRetriever::new(rag_config));

    // Ensure the Qdrant collection exists; log but don't block startup.
    if let Err(e) = rag_retriever.ensure_collection().await {
        tracing::warn!("Qdrant collection init failed: {e} — indexing will retry");
    }

    // ── MCP clients ─────────────────────────────────────────────────────────
    // Auto-register any MCP servers passed via MCP_SERVERS env var.
    // Format: id1=http://host:port,id2=http://host:port
    // The local mock server is registered automatically when running with
    // APP_ENV=development and scripts/mock_mcp_server.py is reachable.
    if let Ok(spec) = std::env::var("MCP_SERVERS") {
        for entry in spec.split(',') {
            if let Some((id, url)) = entry.split_once('=') {
                let cfg = crate::mcp::McpServerConfig {
                    id: id.trim().to_string(),
                    name: id.trim().to_string(),
                    base_url: url.trim().trim_end_matches('/').to_string(),
                    auth_token: None,
                };
                crate::mcp::MCP_MANAGER.register(cfg).await;
            }
        }
    }
    // In development, also probe the local mock server and register it
    // if it's listening (failure is silent — mock server may not be running).
    if config.app_env == "development" {
        let mock_url = "http://localhost:9101";
        let probe = reqwest::Client::new()
            .get(format!("{mock_url}/health"))
            .timeout(Duration::from_millis(500))
            .send()
            .await;
        if probe.is_ok_and(|r| r.status().is_success()) {
            crate::mcp::MCP_MANAGER
                .register(crate::mcp::McpServerConfig {
                    id: "local-mock".into(),
                    name: "Local Mock MCP".into(),
                    base_url: mock_url.into(),
                    auth_token: None,
                })
                .await;
            tracing::info!("Registered local mock MCP server at {mock_url}");
        }
    }

    // ── CLIP image search (T-019) ───────────────────────────────────────────
    let image_search = Arc::new(ImageSearchService::from_env());
    if image_search.configured() {
        tracing::info!("CLIP client ready (model={})", image_search.model_name());
    } else {
        tracing::warn!("CLIP_API_ENDPOINT not set — image upload/search will fail until configured");
    }
    if let Err(e) = image_search.ensure_collection().await {
        tracing::warn!("Qdrant style_images collection init failed: {e}");
    }

    // ── Intent router (T-014) ──────────────────────────────────────────────
    let rules_path = project_root.join("api-core/src/intent/rules.yaml");
    let intent_router = Arc::new(crate::intent::IntentRouter::load_or_embedded(&rules_path)?);

    let services = Arc::new(AppServices {
        config,
        session_store,
        llm_client,
        rbac,
        rag_retriever,
        intent_router,
        image_search,
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
