//! API module — routes and handler registration.

pub mod handlers;

use axum::{
    routing::{get, post},
    Router,
};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Health
        .route("/health", get(handlers::health))
        // Chat
        .route(
            "/v1/chat/completions",
            post(handlers::chat_completions),
        )
        .route(
            "/v1/chat/completions/stream",
            post(handlers::chat_stream),
        )
        // Sessions
        .route(
            "/internal/sessions",
            post(handlers::create_session),
        )
        .route(
            "/internal/sessions/:id",
            get(handlers::get_session),
        )
        .route(
            "/internal/sessions/:id/messages",
            get(handlers::get_messages).post(handlers::save_message),
        )
        // Skills
        .route(
            "/internal/skills/list",
            get(handlers::list_skills),
        )
        .route(
            "/internal/skills/execute",
            post(handlers::execute_skill),
        )
        // Knowledge base (RAG)
        .route(
            "/internal/knowledge/search",
            post(handlers::knowledge_search),
        )
        .route(
            "/api/knowledge/documents",
            post(handlers::upload_knowledge_document),
        )
        // MCP servers
        .route(
            "/api/mcp/servers",
            get(handlers::mcp_servers),
        )
        .route(
            "/api/mcp/servers/:id/tools",
            get(handlers::mcp_server_tools),
        )
}
