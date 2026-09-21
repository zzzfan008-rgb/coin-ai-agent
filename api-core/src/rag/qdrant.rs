//! Qdrant vector store — manages collections and point upsert/search over REST.
//!
//! Uses the Qdrant HTTP API (port 6333) rather than the gRPC client to keep
//! the dependency tree light (no tonic/prost). The REST API is fully
//! equivalent for dense-vector CRUD and ANN search.
//!
//! Collection layout:
//!   - One collection (`fashion_knowledge`) holds chunks from every tenant.
//!   - Tenant isolation is enforced at query time via a payload `must` filter
//!     on `org_id` AND `dept_id`.

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use super::{QdrantChunk, QdrantSearchResult};

/// A point with a free-form payload (used by the CLIP image indexer).
pub struct RawPoint {
    pub id: String,
    pub vector: Vec<f32>,
    pub payload: Value,
}

/// Thin REST client for a single Qdrant collection.
#[derive(Clone)]
pub struct QdrantStore {
    http: Client,
    base_url: String,
    collection: String,
}

impl QdrantStore {
    /// Connect to a Qdrant instance.
    ///
    /// `base_url`   — e.g. `http://localhost:6333`
    /// `collection` — collection name, e.g. `fashion_knowledge`
    pub fn new(base_url: &str, collection: &str) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            collection: collection.to_string(),
        }
    }

    // ── Collection management ────────────────────────────────────────────────

    /// Create the collection if it does not already exist.
    /// Uses cosine distance with the given vector dimension.
    pub async fn ensure_collection(&self, vector_dim: usize) -> Result<()> {
        let info_url = format!("{}/collections/{}", self.base_url, self.collection);

        let resp = self.http.get(&info_url).send().await?;
        if resp.status().is_success() {
            return Ok(()); // already exists
        }

        let create_url = format!("{}/collections", self.base_url);
        let body = serde_json::json!({
            "name": self.collection,
            "vectors": {
                "size": vector_dim,
                "distance": "Cosine",
            },
        });

        let resp = self
            .http
            .put(&create_url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Failed to create Qdrant collection '{}': {text}", self.collection);
        }

        tracing::info!(collection = %self.collection, "Created Qdrant collection");
        Ok(())
    }

    // ── Point upsert ─────────────────────────────────────────────────────────

    /// Upsert (insert-or-replace) a batch of points.
    pub async fn upsert_chunks(&self, chunks: Vec<QdrantChunk>) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let url = format!(
            "{}/collections/{}/points?wait=true",
            self.base_url, self.collection
        );

        let points: Vec<_> = chunks
            .into_iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "vector": c.vector,
                    "payload": c.payload,
                })
            })
            .collect();

        let body = serde_json::json!({ "points": points });

        let resp = self
            .http
            .put(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Qdrant upsert request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("Qdrant upsert failed ({}): {text}", status);
        }

        Ok(())
    }

    // ── Generic raw-point upsert ─────────────────────────────────────────────

    /// Upsert points with arbitrary JSON payloads (used by the image indexer).
    pub async fn upsert_raw_points(&self, points: Vec<RawPoint>) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }

        let url = format!(
            "{}/collections/{}/points?wait=true",
            self.base_url, self.collection
        );

        let points: Vec<_> = points
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "vector": p.vector,
                    "payload": p.payload,
                })
            })
            .collect();

        let body = serde_json::json!({ "points": points });

        let resp = self
            .http
            .put(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Qdrant raw upsert request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("Qdrant raw upsert failed ({}): {text}", status);
        }

        Ok(())
    }

    // ── ANN search ──────────────────────────────────────────────────────────

    /// Search for the closest points to `vector`, filtered by org and dept.
    ///
    /// Returns at most `limit` hits ordered by descending cosine similarity.
    /// A non-existent collection or empty result yields an empty `Vec`, not
    /// an error, so callers can degrade gracefully.
    pub async fn search(
        &self,
        vector: &[f32],
        org_id: &str,
        dept_id: &str,
        limit: usize,
    ) -> Result<Vec<QdrantSearchResult>> {
        let url = format!(
            "{}/collections/{}/points/search",
            self.base_url, self.collection
        );

        // Tenant isolation: both conditions must match.
        let filter = serde_json::json!({
            "must": [
                { "key": "org_id",  "match": { "value": org_id } },
                { "key": "dept_id", "match": { "value": dept_id } },
            ]
        });

        let body = serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true,
            "filter": filter,
        });

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Qdrant search request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            // Common during first-run before any docs are indexed.
            tracing::debug!("Qdrant search returned {}: {text}", status);
            return Ok(Vec::new());
        }

        let envelope: QdrantEnvelope<Vec<QdrantSearchResult>> =
            resp.json().await.context("Failed to decode Qdrant search response")?;

        Ok(envelope.result)
    }

    /// Delete all points belonging to a document (used on re-index or soft-delete).
    pub async fn delete_by_document(&self, doc_id: &str) -> Result<()> {
        let url = format!(
            "{}/collections/{}/points/delete?wait=true",
            self.base_url, self.collection
        );

        let body = serde_json::json!({
            "filter": {
                "must": [
                    { "key": "doc_id", "match": { "value": doc_id } }
                ]
            }
        });

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            tracing::warn!("Qdrant delete-by-document failed: {text}");
        }

        Ok(())
    }
}

// ── Qdrant API envelope ───────────────────────────────────────────────────────

/// All Qdrant REST responses wrap their payload in `{"result": ..., "status": ...}`.
#[derive(Debug, Deserialize)]
struct QdrantEnvelope<T> {
    result: T,
}
