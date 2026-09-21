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

/// High-level service composing the CLIP client and the Qdrant store.
#[derive(Clone)]
pub struct ImageSearchService {
    clip: ClipClient,
    store: QdrantStore,
    vector_dim: usize,
}

impl ImageSearchService {
    /// Build from environment configuration.
    pub fn from_env() -> Self {
        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
        let vector_dim = std::env::var("CLIP_VECTOR_DIM")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(512); // CLIP ViT-B/32 → 512; ViT-L → 768

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

    /// Encode an image and run an ANN search scoped to org + dept.
    pub async fn search_similar(
        &self,
        image_bytes: &[u8],
        org_id: &str,
        dept_id: &str,
        limit: usize,
    ) -> Result<Vec<ImageHit>> {
        let vector = self.clip.encode_image(image_bytes).await?;
        let hits = self.store.search(&vector, org_id, dept_id, limit).await?;

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

        Ok(results)
    }
}

/// Convenience re-export for handler call sites.
pub type StyleId = Option<Uuid>;
