//! Shared helpers for api-core route regression tests (P0-2 / T-022).
//!
//! Every dependency is wired to unreachable local endpoints — no test
//! touches a real PostgreSQL, LLM provider or Qdrant instance. The router
//! under test is the production `api::routes()` served over real HTTP.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use api_core::config::AppConfig;
use api_core::images::ImageSearchService;
use api_core::intent::IntentRouter;
use api_core::llm::LlmClient;
use api_core::rag::RagRetriever;
use api_core::rbac::RbacService;
use api_core::session::SessionStore;
use api_core::{api, AppServices, AppState};

/// AppConfig wired to unreachable local endpoints.
/// `minimax_api_key` must be non-empty: `LlmClient::new` fail-fasts when
/// the active provider has no key.
pub fn test_config() -> AppConfig {
    AppConfig {
        app_env: "test".into(),
        log_level: "info".into(),
        // Port 15432 — a deliberately dead port. (5432 is unsafe as a
        // "no DB" assumption: both this machine and GitHub runners can
        // have a live PostgreSQL there.) A refused connection is retried
        // by sqlx's acquire loop, but SessionStore::lazy caps
        // acquire_timeout at 500ms, so /health still answers fast.
        database_url: "postgres://test@127.0.0.1:15432/no_db".into(),
        db_max_connections: 1,
        redis_url: String::new(),
        // Same reasoning as the DB port above — 6333 (Qdrant default) may
        // host a real instance on dev machines or CI runners, so use a
        // deliberately dead port: any future RAG/image assertion must not
        // silently bind to whatever happens to be listening.
        qdrant_url: "http://127.0.0.1:16333".into(),
        core_port: 0,
        llm_provider: "minimax".into(),
        llm_timeout_secs: 5,
        minimax_api_key: "test-key".into(),
        minimax_model: "test-model".into(),
        minimax_base_url: "http://127.0.0.1:9".into(),
        deepseek_api_key: String::new(),
        deepseek_model: String::new(),
        deepseek_base_url: String::new(),
        qwen_api_key: String::new(),
        qwen_model: String::new(),
        qwen_base_url: String::new(),
        minio_endpoint: String::new(),
        minio_bucket: String::new(),
        minio_access_key: String::new(),
        minio_secret_key: String::new(),
        mcp_org_id: None,
        sse_idle_timeout_secs: 30,
        sse_write_timeout_secs: 60,
    }
}

/// Build the production `AppServices` graph without any live backend.
pub async fn test_state() -> AppState {
    let config = test_config();
    let session_store =
        Arc::new(SessionStore::lazy(&config.database_url).expect("lazy session pool"));
    let llm_client = Arc::new(LlmClient::new(&config).expect("llm client"));
    let rbac = Arc::new(RbacService::new().await.expect("rbac service"));
    let rag_retriever = Arc::new(RagRetriever::new(Default::default()));
    let intent_router = Arc::new(
        IntentRouter::load_or_embedded(std::path::Path::new("/nonexistent/intent-rules.yaml"))
            .expect("embedded intent rules"),
    );
    let image_search = Arc::new(ImageSearchService::from_env());
    Arc::new(AppServices {
        config,
        session_store,
        llm_client,
        rbac,
        rag_retriever,
        intent_router,
        image_search,
    })
}

/// Serve the production `api::routes()` on an ephemeral loopback port.
/// Returns the base URL, e.g. `http://127.0.0.1:34567`.
pub async fn spawn_app(state: AppState) -> String {
    let app = api::routes().with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr: SocketAddr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve test app");
    });
    format!("http://{addr}")
}

/// reqwest client with a hard timeout so a hung stream fails the test
/// instead of hanging CI.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

/// Repo-root `skills/` directory, resolved from CARGO_MANIFEST_DIR (api-core/).
pub fn repo_skills_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("skills")
}

/// JSON body for a `UserContext` with the given role.
pub fn user_context_json(role: &str) -> serde_json::Value {
    serde_json::json!({
        "user_id": format!("test-user-{role}"),
        "org_id": "00000000-0000-0000-0000-000000000001",
        "dept_id": "00000000-0000-0000-0000-000000000002",
        "role": role,
        "extra_permissions": []
    })
}
