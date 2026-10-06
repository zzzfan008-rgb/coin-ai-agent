//! CLIP embedding client — local service, remote APIs, and hybrid fallback
//! (T-019 remote inference; T-027 hybrid local + DashScope).
//!
//! Configured via environment variables:
//!   - `CLIP_PROVIDER`       — `generic` (default when unset, preserves
//!                             pre-T-027 behaviour), `dashscope`/`qwen`,
//!                             `local` (local service only), or `hybrid`
//!                             (local first, DashScope on failure).
//!   - `CLIP_API_ENDPOINT`   — full URL of the embedding endpoint
//!                             (Generic mode) or the DashScope base URL
//!                             (DashScope mode and hybrid fallback; the
//!                             client appends the DashScope API path).
//!   - `CLIP_API_KEY`        — bearer token (falls back to `DASHSCOPE_API_KEY`)
//!   - `CLIP_MODEL`          — model name hint forwarded to the provider
//!   - `CLIP_LOCAL_BASE_URL` — base URL of the local CLIP service
//!                             (default `http://127.0.0.1:8399`; used by
//!                             `local` and `hybrid`; the client appends
//!                             `/encode/image` and `/encode/text`)
//!   - `CLIP_LOCAL_TIMEOUT_SECS` — per-request timeout for the local
//!                             service (default 5s)
//!
//! Local request contract (Generic dialect, T-026 POC `clip_poc_server.py`):
//! ```text
//! POST {base}/encode/image  { "model": "...", "image": "<base64>" }
//! POST {base}/encode/text   { "model": "...", "text":  "..." }
//! → { "embedding": [..], "dim": .., "model": "..", ... }
//! ```
//!
//! The response parser accepts the common provider shapes so the same client
//! works against Replicate-style and OpenAI-style deployments:
//!   - `{ "data": [ { "embedding": [..] } ] }`  (OpenAI/Jina)
//!   - `{ "embedding": [..] }`                   (self-hosted proxy / local POC)
//!   - `{ "output": [..] }` / `{ "output": [[..]] }` (Replicate)
//!   - a bare JSON array `[..]`
//!
//! Hybrid fallback semantics (T-027 increment 1):
//!   - try the local service first;
//!   - fall back to DashScope on: connection error, timeout, HTTP 5xx, or a
//!     response whose shape does not parse as an embedding;
//!   - HTTP 4xx and misconfigured local URLs are *not* fallbackable — they
//!     mean the local service is reachable but rejecting us, or the config
//!     itself is wrong; surfacing them fail-closed beats masking them behind
//!     a remote provider.
//!
//! Every successful encode reports which provider actually served it via
//! `ClipEmbedding::provider` (surfaced as `provider_used` in API responses
//! and indexer logs), and every hybrid fallback logs a `warn!` with the
//! local failure reason and elapsed time.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::Value;

/// Request/response dialect used by the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipMode {
    /// Generic OpenAI/Jina-style single endpoint: `{model, image|text}`.
    Generic,
    /// Alibaba DashScope `multimodal-embedding-v1`:
    /// `POST {host}/api/v1/services/embeddings/multimodal-embedding/multimodal-embedding`
    /// body `{model, input:{contents:[{image|text}]}}`, response
    /// `output.embeddings[].embedding`.
    DashScope,
    /// Local CLIP service only (Generic dialect against
    /// `CLIP_LOCAL_BASE_URL`); failures surface as errors, no fallback.
    Local,
    /// Local service first, DashScope on fallbackable failures (see module
    /// docs for the exact failure classes).
    Hybrid,
}

/// Which provider actually served an encode request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipProviderId {
    /// Generic endpoint (`CLIP_API_ENDPOINT`, Generic mode).
    Generic,
    /// Local CLIP service.
    Local,
    /// Alibaba DashScope (direct or hybrid fallback).
    DashScope,
}

impl ClipProviderId {
    /// Stable wire name, surfaced as `provider_used` in API responses.
    pub fn as_str(&self) -> &'static str {
        match self {
            ClipProviderId::Generic => "generic",
            ClipProviderId::Local => "local",
            ClipProviderId::DashScope => "dashscope",
        }
    }
}

/// An encoded embedding plus the provider that produced it.
#[derive(Debug, Clone)]
pub struct ClipEmbedding {
    pub vector: Vec<f32>,
    pub provider: ClipProviderId,
}

impl ClipEmbedding {
    /// Dimensionality of the returned vector (512 local ViT-B/32,
    /// 1024 DashScope multimodal-embedding-v1).
    pub fn dimension(&self) -> usize {
        self.vector.len()
    }

    /// Wire name for `provider_used` metadata.
    pub fn provider_str(&self) -> &'static str {
        self.provider.as_str()
    }
}

/// Explicit client configuration. `ClipClient::from_env()` is the production
/// entry point; this struct lets tests build hermetic clients without
/// touching process env vars (which race under `cargo test` parallelism).
#[derive(Clone, Debug)]
pub struct ClipConfig {
    /// Raw `CLIP_PROVIDER` value ("" behaves like "generic").
    pub provider: String,
    /// `CLIP_API_ENDPOINT` — Generic endpoint or DashScope base URL.
    pub api_endpoint: String,
    /// Bearer token (`CLIP_API_KEY` or `DASHSCOPE_API_KEY`).
    pub api_key: String,
    /// `CLIP_MODEL`; empty → per-mode default.
    pub model: String,
    /// `CLIP_LOCAL_BASE_URL` (local + hybrid modes).
    pub local_base_url: String,
    /// `CLIP_LOCAL_TIMEOUT_SECS` (local requests only).
    pub local_timeout: Duration,
}

impl ClipConfig {
    /// Build from the `CLIP_*` environment variables (and `DASHSCOPE_API_KEY`).
    pub fn from_env() -> Self {
        let local_timeout = std::env::var("CLIP_LOCAL_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(5));

        Self {
            provider: std::env::var("CLIP_PROVIDER").unwrap_or_default(),
            api_endpoint: std::env::var("CLIP_API_ENDPOINT").unwrap_or_default(),
            api_key: std::env::var("CLIP_API_KEY")
                .or_else(|_| std::env::var("DASHSCOPE_API_KEY"))
                .unwrap_or_default(),
            model: std::env::var("CLIP_MODEL").unwrap_or_default(),
            local_base_url: std::env::var("CLIP_LOCAL_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8399".into()),
            local_timeout,
        }
    }

    /// Map the raw `CLIP_PROVIDER` value to a mode. Unset/unknown values
    /// keep the pre-T-027 Generic behaviour so existing deployments are
    /// untouched.
    pub fn mode(&self) -> ClipMode {
        match self.provider.as_str() {
            "dashscope" | "qwen" => ClipMode::DashScope,
            "local" => ClipMode::Local,
            "hybrid" => ClipMode::Hybrid,
            _ => ClipMode::Generic,
        }
    }

    /// Default model hint when `CLIP_MODEL` is empty.
    fn default_model(&self) -> String {
        match self.mode() {
            ClipMode::DashScope => "multimodal-embedding-v1".into(),
            _ => "clip-vit-base-patch32".into(),
        }
    }
}

/// Client for CLIP image/text embedding services (local, remote, hybrid).
#[derive(Clone)]
pub struct ClipClient {
    http: Client,
    endpoint: String,
    api_key: String,
    model: String,
    mode: ClipMode,
    local_base_url: String,
    local_timeout: Duration,
}

impl ClipClient {
    /// Build from the `CLIP_*` environment variables.
    pub fn from_env() -> Self {
        Self::new(ClipConfig::from_env())
    }

    /// Build from an explicit configuration (hermetic tests use this).
    pub fn new(config: ClipConfig) -> Self {
        let mode = config.mode();
        let model = if config.model.is_empty() {
            config.default_model()
        } else {
            config.model
        };
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            endpoint: config.api_endpoint,
            api_key: config.api_key,
            model,
            mode,
            local_base_url: config.local_base_url,
            local_timeout: config.local_timeout,
        }
    }

    /// True when the active mode has at least one reachable provider
    /// configured. (Hybrid counts as configured when *either* side is set:
    /// a hybrid deployment with only a local URL is valid.)
    pub fn configured(&self) -> bool {
        match self.mode {
            ClipMode::Local => !self.local_base_url.is_empty(),
            ClipMode::Hybrid => !self.local_base_url.is_empty() || !self.endpoint.is_empty(),
            ClipMode::Generic | ClipMode::DashScope => !self.endpoint.is_empty(),
        }
    }

    /// Model name (logged at startup).
    pub fn model_name(&self) -> &str {
        &self.model
    }

    /// Encode an image (raw file bytes, e.g. PNG/JPEG) into a CLIP vector.
    pub async fn encode_image(&self, image_bytes: &[u8]) -> Result<ClipEmbedding> {
        let generic_body = serde_json::json!({
            "model": self.model,
            "image": base64_encode(image_bytes),
        });
        let dashscope_body = serde_json::json!({
            "model": self.model,
            "input": {"contents": [
                {"image": format!("data:image/jpeg;base64,{}", base64_encode(image_bytes))}
            ]},
        });
        self.encode("encode/image", generic_body, dashscope_body)
            .await
    }

    /// Encode text into the same CLIP vector space (cross-modal text→image
    /// search uses this).
    pub async fn encode_text(&self, text: &str) -> Result<ClipEmbedding> {
        let generic_body = serde_json::json!({
            "model": self.model,
            "text": text,
        });
        let dashscope_body = serde_json::json!({
            "model": self.model,
            "input": {"contents": [{"text": text}]},
        });
        self.encode("encode/text", generic_body, dashscope_body)
            .await
    }

    /// Shared dispatch for both encode paths: `local_path` is the suffix to
    /// append to `CLIP_LOCAL_BASE_URL`; the two bodies are the Generic-style
    /// (local) and DashScope-style payloads.
    async fn encode(
        &self,
        local_path: &str,
        generic_body: Value,
        dashscope_body: Value,
    ) -> Result<ClipEmbedding> {
        match self.mode {
            ClipMode::Generic => {
                let vector = self.call_generic(&generic_body).await?;
                Ok(ClipEmbedding {
                    vector,
                    provider: ClipProviderId::Generic,
                })
            }
            ClipMode::DashScope => {
                let vector = self.call_dashscope(&dashscope_body).await?;
                Ok(ClipEmbedding {
                    vector,
                    provider: ClipProviderId::DashScope,
                })
            }
            ClipMode::Local => {
                let vector = self
                    .try_local(local_path, &generic_body)
                    .await
                    .map_err(into_error)?;
                Ok(ClipEmbedding {
                    vector,
                    provider: ClipProviderId::Local,
                })
            }
            ClipMode::Hybrid => {
                if self.local_base_url.is_empty() {
                    tracing::warn!(
                        "CLIP_PROVIDER=hybrid but CLIP_LOCAL_BASE_URL is empty — using DashScope directly"
                    );
                    let vector = self.call_dashscope(&dashscope_body).await?;
                    return Ok(ClipEmbedding {
                        vector,
                        provider: ClipProviderId::DashScope,
                    });
                }
                let started = std::time::Instant::now();
                match self.try_local(local_path, &generic_body).await {
                    Ok(vector) => Ok(ClipEmbedding {
                        vector,
                        provider: ClipProviderId::Local,
                    }),
                    Err(LocalFailure::Fallbackable(e)) => {
                        tracing::warn!(
                            provider = "dashscope",
                            elapsed_ms = started.elapsed().as_millis() as u64,
                            error = %e,
                            "Local CLIP request failed — falling back to DashScope"
                        );
                        let vector = self.call_dashscope(&dashscope_body).await?;
                        Ok(ClipEmbedding {
                            vector,
                            provider: ClipProviderId::DashScope,
                        })
                    }
                    // 4xx or a misconfigured local URL: surface fail-closed,
                    // do not mask a config error behind a remote provider.
                    Err(LocalFailure::Fatal(e)) => Err(e),
                }
            }
        }
    }

    /// Attempt the local service. Errors are classified: fallbackable
    /// (connection error / timeout / 5xx / unrecognisable response shape)
    /// vs fatal (4xx and local config errors).
    async fn try_local(&self, path: &str, body: &Value) -> Result<Vec<f32>, LocalFailure> {
        let url = self.local_url(path);
        let (status, text) = self
            .post(&url, body, Some(self.local_timeout))
            .await
            .map_err(|e| {
                // Timeouts and transport errors (connection refused, reset,
                // DNS) mean the local service is down → fallbackable.
                // Builder errors mean a malformed local URL → config bug,
                // fail closed.
                if e.is_timeout() || !e.is_builder() {
                    LocalFailure::Fallbackable(anyhow::anyhow!("local CLIP request failed: {e}"))
                } else {
                    LocalFailure::Fatal(anyhow::anyhow!("local CLIP URL misconfigured: {e}"))
                }
            })?;

        if status.is_server_error() {
            return Err(LocalFailure::Fallbackable(anyhow::anyhow!(
                "local CLIP {status}: {text}"
            )));
        }
        if !status.is_success() {
            return Err(LocalFailure::Fatal(anyhow::anyhow!(
                "local CLIP {status}: {text}"
            )));
        }

        let json: Value = serde_json::from_str(&text).map_err(|e| {
            LocalFailure::Fallbackable(anyhow::anyhow!("local CLIP response is not JSON: {e}"))
        })?;

        parse_embedding(&json)
            .map_err(|e| LocalFailure::Fallbackable(e.context("local CLIP response shape")))
    }

    /// Local service URL for one of the two encode endpoints.
    fn local_url(&self, path: &str) -> String {
        format!("{}/{}", self.local_base_url.trim_end_matches('/'), path)
    }

    /// Single-endpoint Generic dialect call (pre-T-027 behaviour unchanged).
    async fn call_generic(&self, body: &Value) -> Result<Vec<f32>> {
        if self.endpoint.is_empty() {
            bail!("CLIP_API_ENDPOINT is not configured");
        }
        let (status, text) = self
            .post(&self.endpoint, body, None)
            .await
            .context("CLIP request failed")?;
        if !status.is_success() {
            bail!("CLIP API {status}: {text}");
        }
        let json: Value = serde_json::from_str(&text).context("Failed to decode CLIP response")?;
        parse_embedding(&json)
    }

    /// DashScope dialect call (`multimodal-embedding-v1` path appended to the
    /// configured base URL).
    async fn call_dashscope(&self, body: &Value) -> Result<Vec<f32>> {
        if self.endpoint.is_empty() {
            bail!("CLIP_API_ENDPOINT is not configured (DashScope fallback unavailable)");
        }
        let url = format!(
            "{}/api/v1/services/embeddings/multimodal-embedding/multimodal-embedding",
            self.endpoint.trim_end_matches('/')
        );
        let (status, text) = self
            .post(&url, body, None)
            .await
            .context("DashScope CLIP request failed")?;
        if !status.is_success() {
            bail!("DashScope CLIP API {status}: {text}");
        }
        let json: Value =
            serde_json::from_str(&text).context("Failed to decode DashScope CLIP response")?;
        parse_dashscope_embedding(&json)
    }

    /// Shared POST: JSON body, optional per-request timeout, bearer auth when
    /// a key is configured. Returns (status, raw body text).
    async fn post(
        &self,
        url: &str,
        body: &Value,
        timeout: Option<Duration>,
    ) -> std::result::Result<(reqwest::StatusCode, String), reqwest::Error> {
        let mut req = self
            .http
            .post(url)
            .header("Content-Type", "application/json");
        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }
        let resp = req.json(body).send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Ok((status, text))
    }
}

/// Why a local attempt failed — drives the hybrid fallback decision.
enum LocalFailure {
    /// Local service down or misbehaving; safe to fall back.
    Fallbackable(anyhow::Error),
    /// Local config error or 4xx rejection; surface fail-closed.
    Fatal(anyhow::Error),
}

fn into_error(f: LocalFailure) -> anyhow::Error {
    match f {
        LocalFailure::Fallbackable(e) | LocalFailure::Fatal(e) => e,
    }
}

// ── DashScope response parsing ────────────────────────────────────────────────

/// Extract vectors from `{"output":{"embeddings":[{"embedding":[..]}]}}`.
pub fn parse_dashscope_embedding(json: &Value) -> Result<Vec<f32>> {
    let emb = json
        .get("output")
        .and_then(|o| o.get("embeddings"))
        .and_then(|e| e.as_array())
        .and_then(|a| a.first())
        .and_then(|e| e.get("embedding"))
        .ok_or_else(|| {
            anyhow::anyhow!("DashScope response missing output.embeddings[0].embedding")
        })?;
    let v = json_to_vec(emb)?;
    Ok(v)
}

// ── Response parsing ──────────────────────────────────────────────────────────

/// Extract the embedding vector from any of the common provider response
/// shapes. See module docs.
pub fn parse_embedding(json: &Value) -> Result<Vec<f32>> {
    // 1. OpenAI/Jina style: data[0].embedding
    if let Some(v) = json
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
    {
        if let Some(emb) = v.get("embedding") {
            return json_to_vec(emb);
        }
    }
    // 2. Proxy style: { "embedding": [..] }
    if let Some(emb) = json.get("embedding") {
        return json_to_vec(emb);
    }
    // 3. Replicate style: { "output": [..] } — may be nested one level.
    if let Some(out) = json.get("output") {
        if out.is_array() {
            // Nested [[..]] → take first vector.
            let candidate = if out.get(0).map(|v| v.is_array()).unwrap_or(false) {
                out.get(0).unwrap()
            } else {
                out
            };
            return json_to_vec(candidate);
        }
    }
    // 4. Bare array.
    if json.is_array() {
        return json_to_vec(json);
    }

    bail!("CLIP response contained no recognisable embedding field");
}

fn json_to_vec(v: &Value) -> Result<Vec<f32>> {
    let arr = v
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Embedding is not an array"))?;

    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let n = item
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("Embedding contains a non-numeric value"))?;
        out.push(n as f32);
    }

    if out.is_empty() {
        bail!("Embedding vector is empty");
    }
    Ok(out)
}

// ── Minimal base64 helpers (avoid pulling the base64 crate for this alone) ──

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard-alphabet base64 encoder (with padding).
pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64_ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(B64_ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Decode standard base64 (padding optional). Accepts a `data:` URL prefix.
pub fn base64_decode(input: &str) -> Result<Vec<u8>> {
    let input = input.trim();
    let input = match input.find(',') {
        // tolerate data URLs: data:image/png;base64,xxxx
        Some(i) if input.starts_with("data:") => &input[i + 1..],
        _ => input,
    };

    let lookup =
        |c: u8| -> Option<u32> { B64_ALPHABET.iter().position(|a| *a == c).map(|p| p as u32) };

    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);

    for chunk in clean.chunks(4) {
        let mut buf: u32 = 0;
        let mut pad = 0;
        for (j, &c) in chunk.iter().enumerate() {
            if c == b'=' {
                pad += 1;
                continue;
            }
            let v = lookup(c).ok_or_else(|| anyhow::anyhow!("Invalid base64 character"))?;
            buf |= v << (18 - 6 * j);
        }
        out.push((buf >> 16) as u8);
        if pad < 2 {
            out.push((buf >> 8) as u8);
        }
        if pad == 0 {
            out.push(buf as u8);
        }
    }

    Ok(out)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Ports 18399 / 18400 — deliberately dead ports for failure-path tests,
    // following the same convention as 15432 (DB) and 16333 (Qdrant) in
    // tests/common/mod.rs. Never point tests at 8399: that may host a real
    // local CLIP service on dev machines, and `cargo test` must not touch
    // real services.
    const DEAD_LOCAL_URL: &str = "http://127.0.0.1:18399";
    const DASHSCOPE_PATH: &str =
        "/api/v1/services/embeddings/multimodal-embedding/multimodal-embedding";

    fn fake_vector(n: usize) -> Value {
        let vals: Vec<f64> = (0..n).map(|i| (i as f64) * 0.001).collect();
        serde_json::to_value(vals).unwrap()
    }

    fn test_config(provider: &str, local_base_url: &str, endpoint: &str) -> ClipConfig {
        ClipConfig {
            provider: provider.into(),
            api_endpoint: endpoint.into(),
            api_key: String::new(),
            model: String::new(),
            local_base_url: local_base_url.into(),
            local_timeout: Duration::from_secs(5),
        }
    }

    /// DashScope-shaped success response (1024 dims — the real service dim).
    fn dashscope_ok(n: usize) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "output": {"embeddings": [{"embedding": fake_vector(n)}]},
            "request_id": "test",
        }))
    }

    /// Mount a DashScope success mock; returns the server.
    async fn dashscope_server(n: usize) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(DASHSCOPE_PATH))
            .respond_with(dashscope_ok(n))
            .mount(&server)
            .await;
        server
    }

    async fn requests(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
    }

    #[test]
    fn maps_provider_env_to_mode() {
        assert_eq!(
            ClipConfig {
                provider: String::new(),
                ..test_config("", "", "")
            }
            .mode(),
            ClipMode::Generic
        );
        assert_eq!(test_config("generic", "", "").mode(), ClipMode::Generic);
        assert_eq!(
            test_config("unknown-value", "", "").mode(),
            ClipMode::Generic
        );
        assert_eq!(test_config("dashscope", "", "").mode(), ClipMode::DashScope);
        assert_eq!(test_config("qwen", "", "").mode(), ClipMode::DashScope);
        assert_eq!(test_config("local", "", "").mode(), ClipMode::Local);
        assert_eq!(test_config("hybrid", "", "").mode(), ClipMode::Hybrid);
    }

    #[test]
    fn default_models_follow_mode() {
        assert_eq!(
            test_config("", "", "").default_model(),
            "clip-vit-base-patch32"
        );
        assert_eq!(
            test_config("local", "", "").default_model(),
            "clip-vit-base-patch32"
        );
        assert_eq!(
            test_config("hybrid", "", "").default_model(),
            "clip-vit-base-patch32"
        );
        assert_eq!(
            test_config("dashscope", "", "").default_model(),
            "multimodal-embedding-v1"
        );
    }

    #[test]
    fn parses_openai_style_512() {
        let json = serde_json::json!({
            "object": "list",
            "data": [{ "object": "embedding", "embedding": fake_vector(512) }],
        });
        let v = parse_embedding(&json).unwrap();
        assert_eq!(v.len(), 512);
    }

    #[test]
    fn parses_openai_style_768() {
        let json = serde_json::json!({ "data": [{ "embedding": fake_vector(768) }] });
        let v = parse_embedding(&json).unwrap();
        assert_eq!(v.len(), 768);
    }

    #[test]
    fn parses_proxy_style() {
        let json = serde_json::json!({ "embedding": fake_vector(512) });
        assert_eq!(parse_embedding(&json).unwrap().len(), 512);
    }

    #[test]
    fn parses_replicate_style() {
        let json = serde_json::json!({ "output": fake_vector(512) });
        assert_eq!(parse_embedding(&json).unwrap().len(), 512);
    }

    #[test]
    fn parses_nested_replicate_style() {
        let json = serde_json::json!({ "output": [fake_vector(512)] });
        assert_eq!(parse_embedding(&json).unwrap().len(), 512);
    }

    #[test]
    fn parses_bare_array() {
        let json = fake_vector(768);
        assert_eq!(parse_embedding(&json).unwrap().len(), 768);
    }

    #[test]
    fn rejects_unrecognised_shape() {
        let json = serde_json::json!({ "unexpected": true });
        assert!(parse_embedding(&json).is_err());
    }

    #[test]
    fn rejects_empty_vector() {
        let json = serde_json::json!({ "embedding": [] });
        assert!(parse_embedding(&json).is_err());
    }

    #[test]
    fn parses_dashscope_multimodal_response() {
        let json = serde_json::json!({
            "output": {"embeddings": [{"embedding": fake_vector(1024)}]},
            "request_id": "x",
        });
        assert_eq!(parse_dashscope_embedding(&json).unwrap().len(), 1024);
    }

    #[test]
    fn rejects_dashscope_shape_without_output() {
        let json = serde_json::json!({ "data": [{"embedding": fake_vector(512)}] });
        assert!(parse_dashscope_embedding(&json).is_err());
    }

    #[test]
    fn base64_round_trip() {
        let data: Vec<u8> = (0u32..255).map(|i| i as u8).collect();
        let enc = base64_encode(&data);
        assert_eq!(base64_decode(&enc).unwrap(), data);
    }

    #[test]
    fn base64_decodes_data_url() {
        let data = b"hello";
        let enc = base64_encode(data);
        let url = format!("data:image/png;base64,{enc}");
        assert_eq!(base64_decode(&url).unwrap(), data);
    }

    // ── T-027 hybrid/local mode tests (hermetic: wiremock in-process) ─────────

    #[tokio::test]
    async fn hybrid_uses_local_and_never_calls_dashscope_when_healthy() {
        let local = MockServer::start().await;
        let dashscope = dashscope_server(1024).await;

        // Local POC contract: per-endpoint paths, {"embedding":[...]} body.
        Mock::given(method("POST"))
            .and(path("/encode/image"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embedding": fake_vector(512), "dim": 512, "device": "cpu",
            })))
            .mount(&local)
            .await;
        Mock::given(method("POST"))
            .and(path("/encode/text"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embedding": fake_vector(512), "dim": 512, "device": "cpu",
            })))
            .mount(&local)
            .await;

        let client = ClipClient::new(test_config("hybrid", &local.uri(), &dashscope.uri()));

        let img = client
            .encode_image(b"fake-jpeg")
            .await
            .expect("image encode");
        assert_eq!(img.provider, ClipProviderId::Local);
        assert_eq!(img.dimension(), 512);
        assert_eq!(img.vector.len(), 512);
        assert!(
            (img.vector[1] - 0.001).abs() < 1e-6,
            "vector content must round-trip"
        );

        let txt = client
            .encode_text("米白色羊绒针织衫")
            .await
            .expect("text encode");
        assert_eq!(txt.provider, ClipProviderId::Local);
        assert_eq!(txt.dimension(), 512);

        // Both local endpoints hit; DashScope untouched.
        assert_eq!(requests(&local).await, 2);
        assert_eq!(requests(&dashscope).await, 0);
    }

    #[tokio::test]
    async fn hybrid_falls_back_when_local_unreachable() {
        let dashscope = dashscope_server(1024).await;
        let client = ClipClient::new(test_config("hybrid", DEAD_LOCAL_URL, &dashscope.uri()));

        let emb = client
            .encode_image(b"fake-jpeg")
            .await
            .expect("fallback encode");
        assert_eq!(emb.provider, ClipProviderId::DashScope);
        assert_eq!(emb.dimension(), 1024);
        assert_eq!(requests(&dashscope).await, 1);
    }

    #[tokio::test]
    async fn hybrid_falls_back_on_local_5xx() {
        let local = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/encode/image"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&local)
            .await;
        let dashscope = dashscope_server(1024).await;

        let client = ClipClient::new(test_config("hybrid", &local.uri(), &dashscope.uri()));
        let emb = client
            .encode_image(b"fake-jpeg")
            .await
            .expect("fallback encode");
        assert_eq!(emb.provider, ClipProviderId::DashScope);
        assert_eq!(emb.dimension(), 1024);
        assert_eq!(requests(&local).await, 1);
        assert_eq!(requests(&dashscope).await, 1);
    }

    #[tokio::test]
    async fn hybrid_falls_back_on_local_bad_shape() {
        let local = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/encode/image"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "unexpected": true })),
            )
            .mount(&local)
            .await;
        let dashscope = dashscope_server(1024).await;

        let client = ClipClient::new(test_config("hybrid", &local.uri(), &dashscope.uri()));
        let emb = client
            .encode_image(b"fake-jpeg")
            .await
            .expect("fallback encode");
        assert_eq!(emb.provider, ClipProviderId::DashScope);
        assert_eq!(requests(&dashscope).await, 1);
    }

    #[tokio::test]
    async fn hybrid_falls_back_on_local_timeout() {
        let local = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/encode/image"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "embedding": fake_vector(512) }))
                    .set_delay(Duration::from_secs(3)),
            )
            .mount(&local)
            .await;
        let dashscope = dashscope_server(1024).await;

        let mut cfg = test_config("hybrid", &local.uri(), &dashscope.uri());
        cfg.local_timeout = Duration::from_millis(300);
        let client = ClipClient::new(cfg);

        let emb = client
            .encode_image(b"fake-jpeg")
            .await
            .expect("fallback encode");
        assert_eq!(emb.provider, ClipProviderId::DashScope);
        assert_eq!(requests(&dashscope).await, 1);
    }

    #[tokio::test]
    async fn hybrid_does_not_fallback_on_local_4xx() {
        // 4xx means the local service is reachable but rejecting us — a config
        // or auth problem that fallback would only mask. Fail closed.
        let local = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/encode/image"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad token"))
            .mount(&local)
            .await;
        let dashscope = dashscope_server(1024).await;

        let client = ClipClient::new(test_config("hybrid", &local.uri(), &dashscope.uri()));
        let err = client
            .encode_image(b"fake-jpeg")
            .await
            .expect_err("must not fall back");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("401"),
            "error must carry the local 4xx status: {msg}"
        );
        assert_eq!(
            requests(&dashscope).await,
            0,
            "DashScope must not be called on 4xx"
        );
    }

    #[tokio::test]
    async fn dashscope_mode_never_calls_local() {
        // Regression guard: CLIP_PROVIDER=dashscope keeps calling DashScope
        // directly even when a local CLIP service is up.
        let local = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/encode/image"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "embedding": fake_vector(512) })),
            )
            .mount(&local)
            .await;
        let dashscope = dashscope_server(1024).await;

        let client = ClipClient::new(test_config("dashscope", &local.uri(), &dashscope.uri()));
        let emb = client
            .encode_image(b"fake-jpeg")
            .await
            .expect("dashscope encode");
        assert_eq!(emb.provider, ClipProviderId::DashScope);
        assert_eq!(emb.dimension(), 1024);
        assert_eq!(
            requests(&local).await,
            0,
            "local must not be called in dashscope mode"
        );
        assert_eq!(requests(&dashscope).await, 1);
    }

    #[tokio::test]
    async fn local_mode_uses_local_only_without_dashscope_config() {
        let local = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/encode/text"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "embedding": fake_vector(512) })),
            )
            .mount(&local)
            .await;

        // No DashScope endpoint at all — local mode must not need one.
        let client = ClipClient::new(test_config("local", &local.uri(), ""));
        assert!(
            client.configured(),
            "local mode with base URL is configured"
        );
        let emb = client
            .encode_text("亚麻连衣裙")
            .await
            .expect("local text encode");
        assert_eq!(emb.provider, ClipProviderId::Local);
        assert_eq!(emb.dimension(), 512);
    }

    #[tokio::test]
    async fn generic_mode_behaviour_unchanged() {
        // Regression guard for pre-T-027 deployments: unset CLIP_PROVIDER
        // still POSTs the Generic body straight to CLIP_API_ENDPOINT.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "embedding": fake_vector(512) })),
            )
            .mount(&server)
            .await;

        let client = ClipClient::new(test_config("", "", &server.uri()));
        let emb = client
            .encode_image(b"fake-jpeg")
            .await
            .expect("generic encode");
        assert_eq!(emb.provider, ClipProviderId::Generic);
        assert_eq!(emb.dimension(), 512);
        // The request body must stay the Generic dialect (no data: URL).
        let seen = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&seen[0].body).unwrap();
        assert_eq!(body["image"], base64_encode(b"fake-jpeg"));
        assert!(
            body["input"].is_null(),
            "Generic mode must not send DashScope 'input'"
        );
    }

    #[tokio::test]
    async fn hybrid_skips_local_when_no_local_url_is_configured() {
        let dashscope = dashscope_server(1024).await;
        let client = ClipClient::new(test_config("hybrid", "", &dashscope.uri()));
        let emb = client
            .encode_image(b"fake-jpeg")
            .await
            .expect("dashscope encode");
        assert_eq!(emb.provider, ClipProviderId::DashScope);
        assert_eq!(requests(&dashscope).await, 1);
    }
}
