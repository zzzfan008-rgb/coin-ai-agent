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

/// Legacy collections from the DashScope/DeepSeek era that T-027 tooling
/// must protect — increment 3 addendum: BOTH style and knowledge chains
/// now write dim+model-named collections:
///   - `style_images` (1024): read-only transitional fallback for the
///     style chain + source of the style migration;
///   - `fashion_knowledge` (1536 on this machine): read-only
///     transitional fallback for the document-RAG chain + source of the
///     knowledge migration.
/// Neither name is ever a write target — each may be a migration SOURCE
/// or a read-only fallback only.
///
/// Dim+model-named collections structurally can never match one of
/// these names — `style_collection_name`/`knowledge_collection_name`
/// output is pinned by never-protected tests.
///
/// Scope note (T-027 B phase, implemented): this list guards pipelines
/// (entry assertions), read-only routes, AND every `QdrantStore` write
/// entry — the five mutating methods hard-check it before ANY HTTP
/// request: `ensure_collection`, `upsert_chunks`, `upsert_raw_points`,
/// `upsert_scrolled_points`, `delete_by_document`. Read methods
/// (`scroll_points`, `search`, `points_count`, `verify_collection_dim`)
/// intentionally stay open on protected collections — migrations read
/// their source from them and the transitional fallback searches them.
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

/// One point read via the Qdrant scroll API. Unlike [`RawPoint`] the id is
/// kept as raw JSON (UUID string OR unsigned integer) so a migration can
/// write it back unchanged.
#[derive(Debug, Clone)]
pub struct ScrolledPoint {
    pub id: Value,
    pub vector: Vec<f32>,
    pub payload: Value,
}

/// Result of one scroll request: the page's points plus the offset to use
/// for the next page (`None` when this was the last page).
#[derive(Debug)]
pub struct ScrollPage {
    pub points: Vec<ScrolledPoint>,
    pub next_offset: Option<Value>,
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

    /// Fail before ANY HTTP request when the bound collection is a
    /// protected legacy name (`fashion_knowledge` / `style_images`).
    /// These names are read-only transitional fallbacks and migration
    /// SOURCES — never write targets. Distinct from `assert_writable`:
    /// a `new()`-bound (writable) store pointing at a protected name
    /// still must not write. Fail-closed: called before empty-batch
    /// early returns, so even an empty write attempt is a loud error
    /// (a silent Ok would be a broken gate).
    fn assert_not_protected_legacy(&self) -> Result<()> {
        if is_protected_legacy(&self.collection) {
            bail!(
                "write refused: Qdrant collection '{}' is a PROTECTED LEGACY collection \
                 (read-only fallback / migration source) — writes are forbidden",
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
        // B phase: protected legacy names must not even be probed — a
        // missing-collection PUT would recreate a forbidden target.
        // Fail before the GET.
        self.assert_not_protected_legacy()?;
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

    // ── Reads used by the migration / transitional search ───────────────────

    /// Read `points_count` for the bound collection via GET collection-info.
    ///
    /// Returns `Ok(None)` when the collection is missing, so callers can
    /// distinguish "not there yet" from "empty" only via the server (both
    /// lead to the same transition decision, so None and 0 are treated
    /// identically by the search layer).
    pub async fn points_count(&self) -> Result<Option<u64>> {
        let info_url = format!("{}/collections/{}", self.base_url, self.collection);
        let resp = self
            .http
            .get(&info_url)
            .send()
            .await
            .context("Qdrant collection-info request failed")?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let body: Value = resp.json().await.context("decoding collection-info")?;
        Ok(body
            .pointer("/result/points_count")
            .and_then(|v| v.as_u64()))
    }

    /// Scroll one page of points (POST /collections/{name}/points/scroll)
    /// with vectors and payloads included.
    ///
    /// This is a READ: it works on a read-only-bound store and never
    /// mutates anything. `offset` is the opaque value from a previous
    /// page's [`ScrollPage::next_offset`]; `None` starts at the beginning.
    pub async fn scroll_points(&self, offset: Option<Value>, limit: usize) -> Result<ScrollPage> {
        let url = format!(
            "{}/collections/{}/points/scroll",
            self.base_url, self.collection
        );
        let mut body = serde_json::json!({
            "limit": limit,
            "with_payload": true,
            "with_vector": true,
        });
        if let Some(offset) = offset {
            body["offset"] = offset;
        }

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Qdrant scroll request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("Qdrant scroll failed ({status}): {text}");
        }

        let envelope: ScrollEnvelope = resp.json().await.context("decoding scroll response")?;
        let result = envelope.result;
        let points = result
            .points
            .into_iter()
            .map(parse_scrolled_point)
            .collect::<Result<Vec<_>>>()?;
        // Qdrant 1.19 names this `next_page_offset`; older versions used
        // `next_offset`. Accept either.
        let next_offset = result.next_page_offset.or(result.next_offset);

        Ok(ScrollPage {
            points,
            next_offset,
        })
    }

    /// Upsert points with original (JSON) ids — the migration copy path.
    /// Same id replacement makes the copy idempotent.
    pub async fn upsert_scrolled_points(&self, points: Vec<ScrolledPoint>) -> Result<()> {
        // Fail-closed: protected check before the empty-batch early return.
        self.assert_not_protected_legacy()?;
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
            .context("Qdrant migration upsert request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("Qdrant migration upsert failed ({status}): {text}");
        }
        Ok(())
    }

    // ── Point upsert ─────────────────────────────────────────────────────────

    /// Upsert (insert-or-replace) a batch of chunks.
    pub async fn upsert_chunks(&self, chunks: Vec<QdrantChunk>) -> Result<()> {
        // Fail-closed: protected check before the empty-batch early return.
        self.assert_not_protected_legacy()?;
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
        // Fail-closed: protected check before the empty-batch early return.
        self.assert_not_protected_legacy()?;
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
        // Fail-closed: protected check before any HTTP request.
        self.assert_not_protected_legacy()?;
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

/// Scroll envelope — fields use Qdrant 1.19 names; `next_offset` is the
/// pre-1.19 alias, kept for compatibility.
#[derive(Debug, Deserialize)]
struct ScrollEnvelope {
    result: ScrollResult,
}

#[derive(Debug, Deserialize)]
struct ScrollResult {
    points: Vec<Value>,
    #[serde(rename = "next_page_offset")]
    next_page_offset: Option<Value>,
    #[serde(rename = "next_offset")]
    next_offset: Option<Value>,
}

/// Convert one raw scroll point JSON value into a [`ScrolledPoint`].
///
/// Qdrant returns the vector either as a bare array (single unnamed
/// vector) or an object mapping vector names to arrays; accept both.
fn parse_scrolled_point(point: Value) -> Result<ScrolledPoint> {
    let id = point
        .get("id")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("scroll point missing id: {point}"))?;
    let vector_value = point
        .get("vector")
        .ok_or_else(|| anyhow::anyhow!("scroll point missing vector (with_vector must be true)"))?;
    let vector_array = match vector_value {
        Value::Array(array) => array,
        Value::Object(map) => map
            .values()
            .next()
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("named-vector scroll point has no usable vector"))?,
        _ => anyhow::bail!("unexpected scroll vector shape: {vector_value}"),
    };
    let vector = vector_array
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();
    let payload = point.get("payload").cloned().unwrap_or(Value::Null);
    Ok(ScrolledPoint {
        id,
        vector,
        payload,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::QdrantPayload;
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

    // ── Migration read methods (increment 3 phase C) ─────────────────────

    #[tokio::test]
    async fn scroll_points_sends_expected_body_and_parses_page() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/collections/c1/points/scroll"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {
                    "points": [{
                        "id": "uuid-1",
                        "vector": [0.1, 0.2, 0.3],
                        "payload": {"image_path": "a.jpg"},
                    }],
                    // Pre-1.19 offset field name.
                    "next_offset": 7,
                },
                "status": "ok",
            })))
            .mount(&server)
            .await;

        // Scrolling is a read: a read-only-bound store may scroll.
        let store = QdrantStore::new_read_only(&server.uri(), "c1");
        let page = store
            .scroll_points(Some(serde_json::json!(3)), 100)
            .await
            .expect("scroll");

        // Request body pins the scroll contract.
        let req = &server.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["limit"], 100);
        assert_eq!(body["offset"], 3);
        assert_eq!(body["with_payload"], true);
        assert_eq!(body["with_vector"], true);

        assert_eq!(page.points.len(), 1);
        assert_eq!(page.points[0].id, "uuid-1");
        assert_eq!(page.points[0].vector, vec![0.1, 0.2, 0.3]);
        assert_eq!(page.points[0].payload["image_path"], "a.jpg");
        assert_eq!(page.next_offset, Some(serde_json::json!(7)));
    }

    #[tokio::test]
    async fn scroll_points_parses_1_19_named_vector_and_page_offset() {
        // Qdrant 1.19: next_page_offset field, and a named-vector object
        // form for the point.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/collections/c2/points/scroll"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {
                    "points": [{
                        "id": 9,
                        "vector": {"": [0.5, 0.6]},
                        "payload": null,
                    }],
                    "next_page_offset": "token-9",
                },
            })))
            .mount(&server)
            .await;

        let store = QdrantStore::new(&server.uri(), "c2");
        let page = store.scroll_points(None, 50).await.expect("scroll");
        assert_eq!(page.points[0].id, 9);
        assert_eq!(page.points[0].vector, vec![0.5, 0.6]);
        assert_eq!(page.next_offset, Some(serde_json::json!("token-9")));
    }

    #[tokio::test]
    async fn points_count_reports_count_and_none_when_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/collections/c3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"points_count": 2},
            })))
            .mount(&server)
            .await;

        let store = QdrantStore::new(&server.uri(), "c3");
        assert_eq!(store.points_count().await.unwrap(), Some(2));

        // A second collection that is missing → None (no creation).
        let missing_store = QdrantStore::new_read_only(&server.uri(), "missing");
        assert_eq!(missing_store.points_count().await.unwrap(), None);
        let puts = put_counter(&server).await;
        assert_eq!(puts, 0, "points_count never creates");
    }

    #[tokio::test]
    async fn upsert_scrolled_points_preserves_ids_and_respects_readonly() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/collections/c4/points"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"status": "completed"},
            })))
            .mount(&server)
            .await;

        let store = QdrantStore::new(&server.uri(), "c4");
        store
            .upsert_scrolled_points(vec![ScrolledPoint {
                id: serde_json::json!(42),
                vector: vec![0.1; 2],
                payload: serde_json::json!({"k": "v"}),
            }])
            .await
            .expect("migration upsert");

        let put = &server.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&put.body).unwrap();
        assert_eq!(body["points"][0]["id"], 42, "integer id preserved");
        assert_eq!(body["points"][0]["payload"]["k"], "v");

        // A read-only target store refuses before HTTP.
        let ro_server = MockServer::start().await;
        let ro = QdrantStore::new_read_only(&ro_server.uri(), "c4");
        let err = ro
            .upsert_scrolled_points(vec![ScrolledPoint {
                id: serde_json::json!(1),
                vector: vec![0.1],
                payload: Value::Null,
            }])
            .await
            .expect_err("read-only target refuses");
        assert!(format!("{err:#}").contains("bound read-only"), "{err:#}");
        assert_eq!(ro_server.received_requests().await.unwrap().len(), 0);
    }

    // ── B phase: protected legacy write hard-lock ─────────────────────────

    fn sample_chunk() -> QdrantChunk {
        QdrantChunk {
            id: "00000000-0000-0000-0000-000000000001".into(),
            vector: vec![0.1; 4],
            payload: QdrantPayload {
                text: "t".into(),
                doc_id: "d".into(),
                dept_id: "x".into(),
                org_id: "o".into(),
                title: "t".into(),
                chunk_index: 0,
            },
        }
    }

    fn sample_raw_point() -> RawPoint {
        RawPoint {
            id: "1".into(),
            vector: vec![0.1; 4],
            payload: serde_json::json!({}),
        }
    }

    fn sample_scrolled_point() -> ScrolledPoint {
        ScrolledPoint {
            id: serde_json::json!("uuid-1"),
            vector: vec![0.1; 4],
            payload: serde_json::json!({}),
        }
    }

    /// Every protected name must be refused by every mutating entry,
    /// before ANY HTTP request (server receives nothing at all).
    #[tokio::test]
    async fn protected_legacy_refuses_all_five_write_entries_before_http() {
        for name in PROTECTED_LEGACY_COLLECTIONS {
            // 1. ensure_collection
            let server = MockServer::start().await;
            let store = QdrantStore::new(&server.uri(), name);
            let err = store
                .ensure_collection(4, None)
                .await
                .expect_err("protected ensure must fail");
            assert!(
                format!("{err:#}").contains("PROTECTED LEGACY"),
                "{name}: {err:#}"
            );
            assert_eq!(server.received_requests().await.unwrap().len(), 0, "{name}");

            // 2. upsert_chunks
            let server = MockServer::start().await;
            let store = QdrantStore::new(&server.uri(), name);
            let err = store
                .upsert_chunks(vec![sample_chunk()])
                .await
                .expect_err("protected upsert_chunks must fail");
            assert!(
                format!("{err:#}").contains("PROTECTED LEGACY"),
                "{name}: {err:#}"
            );
            assert_eq!(server.received_requests().await.unwrap().len(), 0, "{name}");

            // 3. upsert_raw_points
            let server = MockServer::start().await;
            let store = QdrantStore::new(&server.uri(), name);
            let err = store
                .upsert_raw_points(vec![sample_raw_point()])
                .await
                .expect_err("protected upsert_raw_points must fail");
            assert!(
                format!("{err:#}").contains("PROTECTED LEGACY"),
                "{name}: {err:#}"
            );
            assert_eq!(server.received_requests().await.unwrap().len(), 0, "{name}");

            // 4. upsert_scrolled_points
            let server = MockServer::start().await;
            let store = QdrantStore::new(&server.uri(), name);
            let err = store
                .upsert_scrolled_points(vec![sample_scrolled_point()])
                .await
                .expect_err("protected upsert_scrolled_points must fail");
            assert!(
                format!("{err:#}").contains("PROTECTED LEGACY"),
                "{name}: {err:#}"
            );
            assert_eq!(server.received_requests().await.unwrap().len(), 0, "{name}");

            // 5. delete_by_document
            let server = MockServer::start().await;
            let store = QdrantStore::new(&server.uri(), name);
            let err = store
                .delete_by_document("doc-1")
                .await
                .expect_err("protected delete must fail");
            assert!(
                format!("{err:#}").contains("PROTECTED LEGACY"),
                "{name}: {err:#}"
            );
            assert_eq!(server.received_requests().await.unwrap().len(), 0, "{name}");
        }
    }

    /// Fail-closed ordering: an EMPTY batch to a protected collection must
    /// still be a loud error — the protected check runs before the
    /// empty-batch early return, otherwise a silent Ok would void the gate.
    #[tokio::test]
    async fn protected_legacy_empty_batch_still_fails() {
        for name in PROTECTED_LEGACY_COLLECTIONS {
            let server = MockServer::start().await;
            let store = QdrantStore::new(&server.uri(), name);
            let err = store
                .upsert_chunks(Vec::new())
                .await
                .expect_err("empty upsert_chunks on protected must fail");
            assert!(
                format!("{err:#}").contains("PROTECTED LEGACY"),
                "{name}: {err:#}"
            );

            let err = store
                .upsert_raw_points(Vec::new())
                .await
                .expect_err("empty upsert_raw_points on protected must fail");
            assert!(
                format!("{err:#}").contains("PROTECTED LEGACY"),
                "{name}: {err:#}"
            );

            let err = store
                .upsert_scrolled_points(Vec::new())
                .await
                .expect_err("empty upsert_scrolled_points on protected must fail");
            assert!(
                format!("{err:#}").contains("PROTECTED LEGACY"),
                "{name}: {err:#}"
            );
        }
    }

    /// Read paths stay open on protected collections: migrations read their
    /// source from them and the transitional fallback searches them.
    #[tokio::test]
    async fn protected_legacy_read_paths_stay_open() {
        let name = "fashion_knowledge";

        // scroll_points
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/collections/{name}/points/scroll")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"points": [{
                    "id": "p1", "vector": [0.1, 0.2], "payload": null,
                }], "next_page_offset": null},
            })))
            .mount(&server)
            .await;
        let store = QdrantStore::new(&server.uri(), name);
        let page = store
            .scroll_points(None, 10)
            .await
            .expect("scroll readable");
        assert_eq!(page.points.len(), 1);

        // search
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/collections/{name}/points/search")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": [],
            })))
            .mount(&server)
            .await;
        let store = QdrantStore::new(&server.uri(), name);
        let hits = store
            .search(&[0.1; 4], "org", "dept", 5)
            .await
            .expect("search readable");
        assert!(hits.is_empty());

        // points_count
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/collections/{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"points_count": 7},
            })))
            .mount(&server)
            .await;
        let store = QdrantStore::new(&server.uri(), name);
        assert_eq!(store.points_count().await.unwrap(), Some(7));

        // verify_collection_dim
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/collections/{name}")))
            .respond_with(collection_info(4))
            .mount(&server)
            .await;
        let store = QdrantStore::new_read_only(&server.uri(), name);
        assert_eq!(
            store
                .verify_collection_dim(4)
                .await
                .expect("verify readable"),
            true
        );
    }

    /// Non-protected empty batches still short-circuit to Ok (behaviour
    /// unchanged for writable names).
    #[tokio::test]
    async fn non_protected_empty_batch_is_silent_ok() {
        let server = MockServer::start().await;
        let store = QdrantStore::new(&server.uri(), "c1");
        store.upsert_chunks(Vec::new()).await.expect("empty ok");
        store.upsert_raw_points(Vec::new()).await.expect("empty ok");
        store
            .upsert_scrolled_points(Vec::new())
            .await
            .expect("empty ok");
        assert_eq!(server.received_requests().await.unwrap().len(), 0);
    }
}
