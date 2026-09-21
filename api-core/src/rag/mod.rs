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
//!     → EmbeddingService           [DeepSeek /embeddings]
//!     → QdrantStore::upsert()      [collection: fashion_knowledge]
//!     → UPDATE doc status → ready
//!
//! Query flow:
//!   knowledge_search()
//!     → embed query
//!     → QdrantStore::search()      [org_id + dept_id payload filter]
//!     → return top-K chunks
//! ```
//!
//! Tenant isolation: a single physical Qdrant collection holds every
//! organisation's chunks; each query carries a `must` filter on both
//! `org_id` and `dept_id`.

mod embedding;
mod indexer;
mod parser;
mod qdrant;

pub use embedding::EmbeddingService;
pub use indexer::DocumentIndexer;
pub use parser::DocumentParser;
pub use qdrant::QdrantStore;

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
    pub deepseek_api_key: String,
    /// OpenAI-compatible base URL (without trailing `/embeddings`).
    pub deepseek_base_url: String,
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
            deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
            deepseek_base_url: std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com/v1".into()),
            embedding_model: std::env::var("EMBEDDING_MODEL")
                .unwrap_or_else(|_| "deepseek-embedding".into()),
            embedding_dim: 1536,
            chunk_size: 500,
            chunk_overlap: 50,
            upsert_batch_size: 32,
            default_top_k: 5,
        }
    }
}

impl RagConfig {
    /// Build RAG config from the global [`AppConfig`](crate::config::AppConfig).
    pub fn from_app_config(cfg: &crate::config::AppConfig) -> Self {
        Self {
            qdrant_url: cfg.qdrant_url.clone(),
            qdrant_collection: "fashion_knowledge".into(),
            deepseek_api_key: cfg.deepseek_api_key.clone(),
            deepseek_base_url: cfg.deepseek_base_url.clone(),
            embedding_model: "deepseek-embedding".into(),
            embedding_dim: 1536,
            chunk_size: 500,
            chunk_overlap: 50,
            upsert_batch_size: 32,
            default_top_k: 5,
        }
    }
}

// ── RagRetriever facade ──────────────────────────────────────────────────────

/// High-level RAG interface used by both HTTP handlers and the agent engine.
/// Composes [`EmbeddingService`] and [`QdrantStore`].
#[derive(Clone)]
pub struct RagRetriever {
    embedding: EmbeddingService,
    store: QdrantStore,
    config: RagConfig,
}

impl RagRetriever {
    /// Build all sub-services from config.
    pub fn new(config: RagConfig) -> Self {
        let embedding = EmbeddingService::new(
            config.deepseek_api_key.clone(),
            config.deepseek_base_url.clone(),
            config.embedding_model.clone(),
        );
        let store = QdrantStore::new(&config.qdrant_url, &config.qdrant_collection);

        Self { embedding, store, config }
    }

    /// Embed a single piece of text.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        self.embedding.embed_text(text).await
    }

    /// Create the Qdrant collection on startup if missing.
    pub async fn ensure_collection(&self) -> Result<()> {
        self.store.ensure_collection(self.config.embedding_dim).await
    }

    /// Upsert a batch of embedded chunks.
    pub async fn upsert_chunks(&self, chunks: Vec<QdrantChunk>) -> Result<()> {
        self.store.upsert_chunks(chunks).await
    }

    /// Raw ANN search returning full Qdrant hits.
    pub async fn search(
        &self,
        vector: &[f32],
        org_id: &str,
        dept_id: &str,
        limit: usize,
    ) -> Result<Vec<QdrantSearchResult>> {
        self.store.search(vector, org_id, dept_id, limit).await
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

    /// Delete all Qdrant points belonging to a document.
    pub async fn delete_document_points(&self, doc_id: &str) -> Result<()> {
        self.store.delete_by_document(doc_id).await
    }
}
