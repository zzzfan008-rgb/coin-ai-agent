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
}

impl EmbeddingService {
    /// Build an embedding service from explicit parts.
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let base = base_url.into().trim_end_matches('/').to_string();
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            api_key: api_key.into(),
            endpoint: format!("{base}/embeddings"),
            model: model.into(),
        }
    }

    /// Embed a single piece of text, returning a dense float vector.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        if self.api_key.is_empty() {
            bail!("Embedding API key is not configured");
        }

        let body = serde_json::json!({
            "model": self.model,
            "input": text,
        });

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
