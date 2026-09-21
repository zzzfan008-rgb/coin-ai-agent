//! CLIP third-party API client (Phase 1B — remote inference, no local GPU).
//!
//! Configured via environment variables:
//!   - `CLIP_API_ENDPOINT` — full URL of the embedding endpoint
//!   - `CLIP_API_KEY`      — bearer token (optional for self-hosted proxies)
//!   - `CLIP_MODEL`        — model name hint forwarded to the provider
//!
//! Request contract (works with an OpenAI-compatible CLIP proxy / Jina-style
//! multimodal embeddings / a thin self-hosted CLIP server):
//! ```json
//! { "model": "...", "image": "<base64>" }   // image encoding
//! { "model": "...", "text":  "a red dress" } // text encoding
//! ```
//!
//! The response parser accepts the common provider shapes so the same client
//! works against Replicate-style and OpenAI-style deployments:
//!   - `{ "data": [ { "embedding": [..] } ] }`  (OpenAI/Jina)
//!   - `{ "embedding": [..] }`                   (self-hosted proxy)
//!   - `{ "output": [..] }` / `{ "output": [[..]] }` (Replicate)
//!   - a bare JSON array `[..]`

use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::Value;

/// Client for a remote CLIP image/text embedding service.
#[derive(Clone)]
pub struct ClipClient {
    http: Client,
    endpoint: String,
    api_key: String,
    model: String,
}

impl ClipClient {
    /// Build from `CLIP_API_*` environment variables.
    pub fn from_env() -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            endpoint: std::env::var("CLIP_API_ENDPOINT").unwrap_or_default(),
            api_key: std::env::var("CLIP_API_KEY").unwrap_or_default(),
            model: std::env::var("CLIP_MODEL").unwrap_or_else(|_| "clip-vit-base-patch32".into()),
        }
    }

    /// True when an endpoint has been configured.
    pub fn configured(&self) -> bool {
        !self.endpoint.is_empty()
    }

    /// Model name (logged at startup).
    pub fn model_name(&self) -> &str {
        &self.model
    }

    /// Encode an image (raw file bytes, e.g. PNG/JPEG) into a CLIP vector.
    pub async fn encode_image(&self, image_bytes: &[u8]) -> Result<Vec<f32>> {
        let payload = serde_json::json!({
            "model": self.model,
            "image": base64_encode(image_bytes),
        });
        self.call(payload).await
    }

    /// Encode text into the same CLIP vector space (cross-modal text→image
    /// search uses this).
    pub async fn encode_text(&self, text: &str) -> Result<Vec<f32>> {
        let payload = serde_json::json!({
            "model": self.model,
            "text": text,
        });
        self.call(payload).await
    }

    async fn call(&self, body: Value) -> Result<Vec<f32>> {
        if self.endpoint.is_empty() {
            bail!("CLIP_API_ENDPOINT is not configured");
        }

        let mut req = self
            .http
            .post(&self.endpoint)
            .header("Content-Type", "application/json");
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req
            .json(&body)
            .send()
            .await
            .context("CLIP request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let raw = resp.text().await.unwrap_or_default();
            bail!("CLIP API {status}: {raw}");
        }

        let json: Value = resp
            .json()
            .await
            .context("Failed to decode CLIP response")?;

        parse_embedding(&json)
    }
}

// ── Response parsing ──────────────────────────────────────────────────────────

/// Extract the embedding vector from any of the common provider response
/// shapes. See module docs.
pub fn parse_embedding(json: &Value) -> Result<Vec<f32>> {
    // 1. OpenAI/Jina style: data[0].embedding
    if let Some(v) = json.get("data").and_then(|d| d.as_array()).and_then(|a| a.first()) {
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
            let candidate = if out
                .get(0)
                .map(|v| v.is_array())
                .unwrap_or(false)
            {
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

const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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

    let lookup = |c: u8| -> Option<u32> {
        B64_ALPHABET.iter().position(|a| *a == c).map(|p| p as u32)
    };

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

    fn fake_vector(n: usize) -> Value {
        let vals: Vec<f64> = (0..n).map(|i| (i as f64) * 0.001).collect();
        serde_json::to_value(vals).unwrap()
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
}
