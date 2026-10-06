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

// ── CLIP health (T-027 increment 2) ─────────────────────────────────────────

/// GET /health must report the CLIP section: active mode, and a local
/// endpoint that is configured but unreachable (dead 18399) without the
/// endpoint itself failing the request.
#[tokio::test]
async fn health_reports_clip_section_with_local_down() {
    let base = common::spawn_app(common::test_state().await).await;
    let resp = common::client()
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("GET /health");
    assert_eq!(resp.status().as_u16(), 200);

    let body: serde_json::Value = resp.json().await.expect("health JSON");
    let clip = &body["services"]["clip"];
    assert_eq!(clip["mode"], "local");
    assert_eq!(clip["model"], "clip-vit-base-patch32");
    assert_eq!(clip["local"]["configured"], true);
    assert_eq!(clip["local"]["reachable"], false);
    assert_eq!(clip["local"]["endpoint"], "http://127.0.0.1:18399");
    assert!(clip["local"]["error"].is_string());
}

/// GET /health with a wiremock-backed local CLIP reporting healthy must
/// surface reachability fields and the most recent fallback event.
#[tokio::test]
async fn health_reports_clip_local_up_and_last_fallback() {
    use api_core::images::clip::{record_fallback, ClipClient, ClipConfig};
    use api_core::images::style_collection_name;
    use api_core::images::ImageSearchService;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Local CLIP: /health 200 with POC-contract fields.
    let local = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "model": "clip-vit-base-patch32",
            "device": "cpu",
            "dim": 512,
            "degraded": false,
        })))
        .mount(&local)
        .await;

    // Seed a uniquely-marked fallback event.
    let marker = format!("it-marker-{}", uuid::Uuid::new_v4());
    record_fallback(marker.clone(), 42);

    let clip = ClipClient::new(ClipConfig {
        provider: "hybrid".into(),
        api_endpoint: String::new(),
        api_key: String::new(),
        model: String::new(),
        local_base_url: local.uri(),
        local_timeout: std::time::Duration::from_secs(2),
    });
    let collection = style_collection_name("local", "clip-vit-base-patch32", 512);
    let image_search = ImageSearchService::new(
        clip,
        "http://127.0.0.1:16333",
        // Hybrid-like routes: 512 writable + 1024 read-only legacy.
        vec![
            (512, collection, true),
            (1024, "style_images".into(), false),
        ],
    );

    let base = common::spawn_app(common::test_state_with(image_search).await).await;
    let resp = common::client()
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("GET /health");
    assert_eq!(resp.status().as_u16(), 200);

    let body: serde_json::Value = resp.json().await.expect("health JSON");
    let clip_section = &body["services"]["clip"];
    assert_eq!(clip_section["mode"], "hybrid");
    assert_eq!(clip_section["local"]["reachable"], true);
    assert_eq!(clip_section["local"]["model"], "clip-vit-base-patch32");
    assert_eq!(clip_section["local"]["device"], "cpu");
    assert_eq!(clip_section["local"]["dim"], 512);

    assert_eq!(clip_section["last_fallback"]["reason"], marker);
    assert_eq!(clip_section["last_fallback"]["elapsed_ms"], 42);
    assert!(
        clip_section["last_fallback"]["at"].is_string(),
        "fallback event must carry a timestamp"
    );
}
