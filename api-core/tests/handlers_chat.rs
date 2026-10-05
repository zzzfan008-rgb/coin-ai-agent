//! Route regression tests — chat & health endpoints (P0-2 / T-022).
//!
//! Replaces the deleted `assert!(true)` shells with real assertions against
//! the production router served over HTTP:
//!   - /health degrades gracefully when DB/LLM are unreachable
//!   - POST /v1/chat/completions/stream returns a real SSE stream with a
//!     single `data: ` prefix (regression: the old double-prefix bug)
//!   - method mismatch on the chat route yields 405

mod common;

/// GET /health with no DB and no reachable LLM must still return 200 with
/// an honest degraded payload — not a 5xx, not an empty body.
#[tokio::test]
async fn health_returns_degraded_json_when_backends_down() {
    let base = common::spawn_app(common::test_state().await).await;
    let resp = common::client()
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("GET /health");

    assert_eq!(resp.status().as_u16(), 200, "health must not 5xx");

    let body: serde_json::Value = resp.json().await.expect("health JSON");
    assert_eq!(
        body["status"], "degraded",
        "no DB → status must be degraded, got: {body}"
    );
    assert_eq!(
        body["services"]["database"], "down",
        "services.database must report down, got: {body}"
    );
    assert!(
        body["version"].is_string(),
        "version must be present, got: {body}"
    );
}

/// Test harness now serves `api_core::build_router()` — the exact
/// production middleware stack. This pins the CORS layer: a request with an
/// Origin header must come back with `access-control-allow-origin` (F4).
#[tokio::test]
async fn cors_layer_is_active_on_health() {
    let base = common::spawn_app(common::test_state().await).await;
    let resp = common::client()
        .get(format!("{base}/health"))
        .header(reqwest::header::ORIGIN, "https://example.com")
        .send()
        .await
        .expect("GET /health with Origin");

    assert_eq!(resp.status().as_u16(), 200, "health must not 5xx");
    assert_eq!(
        resp.headers()
            .get(reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok()),
        Some("*"),
        "permissive CORS must answer access-control-allow-origin: *"
    );
}

/// SSE regression: the stream endpoint must answer with
/// `content-type: text/event-stream` and emit single-prefixed `data: `
/// events — never the old `data: data: ` double prefix that broke
/// client-side JSON.parse (fixed in 6222db0, must not regress).
#[tokio::test]
async fn chat_stream_returns_sse_with_single_data_prefix() {
    let base = common::spawn_app(common::test_state().await).await;
    let body = serde_json::json!({
        "model": "__default__",
        "messages": [{"role": "user", "content": "hi"}],
        "stream": true,
        "user_context": common::user_context_json("designer"),
    });

    let resp = common::client()
        .post(format!("{base}/v1/chat/completions/stream"))
        .json(&body)
        .send()
        .await
        .expect("POST stream");

    assert_eq!(resp.status().as_u16(), 200, "SSE endpoint must return 200");
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("text/event-stream"),
        "content-type must be text/event-stream, got: {content_type:?}"
    );

    // The LLM target is unreachable in tests, so the spawned task fails
    // fast and the stream closes; body must still be valid single-prefix SSE.
    let text = resp.text().await.expect("read SSE body");
    assert!(
        text.contains("data: "),
        "SSE body must contain data: events, got: {text:?}"
    );
    assert!(
        !text.contains("data: data:"),
        "SSE body must not contain the double-prefix regression, got: {text:?}"
    );
}

/// Route-table regression: /v1/chat/completions is POST-only; a GET must
/// be rejected with 405 rather than falling through to another handler.
#[tokio::test]
async fn chat_completions_rejects_get_with_405() {
    let base = common::spawn_app(common::test_state().await).await;
    let resp = common::client()
        .get(format!("{base}/v1/chat/completions"))
        .send()
        .await
        .expect("GET /v1/chat/completions");

    assert_eq!(
        resp.status().as_u16(),
        405,
        "GET on POST-only chat route must be 405"
    );
}
