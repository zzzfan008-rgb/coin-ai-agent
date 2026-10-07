//! RAG module — document parsing, embedding, and vector-index pipeline.
//!
//! # Architecture
//!
//! ```text
//! Upload flow (HTTP):
//!   upload_knowledge_document()
//!     → INSERT knowledge_documents (status=pending)
//!     → tokio::spawn(index_document)
//!
//! Index flow (background):
//!   index_document()
//!     → DocumentParser::parse()    [pdf | docx | txt | md]
//!     → chunk_text()               [500 tokens, 50 overlap]
//!     → EmbeddingService           [text-embedding-v3 via qwen-compatible endpoint]
//!     → QdrantStore::upsert()      [writes: fashion_knowledge_{provider}_{model}_{dim}]
//!     → UPDATE doc status → ready
//!
//! Query flow:
//!   knowledge_search()
//!     → embed query
//!     → QdrantStore::search()      [new collection preferred; read-only
//!                                  legacy fashion_knowledge fallback]
//!     → return top-K chunks
//! ```
//!
//! Transitional layout (T-027 increment 3 addendum): writes go to the
//! dim+model-named collection; the legacy collection is bound read-only
//! as a fail-safe fallback until migration completes. Tenant isolation:
//! a single physical Qdrant collection holds every organisation's
//! chunks; each query carries a `must` filter on both `org_id` and `dept_id`.

mod embedding;
mod indexer;
mod parser;
mod qdrant;

pub use embedding::EmbeddingService;
pub use indexer::DocumentIndexer;
pub use parser::DocumentParser;
pub use qdrant::is_protected_legacy;
pub use qdrant::QdrantStore;
pub use qdrant::RawPoint;

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ── Shared types ──────────────────────────────────────────────────────────────

/// A point to be stored in Qdrant.
#[derive(Debug, Clone, Serialize)]
pub struct QdrantChunk {
    /// Must be a valid UUID string (Qdrant accepts UUID or unsigned int).
    pub id: String,
    pub vector: Vec<f32>,
    pub payload: QdrantPayload,
}

/// Payload attached to every chunk point.
#[derive(Debug, Clone, Serialize)]
pub struct QdrantPayload {
    pub text: String,
    pub doc_id: String,
    pub dept_id: String,
    pub org_id: String,
    pub title: String,
    pub chunk_index: i32,
}

/// A single ANN hit returned by Qdrant.
#[derive(Debug, Clone, Deserialize)]
pub struct QdrantSearchResult {
    /// UUID string or unsigned integer — kept as raw JSON for flexibility.
    pub id: serde_json::Value,
    pub score: f32,
    pub payload: serde_json::Value,
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Tunables for the chunk → embed → index pipeline.
#[derive(Debug, Clone)]
pub struct RagConfig {
    pub qdrant_url: String,
    pub qdrant_collection: String,
    pub embedding_api_key: String,
    /// OpenAI-compatible base URL (without trailing `/embeddings`).
    pub embedding_base_url: String,
    pub embedding_model: String,
    /// Expected embedding dimension.
    pub embedding_dim: usize,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    /// Max points per Qdrant upsert batch.
    pub upsert_batch_size: usize,
    /// Default number of search hits.
    pub default_top_k: usize,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            qdrant_url: std::env::var("QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6333".into()),
            qdrant_collection: std::env::var("QDRANT_COLLECTION")
                .unwrap_or_else(|_| "fashion_knowledge".into()),
            embedding_api_key: first_present(&["EMBEDDING_API_KEY", "DASHSCOPE_API_KEY"]),
            embedding_base_url: std::env::var("EMBEDDING_BASE_URL")
                .unwrap_or_else(|_| "https://maas.qianwenaiapi.com/compatible-mode/v1".into()),
            embedding_model: std::env::var("EMBEDDING_MODEL")
                .unwrap_or_else(|_| "text-embedding-v3".into()),
            embedding_dim: std::env::var("EMBEDDING_DIM")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1024),
            chunk_size: 500,
            chunk_overlap: 50,
            upsert_batch_size: 32,
            default_top_k: 5,
        }
    }
}

fn first_present(keys: &[&str]) -> String {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    String::new()
}

// ── Knowledge collection naming (T-027 increment 3 addendum) ────────────────

/// Short, collection-name-safe identifier for known text-embedding
/// models. Unknown models fall back to their ASCII-alphanumeric
/// characters lowercased.
fn knowledge_model_tag(model: &str) -> String {
    match model {
        "text-embedding-v1" => "tev1".into(),
        "text-embedding-v2" => "tev2".into(),
        "text-embedding-v3" => "tev3".into(),
        "deepseek-embedding" => "dsembed".into(),
        other => other
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase(),
    }
}

/// Name of a knowledge (document-RAG) collection:
/// `fashion_knowledge_{provider}_{model-tag}_{dim}`.
///
/// Same dim+model convention as the style-image collections, with the
/// knowledge prefix. The dim is the trailing token for the same reasons
/// (sort order, unmistakeable embedding space).
pub fn knowledge_collection_name(provider: &str, model: &str, dim: usize) -> String {
    format!(
        "fashion_knowledge_{provider}_{}_{dim}",
        knowledge_model_tag(model)
    )
}

impl RagConfig {
    /// Build RAG config from the global [`AppConfig`](crate::config::AppConfig).
    /// Embedding is provider-agnostic: defaults to the configured Qwen/DashScope
    /// endpoint and can be overridden with EMBEDDING_* environment variables.
    pub fn from_app_config(cfg: &crate::config::AppConfig) -> Self {
        let env_default = Self::default();
        Self {
            qdrant_url: cfg.qdrant_url.clone(),
            qdrant_collection: "fashion_knowledge".into(),
            embedding_api_key: if env_default.embedding_api_key.is_empty() {
                cfg.qwen_api_key.clone()
            } else {
                env_default.embedding_api_key
            },
            embedding_base_url: if std::env::var("EMBEDDING_BASE_URL").is_ok() {
                env_default.embedding_base_url
            } else {
                cfg.qwen_base_url.clone()
            },
            embedding_model: if std::env::var("EMBEDDING_MODEL").is_ok() {
                env_default.embedding_model
            } else {
                "text-embedding-v3".into()
            },
            embedding_dim: if std::env::var("EMBEDDING_DIM").is_ok() {
                env_default.embedding_dim
            } else {
                1024
            },
            chunk_size: 500,
            chunk_overlap: 50,
            upsert_batch_size: 32,
            default_top_k: 5,
        }
    }
}

// ── RagRetriever facade ──────────────────────────────────────────────────────

/// High-level RAG interface used by both HTTP handlers and the agent engine.
/// Composes [`EmbeddingService`] and two [`QdrantStore`]s:
///   - `named_store` is bound to the dim+model-named collection (all
///     writes: ensure/upsert/delete);
///   - `legacy_store` is bound read-only to the legacy
///     `config.qdrant_collection` (fashion_knowledge) and serves searches
///     only as the transitional fail-safe fallback while the named
///     collection is missing or empty.
#[derive(Clone)]
pub struct RagRetriever {
    embedding: EmbeddingService,
    named_store: QdrantStore,
    named_collection: String,
    legacy_store: QdrantStore,
    config: RagConfig,
}

impl RagRetriever {
    /// Build all sub-services from config.
    pub fn new(config: RagConfig) -> Self {
        let embedding = EmbeddingService::new(
            config.embedding_api_key.clone(),
            config.embedding_base_url.clone(),
            config.embedding_model.clone(),
            config.embedding_dim,
        );
        // Name the writable collection from the embedding chain's actual
        // identity (endpoint-derived provider + effective model + dim).
        let (provider, model) = embedding.identity();
        let named_collection = knowledge_collection_name(provider, model, config.embedding_dim);
        let named_store = QdrantStore::new(&config.qdrant_url, &named_collection);
        // Legacy collection: read-only binding, fallback only.
        let legacy_store =
            QdrantStore::new_read_only(&config.qdrant_url, &config.qdrant_collection);

        Self {
            embedding,
            named_store,
            named_collection,
            legacy_store,
            config,
        }
    }

    /// Embed a single piece of text.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        self.embedding.embed_text(text).await
    }

    /// Ensure the writable named collection exists with dim+model
    /// metadata; verify the read-only legacy route without creating or
    /// touching it.
    pub async fn ensure_collection(&self) -> Result<()> {
        let (provider, identity_model) = self.embedding.identity();
        let metadata = serde_json::json!({
            "vector_dim": self.config.embedding_dim,
            "model": identity_model,
            "provider": provider,
            "created_by": "api-core",
            "task": "T-027",
        });
        self.named_store
            .ensure_collection(self.config.embedding_dim, Some(metadata))
            .await?;
        match self
            .legacy_store
            .verify_collection_dim(self.config.embedding_dim)
            .await?
        {
            true => {}
            false => tracing::debug!(
                collection = %self.config.qdrant_collection,
                dim = self.config.embedding_dim,
                "legacy knowledge collection missing — fallback searches return empty until it exists"
            ),
        }
        Ok(())
    }

    /// Resolve the effective READ store with the transitional fail-safe
    /// semantics: named collection present with points_count > 0 →
    /// named; missing (None) or empty → read-only legacy store.
    async fn read_store(&self) -> Result<&QdrantStore> {
        match self.named_store.points_count().await? {
            Some(count) if count > 0 => Ok(&self.named_store),
            _ => Ok(&self.legacy_store),
        }
    }

    /// Upsert a batch of embedded chunks into the named collection.
    pub async fn upsert_chunks(&self, chunks: Vec<QdrantChunk>) -> Result<()> {
        self.named_store.upsert_chunks(chunks).await
    }

    /// Raw ANN search returning full Qdrant hits (new collection
    /// preferred, read-only legacy fallback).
    pub async fn search(
        &self,
        vector: &[f32],
        org_id: &str,
        dept_id: &str,
        limit: usize,
    ) -> Result<Vec<QdrantSearchResult>> {
        let store = self.read_store().await?;
        store.search(vector, org_id, dept_id, limit).await
    }

    /// Delete points of a document from the named collection (soft-delete
    /// path). Scope note: points existing ONLY in the legacy collection
    /// are not removed by this path — the migration must run first, same
    /// as the style chain; legacy stays read-only.
    pub async fn delete_document_vectors(&self, doc_id: &str) -> Result<()> {
        self.named_store.delete_by_document(doc_id).await
    }

    /// Convenience method: embed the query, search, and return concatenated
    /// text suitable for injection into an LLM prompt.
    ///
    /// Returns an empty string when no relevant chunks are found.
    pub async fn retrieve(
        &self,
        query: &str,
        org_id: &str,
        dept_id: &str,
        limit: usize,
    ) -> Result<String> {
        let vector = self.embed_text(query).await?;
        let hits = self.search(&vector, org_id, dept_id, limit).await?;

        let context = hits
            .iter()
            .enumerate()
            .filter_map(|(i, hit)| {
                hit.payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|text| format!("[{}] {text}", i + 1))
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(context)
    }

    /// Access the underlying configuration (used by the indexer).
    pub fn config(&self) -> &RagConfig {
        &self.config
    }

    /// Bound writable named collection name (observability/tests).
    pub fn named_collection(&self) -> &str {
        &self.named_collection
    }

    /// Delete all Qdrant points belonging to a document.
    pub async fn delete_document_points(&self, doc_id: &str) -> Result<()> {
        self.named_store.delete_by_document(doc_id).await
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const NAMED_COLLECTION: &str = "fashion_knowledge_generic_tev3_1536";
    const LEGACY_COLLECTION: &str = "fashion_knowledge";

    // ── Naming ─────────────────────────────────────────────────────────────

    #[test]
    fn knowledge_collection_names_encode_provider_model_and_dim() {
        assert_eq!(
            knowledge_collection_name("dashscope", "text-embedding-v3", 1536),
            "fashion_knowledge_dashscope_tev3_1536"
        );
        assert_eq!(
            knowledge_collection_name("deepseek", "deepseek-embedding", 1536),
            "fashion_knowledge_deepseek_dsembed_1536"
        );
        // Unknown models sanitise to ascii-alphanumeric lowercase.
        assert_eq!(
            knowledge_collection_name("generic", "Future-Model/2", 256),
            "fashion_knowledge_generic_futuremodel2_256"
        );
    }

    #[test]
    fn generated_knowledge_collection_names_are_never_protected_legacy() {
        for (provider, model, dim) in [
            ("dashscope", "text-embedding-v3", 1536),
            ("deepseek", "deepseek-embedding", 1536),
            ("generic", "", 0),
        ] {
            let name = knowledge_collection_name(provider, model, dim);
            assert!(
                !is_protected_legacy(&name),
                "{name} must not be a protected legacy name"
            );
        }
    }

    // ── Embedding identity ─────────────────────────────────────────────────

    #[test]
    fn embedding_identity_follows_endpoint_host() {
        let cases = [
            (
                "https://maas.qianwenaiapi.com/compatible-mode/v1",
                "dashscope",
            ),
            ("https://api.deepseek.com/v1", "deepseek"),
            ("http://127.0.0.1:18401", "generic"),
        ];
        for (base, provider) in cases {
            let svc = EmbeddingService::new("key", base, "text-embedding-v3", 1536);
            let (got_provider, model) = svc.identity();
            assert_eq!(got_provider, provider, "{base}");
            assert_eq!(model, "text-embedding-v3");
        }
    }

    // ── RagRetriever construction ──────────────────────────────────────────

    fn config_for(url: &str) -> RagConfig {
        let mut config = RagConfig::default();
        config.qdrant_url = url.into();
        config.embedding_dim = 1536;
        config.embedding_base_url = "http://127.0.0.1:18401".into();
        config.embedding_model = "text-embedding-v3".into();
        config
    }

    #[test]
    fn new_binds_named_collection_from_embedding_identity() {
        let retriever = RagRetriever::new(config_for("http://127.0.0.1:16333"));
        assert_eq!(retriever.named_collection(), NAMED_COLLECTION);
    }

    fn collection_info(count: u64) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "result": {"points_count": count},
        }))
    }

    fn knowledge_hit(text: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "result": [{
                "id": "p1", "score": 0.8,
                "payload": {"text": text, "doc_id": "d1", "title": "t", "chunk_index": 0},
            }]
        }))
    }

    // ── Transitional search: two states ────────────────────────────────────

    #[tokio::test]
    async fn search_prefers_named_collection_when_nonempty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/collections/{NAMED_COLLECTION}")))
            .respond_with(collection_info(7))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/collections/{NAMED_COLLECTION}/points/search"
            )))
            .respond_with(knowledge_hit("from-new-collection"))
            .mount(&server)
            .await;

        let retriever = RagRetriever::new(config_for(&server.uri()));
        let hits = retriever
            .search(&[0.1; 1536], "org", "dept", 5)
            .await
            .expect("named search");
        assert_eq!(hits[0].payload["text"], "from-new-collection");

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2, "points-count GET + named search POST");
        assert!(seen[0]
            .url
            .path()
            .ends_with(&format!("/collections/{NAMED_COLLECTION}")));
        assert!(seen[1]
            .url
            .path()
            .ends_with(&format!("/collections/{NAMED_COLLECTION}/points/search")));
        assert!(seen
            .iter()
            .all(|r| !r.url.path().contains("/collections/fashion_knowledge/")));
    }

    #[tokio::test]
    async fn search_falls_back_to_legacy_when_named_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/collections/{NAMED_COLLECTION}")))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/collections/{LEGACY_COLLECTION}/points/search"
            )))
            .respond_with(knowledge_hit("from-legacy-collection"))
            .mount(&server)
            .await;

        let retriever = RagRetriever::new(config_for(&server.uri()));
        let hits = retriever
            .search(&[0.1; 1536], "org", "dept", 5)
            .await
            .expect("fail-safe fallback search");
        assert_eq!(hits[0].payload["text"], "from-legacy-collection");

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen[1]
            .url
            .path()
            .ends_with("/collections/fashion_knowledge/points/search"));
    }

    #[tokio::test]
    async fn search_falls_back_to_legacy_when_named_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/collections/{NAMED_COLLECTION}")))
            .respond_with(collection_info(0))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/collections/{LEGACY_COLLECTION}/points/search"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": []
            })))
            .mount(&server)
            .await;

        let retriever = RagRetriever::new(config_for(&server.uri()));
        let hits = retriever
            .search(&[0.1; 1536], "org", "dept", 5)
            .await
            .expect("empty named → legacy route");
        assert!(hits.is_empty());

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen[1]
            .url
            .path()
            .ends_with("/collections/fashion_knowledge/points/search"));
    }

    // ── Write routing ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn upsert_chunks_routes_to_named_collection() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path(format!("/collections/{NAMED_COLLECTION}/points")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"status": "completed"},
            })))
            .mount(&server)
            .await;

        let retriever = RagRetriever::new(config_for(&server.uri()));
        retriever
            .upsert_chunks(vec![QdrantChunk {
                id: "p1".into(),
                vector: vec![0.1; 1536],
                payload: QdrantPayload {
                    text: "x".into(),
                    doc_id: "d1".into(),
                    dept_id: "dept".into(),
                    org_id: "org".into(),
                    title: "t".into(),
                    chunk_index: 0,
                },
            }])
            .await
            .expect("named upsert");

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0]
            .url
            .path()
            .ends_with("/collections/fashion_knowledge_generic_tev3_1536/points"));
    }

    #[tokio::test]
    async fn delete_routes_to_named_collection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/collections/{NAMED_COLLECTION}/points/delete"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"status": "completed"},
            })))
            .mount(&server)
            .await;

        let retriever = RagRetriever::new(config_for(&server.uri()));
        retriever
            .delete_document_vectors("d1")
            .await
            .expect("named delete");

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0]
            .url
            .path()
            .ends_with("/collections/fashion_knowledge_generic_tev3_1536/points/delete"));
    }

    // ── Ensure ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn ensure_creates_named_with_metadata_and_verifies_legacy_readonly() {
        let server = MockServer::start().await;
        // Named: missing → create.
        Mock::given(method("GET"))
            .and(path(format!("/collections/{NAMED_COLLECTION}")))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(format!("/collections/{NAMED_COLLECTION}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": true, "status": "ok",
            })))
            .mount(&server)
            .await;
        // Legacy: also missing → verify returns false, no create.
        Mock::given(method("GET"))
            .and(path(format!("/collections/{LEGACY_COLLECTION}")))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let retriever = RagRetriever::new(config_for(&server.uri()));
        retriever
            .ensure_collection()
            .await
            .expect("ensure named; legacy missing is tolerated");

        let puts: Vec<_> = server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.to_string() == "PUT")
            .collect();
        assert_eq!(puts.len(), 1, "only the named collection is created");
        let body: serde_json::Value = serde_json::from_slice(&puts[0].body).unwrap();
        assert_eq!(body["vectors"]["size"], 1536);
        assert_eq!(body["metadata"]["provider"], "generic");
        assert_eq!(body["metadata"]["model"], "text-embedding-v3");
        assert_eq!(body["metadata"]["vector_dim"], 1536);
    }
}
