//! Images module — CLIP image encoding, indexing, and image search (T-019).
//!
//! Storage layout:
//!   - Qdrant collection `style_images`, cosine distance, one point per image
//!   - Payload: `{ image_path, style_id, org_id, dept_id }`
//!   - Tenant isolation: `must` filter on `org_id` AND `dept_id` at search time

pub mod clip;
pub mod indexer;

use anyhow::Result;
use uuid::Uuid;

use crate::rag::QdrantStore;

use clip::ClipClient;

/// A single image search hit (before style-name enrichment).
#[derive(Debug, Clone)]
pub struct ImageHit {
    pub image_path: String,
    pub style_id: Option<String>,
    pub score: f32,
}

/// Outcome of `search_similar`: hits plus which CLIP provider actually
/// served the query embedding (T-027 observability; surfaced to callers as
/// `provider_used`).
#[derive(Debug, Clone)]
pub struct SimilarSearchOutcome {
    pub hits: Vec<ImageHit>,
    /// Wire name from `clip::ClipProviderId::as_str()`.
    pub provider_used: String,
}

/// High-level service composing the CLIP client and the Qdrant store.
#[derive(Clone)]
pub struct ImageSearchService {
    clip: ClipClient,
    store: QdrantStore,
    vector_dim: usize,
}

/// Resolve the `style_images` collection vector dim at startup.
///
/// Priority (T-027): an explicit `CLIP_VECTOR_DIM` env value always wins —
/// existing deployments that set it in `.env` behave exactly as before.
/// Without it the dim follows the configured provider:
///   - DashScope `multimodal-embedding-v1` emits 1024 dims (the pre-T-027
///     default of 512 was wrong for it — the config pitfall this fixes);
///   - local CLIP ViT-B/32, Generic, and Hybrid (local is primary) are 512.
pub fn resolve_vector_dim(explicit: Option<usize>, provider: &str) -> usize {
    if let Some(d) = explicit {
        return d;
    }
    match provider {
        "dashscope" | "qwen" => 1024,
        _ => 512,
    }
}

impl ImageSearchService {
    /// Build from environment configuration.
    pub fn from_env() -> Self {
        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
        let vector_dim = resolve_vector_dim(
            std::env::var("CLIP_VECTOR_DIM")
                .ok()
                .and_then(|v| v.parse().ok()),
            &std::env::var("CLIP_PROVIDER").unwrap_or_default(),
        );

        Self {
            clip: ClipClient::from_env(),
            store: QdrantStore::new(&qdrant_url, "style_images"),
            vector_dim,
        }
    }

    /// Create the `style_images` collection on startup if missing.
    pub async fn ensure_collection(&self) -> Result<()> {
        self.store.ensure_collection(self.vector_dim).await
    }

    /// Access the CLIP client (used by the background indexing task).
    pub fn clip(&self) -> &ClipClient {
        &self.clip
    }

    /// Access the Qdrant store (used by the background indexing task).
    pub fn store(&self) -> &QdrantStore {
        &self.store
    }

    /// Whether a CLIP endpoint is configured.
    pub fn configured(&self) -> bool {
        self.clip.configured()
    }

    /// CLIP model name (for startup logging).
    pub fn model_name(&self) -> &str {
        self.clip.model_name()
    }

    /// Encode an image and run an ANN search scoped to org + dept. The
    /// returned outcome also reports which CLIP provider served the query
    /// embedding (`provider_used`), so callers can surface it in responses.
    pub async fn search_similar(
        &self,
        image_bytes: &[u8],
        org_id: &str,
        dept_id: &str,
        limit: usize,
    ) -> Result<SimilarSearchOutcome> {
        let embedding = self.clip.encode_image(image_bytes).await?;
        let hits = self
            .store
            .search(&embedding.vector, org_id, dept_id, limit)
            .await?;

        let results = hits
            .into_iter()
            .map(|h| {
                let p = &h.payload;
                ImageHit {
                    image_path: p
                        .get("image_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    style_id: p
                        .get("style_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    score: h.score,
                }
            })
            .collect();

        Ok(SimilarSearchOutcome {
            hits: results,
            provider_used: embedding.provider_str().to_string(),
        })
    }
}

/// Convenience re-export for handler call sites.
pub type StyleId = Option<Uuid>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_vector_dim_follows_provider() {
        // Local CLIP ViT-B/32 → 512.
        assert_eq!(resolve_vector_dim(None, ""), 512);
        assert_eq!(resolve_vector_dim(None, "generic"), 512);
        assert_eq!(resolve_vector_dim(None, "local"), 512);
        // Hybrid: local is the primary provider → 512.
        assert_eq!(resolve_vector_dim(None, "hybrid"), 512);
        // DashScope multimodal-embedding-v1 → 1024 (the corrected default).
        assert_eq!(resolve_vector_dim(None, "dashscope"), 1024);
        assert_eq!(resolve_vector_dim(None, "qwen"), 1024);
        // Unknown values keep the pre-T-027 512 behaviour.
        assert_eq!(resolve_vector_dim(None, "some-future-provider"), 512);
    }

    #[test]
    fn explicit_vector_dim_always_wins() {
        // Highest priority: explicit CLIP_VECTOR_DIM overrides every
        // provider default, including DashScope's 1024.
        assert_eq!(resolve_vector_dim(Some(768), "dashscope"), 768);
        assert_eq!(resolve_vector_dim(Some(512), "dashscope"), 512);
        assert_eq!(resolve_vector_dim(Some(1024), "local"), 1024);
        assert_eq!(resolve_vector_dim(Some(42), "hybrid"), 42);
    }
}
