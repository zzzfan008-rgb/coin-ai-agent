//! Embedding service — calls DeepSeek's OpenAI-compatible embeddings endpoint.
//!
//! ```text
//! POST {base_url}/embeddings
//! Authorization: Bearer {api_key}
//! { "model": "deepseek-embedding", "input": "text to embed" }
//! ```
//!
//! Returns a dense vector (1536 dimensions by default).

use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Deserialize;

/// Generates text embeddings via an OpenAI-compatible `/embeddings` API.
#[derive(Clone)]
pub struct EmbeddingService {
    http: Client,
    api_key: String,
    endpoint: String,
    model: String,
    /// Vector dimensionality requested from the provider (0 = provider default).
    dimensions: usize,
}

impl EmbeddingService {
    /// Build an embedding service from explicit parts.
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        dimensions: usize,
    ) -> Self {
        let base = base_url.into().trim_end_matches('/').to_string();
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            api_key: api_key.into(),
            endpoint: format!("{base}/embeddings"),
            model: model.into(),
            dimensions,
        }
    }

    /// Embed a single piece of text, returning a dense float vector.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        if self.api_key.is_empty() {
            bail!("Embedding API key is not configured");
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "input": text,
        });
        if self.dimensions > 0 {
            body["dimensions"] = serde_json::json!(self.dimensions);
        }

        let resp = self
            .http
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Embedding request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let raw = resp.text().await.unwrap_or_default();
            bail!("Embedding API {status}: {raw}");
        }

        let parsed: EmbedResponse = resp
            .json()
            .await
            .context("Failed to decode embedding response")?;

        let vector = parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| anyhow::anyhow!("Embedding response contained no data"))?;

        if vector.is_empty() {
            bail!("Embedding API returned an empty vector");
        }

        Ok(vector)
    }

    /// Expected embedding dimension (used for Qdrant collection creation).
    pub fn model_name(&self) -> &str {
        &self.model
    }

    /// `(provider, model)` identity for collection metadata and the
    /// dim+model collection name.
    ///
    /// Provider is derived from the CONFIGURED endpoint host rather than
    /// hardcoded, so it follows where embeddings actually go:
    ///   - hosts containing dashscope / qianwen / maas → `dashscope`
    ///     (Aliyun text-embedding models, including the qwen-compatible
    ///     proxy this project defaults to);
    ///   - host containing deepseek → `deepseek`;
    ///   - anything else (localhost proxies included) → `generic`.
    /// Model is the effective model name sent on every request.
    pub fn identity(&self) -> (&'static str, &str) {
        let host = self
            .endpoint
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split(['/', ':'])
            .next()
            .unwrap_or("")
            .to_lowercase();
        let provider =
            if host.contains("dashscope") || host.contains("qianwen") || host.contains("maas") {
                "dashscope"
            } else if host.contains("deepseek") {
                "deepseek"
            } else {
                "generic"
            };
        (provider, &self.model)
    }
}

// ── Response types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Debug, Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}
