//! Image indexing pipeline — persists uploaded images and upserts CLIP
//! vectors into the dim-routed Qdrant collections.
//!
//! ```text
//! upload (HTTP handler)
//!   → write bytes to uploads/style-images/{org_id}/{uuid}.{ext}  (MinIO later)
//!   → INSERT style_images
//!   → [spawned] ClipClient::encode_image
//!   → ImageSearchService::publish_raw_point
//!       routed by the embedding's actual dim to the matching collection
//!       payload = { image_path, style_id, org_id, dept_id }
//! ```

use anyhow::Result;
use uuid::Uuid;

use crate::rag::RawPoint;

use super::ImageSearchService;

/// Runs the store → encode → upsert pipeline for one image.
pub struct ImageIndexer;

impl ImageIndexer {
    /// Persist raw image bytes to local storage (same convention as T-015:
    /// MinIO object key layout, local disk for now).
    pub async fn store_image(
        org_id: &str,
        image_id: Uuid,
        file_type: &str,
        bytes: &[u8],
    ) -> Result<String> {
        let dir = std::path::Path::new("uploads/style-images").join(org_id);
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(format!("{image_id}.{file_type}"));
        tokio::fs::write(&path, bytes).await?;
        Ok(path.to_string_lossy().to_string())
    }

    /// CLIP-encode the image and upsert its vector into the collection
    /// matching the embedding's actual dim.
    ///
    /// The point id equals the `style_images.id` UUID so a re-upload of the
    /// same row replaces rather than duplicates the vector. When the
    /// embedding has no writable route (e.g. a 1024 fallback vector whose
    /// only route is the protected legacy collection) the upsert fails
    /// closed before any HTTP request.
    pub async fn index_image(
        service: &ImageSearchService,
        image_id: Uuid,
        image_path: &str,
        style_id: Option<Uuid>,
        org_id: &str,
        dept_id: &str,
        bytes: &[u8],
    ) -> Result<()> {
        let embedding = service.clip().encode_image(bytes).await?;
        // T-027: embed-path observability — which provider actually served
        // this image's embedding (local vs DashScope fallback).
        let provider = embedding.provider_str();
        let vector_dim = embedding.dimension();

        let payload = serde_json::json!({
            "image_path": image_path,
            "style_id": style_id.map(|s| s.to_string()),
            "org_id": org_id,
            "dept_id": dept_id,
        });

        service
            .publish_raw_point(RawPoint {
                id: image_id.to_string(),
                vector: embedding.vector,
                payload,
            })
            .await?;

        tracing::info!(
            %image_id,
            style_id = ?style_id,
            provider,
            vector_dim,
            "Image indexed in Qdrant"
        );
        Ok(())
    }
}
