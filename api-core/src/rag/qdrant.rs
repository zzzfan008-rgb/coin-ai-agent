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

/// Legacy collections created in the DashScope era (`fashion_knowledge`
/// chunks, `style_images` 1024-dim points). T-027 tooling must never
/// modify them — new pipelines carry their own dim+model-named
/// collections, and writes against a protected name are refused in code,
/// not merely by convention.
pub const PROTECTED_LEGACY_COLLECTIONS: &[&str] = &["fashion_knowledge", "style_images"];

/// Whether `collection` names a protected legacy collection.
pub fn is_protected_legacy(collection: &str) -> bool {
    PROTECTED_LEGACY_COLLECTIONS.contains(&collection)
}

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
    /// When false every mutating call (upserts, deletes) fails before any
    /// HTTP request. Read-only stores back hybrid-fallback routes onto
    /// legacy collections.
    allow_writes: bool,
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
            allow_writes: true,
        }
    }

    /// Connect with mutating operations disabled. Any upsert/delete is
    /// rejected client-side even if the server would accept it — this is
    /// the hard guard for routes that read legacy collections.
    pub fn new_read_only(base_url: &str, collection: &str) -> Self {
        let mut store = Self::new(base_url, collection);
        store.allow_writes = false;
        store
    }

    /// Name of the bound collection.
    pub fn collection_name(&self) -> &str {
        &self.collection
    }

    /// Fail before any mutation when the store is read-only.
    fn assert_writable(&self) -> Result<()> {
        if !self.allow_writes {
            bail!(
                "write refused: Qdrant collection '{}' is bound read-only (legacy protection)",
                self.collection
            );
        }
        Ok(())
    }

    // ── Collection management ────────────────────────────────────────────────

    /// Create the collection if it does not already exist.
    ///
    /// Uses cosine distance with the given vector dimension. `metadata`
    /// (Qdrant 1.19+ collection metadata, returned at
    /// `result.config.metadata`) carries application-level identity —
    /// `vector_dim`, `model`, `provider` — so a collection's embedding
    /// space is self-describing rather than inferable only from its name.
    ///
    /// When the collection already exists its ACTUAL vector size is read
    /// from `result.config.params.vectors.size` and compared against
    /// `vector_dim`: a mismatch fails loudly (dimension drift) instead of
    /// letting a wrongly-sized vector reach the collection at query time.
    pub async fn ensure_collection(
        &self,
        vector_dim: usize,
        metadata: Option<Value>,
    ) -> Result<()> {
        let info_url = format!("{}/collections/{}", self.base_url, self.collection);

        let resp = self.http.get(&info_url).send().await?;
        if resp.status().is_success() {
            let info: Value = resp.json().await?;
            return verify_vectors_size(&info, &self.collection, vector_dim);
        }

        // Qdrant creates a collection at PUT /collections/{name}
        // (PUT /collections with a body returns 404).
        let create_url = format!("{}/collections/{}", self.base_url, self.collection);
        let mut body = serde_json::json!({
            "vectors": {
                "size": vector_dim,
                "distance": "Cosine",
            },
        });
        if let Some(metadata) = metadata {
            // Arbitrary JSON metadata (Qdrant 1.19+).
            body["metadata"] = metadata;
        }

        let resp = self
            .http
            .put(&create_url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!(
                "Failed to create Qdrant collection '{}': {text}",
                self.collection
            );
        }

        tracing::info!(collection = %self.collection, vector_dim, "Created Qdrant collection");
        Ok(())
    }

    /// Read-only collection check: GET only, never creates.
    ///
    /// Returns `Ok(true)` when the collection exists and its vector size
    /// equals `vector_dim`, `Ok(false)` when it is missing (callers let
    /// searches degrade to empty), and errors on dimension drift.
    pub async fn verify_collection_dim(&self, vector_dim: usize) -> Result<bool> {
        let info_url = format!("{}/collections/{}", self.base_url, self.collection);
        let resp = self.http.get(&info_url).send().await?;
        if !resp.status().is_success() {
            return Ok(false);
        }
        let info: Value = resp.json().await?;
        verify_vectors_size(&info, &self.collection, vector_dim)?;
        Ok(true)
    }

    // ── Point upsert ─────────────────────────────────────────────────────────

    /// Upsert (insert-or-replace) a batch of chunks.
    pub async fn upsert_chunks(&self, chunks: Vec<QdrantChunk>) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }
        self.assert_writable()?;

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
        self.assert_writable()?;

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

        let envelope: QdrantEnvelope<Vec<QdrantSearchResult>> = resp
            .json()
            .await
            .context("Failed to decode Qdrant search response")?;

        Ok(envelope.result)
    }

    /// Delete all points belonging to a document (used on re-index or soft-delete).
    pub async fn delete_by_document(&self, doc_id: &str) -> Result<()> {
        self.assert_writable()?;

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

/// Validate an existing collection's configured vector size against the
/// expected dim. Collection-info JSON path (verified against Qdrant
/// 1.19.x): `result.config.params.vectors.size` for a single unnamed
/// vector; named vectors arrive as an object without `size`, which we
/// cannot validate generically — error rather than guess.
fn verify_vectors_size(info: &Value, collection: &str, expected: usize) -> Result<()> {
    let vectors = info.pointer("/result/config/params/vectors");
    let actual = vectors.and_then(|v| v.get("size")).and_then(|v| v.as_u64());
    match actual {
        Some(actual) if actual as usize == expected => Ok(()),
        Some(actual) => bail!(
            "Qdrant collection '{collection}' vector-dim drift: collection has {actual}, expected {expected}"
        ),
        None => bail!(
            "Qdrant collection '{collection}': cannot read config.params.vectors.size (named vectors or unexpected shape)"
        ),
    }
}

/// All Qdrant REST responses wrap their payload in `{"result": ..., "status": ...}`.
#[derive(Debug, Deserialize)]
struct QdrantEnvelope<T> {
    result: T,
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn collection_info(size: usize) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "result": {"config": {"params": {"vectors": {
                "size": size, "distance": "Cosine",
            }}}},
        }))
    }

    async fn put_counter(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.to_string() == "PUT")
            .count()
    }

    #[test]
    fn protected_legacy_covers_both_existing_collections() {
        assert!(is_protected_legacy("style_images"));
        assert!(is_protected_legacy("fashion_knowledge"));
        assert!(!is_protected_legacy("style_images_local_clipvitb32_512"));
    }

    #[tokio::test]
    async fn creates_collection_with_metadata_when_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/collections/c1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": true, "status": "ok",
            })))
            .mount(&server)
            .await;

        let store = QdrantStore::new(&server.uri(), "c1");
        store
            .ensure_collection(
                512,
                Some(serde_json::json!({
                    "vector_dim": 512,
                    "model": "clip-vit-base-patch32",
                    "provider": "local",
                })),
            )
            .await
            .expect("ensure creates");

        let requests = server.received_requests().await.unwrap();
        let put = requests
            .iter()
            .find(|r| r.method.to_string() == "PUT")
            .unwrap();
        let body: Value = serde_json::from_slice(&put.body).unwrap();
        assert_eq!(body["vectors"]["size"], 512);
        assert_eq!(body["vectors"]["distance"], "Cosine");
        assert_eq!(body["metadata"]["vector_dim"], 512);
        assert_eq!(body["metadata"]["model"], "clip-vit-base-patch32");
        assert_eq!(body["metadata"]["provider"], "local");
    }

    #[tokio::test]
    async fn existing_collection_passes_when_dims_match_and_makes_no_put() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(collection_info(512))
            .mount(&server)
            .await;

        let store = QdrantStore::new(&server.uri(), "c1");
        store
            .ensure_collection(512, None)
            .await
            .expect("existing collection, same dim");
        assert_eq!(put_counter(&server).await, 0);
    }

    #[tokio::test]
    async fn existing_collection_fails_on_dim_drift() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(collection_info(1024))
            .mount(&server)
            .await;

        let store = QdrantStore::new(&server.uri(), "c1");
        let err = store
            .ensure_collection(512, None)
            .await
            .expect_err("drift must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("1024") && msg.contains("512"), "{msg}");
        // And it must not try to recreate.
        assert_eq!(put_counter(&server).await, 0);
    }

    #[tokio::test]
    async fn verify_collection_dim_reports_missing_without_creating() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let store = QdrantStore::new_read_only(&server.uri(), "c1");
        let exists = store
            .verify_collection_dim(512)
            .await
            .expect("verify is hermetic");
        assert!(!exists);
        assert_eq!(put_counter(&server).await, 0);
    }

    #[tokio::test]
    async fn read_only_store_refuses_upsert_before_http() {
        // No server routes at all: the refusal must happen client-side.
        let server = MockServer::start().await;
        let store = QdrantStore::new_read_only(&server.uri(), "c1");
        let err = store
            .upsert_raw_points(vec![RawPoint {
                id: "1".into(),
                vector: vec![0.1; 4],
                payload: serde_json::json!({}),
            }])
            .await
            .expect_err("read-only refuses");
        assert!(format!("{err:#}").contains("bound read-only"), "{err:#}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            0,
            "no HTTP request may leave for a refused write"
        );
    }
}
