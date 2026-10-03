//! Fashion AI Platform — Core Inference Service library (codex-rs fork)
//!
//! Wires together:
//!   - Axum HTTP server
//!   - PostgreSQL session store
//!   - LLM client (MiniMax / DeepSeek)
//!   - Agent loop, tool engine, skill loader, MCP client
//!   - PreToolCall permission hook
//!
//! `src/main.rs` is a thin binary wrapper calling [`run`]; integration tests
//! in `tests/` exercise the real routes through this library target.

// Reserved scaffold modules (skill runtime, MCP manager) are exercised in
// Phase 1B; allow their currently-unreachable items.
#![allow(dead_code)]

pub mod agent;
pub mod api;
pub mod config;
pub mod error;
pub mod images;
pub mod intent;
pub mod llm;
pub mod mcp;
pub mod middleware;
pub mod rag;
pub mod rbac;
pub mod session;
pub mod skill_engine;
pub mod tool;

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
use crate::images::ImageSearchService;
use crate::llm::LlmClient;
use crate::rag::{RagConfig, RagRetriever};
use crate::rbac::RbacService;
use crate::session::SessionStore;

pub type AppState = Arc<AppServices>;

pub struct AppServices {
    pub config: AppConfig,
    pub session_store: Arc<SessionStore>,
    pub llm_client: Arc<LlmClient>,
    pub rag_retriever: Arc<RagRetriever>,
    pub intent_router: Arc<crate::intent::IntentRouter>,
    pub image_search: Arc<ImageSearchService>,
}

/// 定位 skills 目录，按优先级返回首个存在的目录：
/// SKILLS_DIR → 可执行文件 ../skills、./skills → 工作目录 ./skills、../skills。
fn resolve_skills_dir() -> std::path::PathBuf {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(dir) = std::env::var("SKILLS_DIR") {
        candidates.push(dir.into());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("..").join("skills"));
            candidates.push(exe_dir.join("skills"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("skills"));
        candidates.push(cwd.join("..").join("skills"));
    }
    for c in &candidates {
        if c.is_dir() {
            return c.clone();
        }
    }
    // 全部不存在：回退到 ./skills，交由 load_from_dir 产生明确错误（fail-fast）。
    std::path::PathBuf::from("skills")
}

/// Boot the full service: load config, build every dependency, bind and serve.
/// Called by the binary entry point in `src/main.rs`.
pub async fn run() -> Result<()> {
    // ── Logging ──────────────────────────────────────────────────────────────
    let config = AppConfig::load()?;
    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
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

    // ── MCP Server registration ──────────────────────────────────────────
    // Register active MCP servers from DB into the global manager.
    // Failures are logged but non-fatal — MCP is an optional component.
    //
    // P1-2: registration is scoped to ONE org — the MCP manager is
    // process-global, so servers of other orgs must never be registered.
    // Scope = MCP_ORG_ID when set; otherwise auto-select if exactly one org
    // owns active servers; multiple orgs without MCP_ORG_ID → fail closed
    // (register nothing) rather than leak servers across orgs.
    {
        let pool = session_store.pool();

        let org_scope: Option<uuid::Uuid> = match &config.mcp_org_id {
            Some(raw) => match uuid::Uuid::parse_str(raw) {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::error!(error = %e, raw = %raw, "MCP_ORG_ID is not a valid UUID — MCP registration skipped (fail-closed)");
                    None
                }
            },
            None => {
                let orgs: Vec<uuid::Uuid> = sqlx::query_scalar(
                    "SELECT DISTINCT org_id FROM mcp_servers WHERE is_active = true",
                )
                .fetch_all(pool)
                .await
                .unwrap_or_default();
                match orgs.len() {
                    0 => None,
                    1 => Some(orgs[0]),
                    n => {
                        tracing::error!(
                            orgs = n,
                            "multiple orgs own active MCP servers and MCP_ORG_ID is unset — MCP registration skipped (fail-closed)"
                        );
                        None
                    }
                }
            }
        };

        // Distinguish "no active servers" from "scope resolution failed":
        // an explicit/derived scope of None with active servers present is a
        // hard skip; when the tables are simply empty the SELECT is a no-op.
        let scope_failed = match &config.mcp_org_id {
            Some(_) => org_scope.is_none(),
            None => {
                let active: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM mcp_servers WHERE is_active = true")
                        .fetch_one(pool)
                        .await
                        .unwrap_or(0);
                active > 0 && org_scope.is_none()
            }
        };
        if scope_failed {
            tracing::warn!("MCP server registration skipped this boot (org scope unresolved)");
        }

        let rows: Vec<(uuid::Uuid, String, Option<String>, Option<String>)> = if scope_failed {
            vec![]
        } else if let Some(org) = org_scope {
            sqlx::query_as(
                "SELECT id, name, endpoint, auth_token FROM mcp_servers WHERE is_active = true AND org_id = $1",
            )
            .bind(org)
            .fetch_all(pool)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "MCP server query failed");
                vec![]
            })
        } else {
            sqlx::query_as(
                "SELECT id, name, endpoint, auth_token FROM mcp_servers WHERE is_active = true",
            )
            .fetch_all(pool)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "MCP server query failed");
                vec![]
            })
        };

        match Ok::<_, sqlx::Error>(rows) {
            Ok(rows) => {
                for (id, name, endpoint, auth_token) in rows {
                    let base_url = endpoint.unwrap_or_default();
                    if base_url.is_empty() {
                        tracing::warn!(%name, "MCP server has no endpoint, skipping");
                        continue;
                    }
                    let cfg = crate::mcp::McpServerConfig {
                        id: id.to_string(),
                        name,
                        base_url,
                        auth_token,
                    };
                    crate::mcp::MCP_MANAGER.register(cfg).await;
                }
                let count = crate::mcp::MCP_MANAGER.server_ids().await.len();
                tracing::info!("MCP servers registered: {count}");
            }
            Err(e) => {
                tracing::warn!("Failed to load MCP server configs: {e} — MCP disabled");
            }
        }
    }

    // ── SKILL Engine ────────────────────────────────────────────────────────
    // 不再只依赖 cwd/skills：按 SKILLS_DIR → 可执行文件相对 → 工作目录相对
    // 的顺序定位，避免从其它目录启动时找不到 skill。找不到时 load 会 fail-fast。
    let skills_dir = resolve_skills_dir();
    tracing::info!("Loading skills from {:?}", skills_dir);

    let engine = crate::skill_engine::SkillEngine::load_from_dir(&skills_dir)
        .map_err(|e| anyhow::anyhow!("Failed to load skill engine: {e}"))?;

    let skill_ids: Vec<_> = engine.loader.skill_ids();
    tracing::info!(
        "Skill engine loaded: {} skills ({:?})",
        skill_ids.len(),
        skill_ids
    );

    {
        let mut global = crate::skill_engine::SKILL_ENGINE
            .write()
            .expect("SKILL_ENGINE poisoned");
        *global = Some(engine);
    }

    // ── RBAC (Casbin) ───────────────────────────────────────────────────────
    let rbac = Arc::new(RbacService::new().await?);
    // Publish the process-global used by detached agent-loop functions.
    // RbacService is backed by an Arc, so this clone is cheap.
    crate::rbac::RBAC_SERVICE
        .set((*rbac).clone())
        .map_err(|_| anyhow::anyhow!("RBAC_SERVICE already initialised"))?;

    // P1-2: enterprise MCP grants live in mcp_permissions (data-driven),
    // not the embedded policies.csv. Load them into the enforcer at boot;
    // failure only loses DB-sourced grants — embedded seed policies remain.
    match rbac.load_mcp_policies(session_store.pool()).await {
        Ok(n) => tracing::info!("Casbin MCP grants loaded from DB: {n}"),
        Err(e) => tracing::warn!(
            error = %e,
            "failed to load MCP permissions from DB — only embedded seed policies active"
        ),
    }

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
        tracing::warn!(
            "CLIP_API_ENDPOINT not set — image upload/search will fail until configured"
        );
    }
    if let Err(e) = image_search.ensure_collection().await {
        tracing::warn!("Qdrant style_images collection init failed: {e}");
    }

    // ── Intent router (T-014) ──────────────────────────────────────────────
    // 路径可由 INTENT_RULES_PATH 覆盖；默认相对仓库根，定位不到时 load_or_embedded
    // 会回退到编译期内置规则（见 intent 模块）。
    let rules_path = std::path::PathBuf::from(
        std::env::var("INTENT_RULES_PATH")
            .unwrap_or_else(|_| "api-core/src/intent/rules.yaml".to_string()),
    );
    let intent_router = Arc::new(crate::intent::IntentRouter::load_or_embedded(&rules_path)?);

    let services = Arc::new(AppServices {
        config,
        session_store,
        llm_client,
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
