//! Image indexing pipeline — persists uploaded images and upserts CLIP
//! vectors into the Qdrant `style_images` collection.
//!
//! ```text
//! upload (HTTP handler)
//!   → write bytes to uploads/style-images/{org_id}/{uuid}.{ext}  (MinIO later)
//!   → INSERT style_images
//!   → [spawned] ClipClient::encode_image
//!   → QdrantStore::upsert_raw_points  collection=style_images
//!       payload = { image_path, style_id, org_id, dept_id }
//! ```

use anyhow::Result;
use uuid::Uuid;

use crate::rag::{QdrantStore, RawPoint};

use super::clip::ClipClient;

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

    /// CLIP-encode the image and upsert its vector into Qdrant.
    ///
    /// The point id equals the `style_images.id` UUID so a re-upload of the
    /// same row replaces rather than duplicates the vector.
    pub async fn index_image(
        clip: &ClipClient,
        store: &QdrantStore,
        image_id: Uuid,
        image_path: &str,
        style_id: Option<Uuid>,
        org_id: &str,
        dept_id: &str,
        bytes: &[u8],
    ) -> Result<()> {
        let vector = clip.encode_image(bytes).await?;

        let payload = serde_json::json!({
            "image_path": image_path,
            "style_id": style_id.map(|s| s.to_string()),
            "org_id": org_id,
            "dept_id": dept_id,
        });

        store
            .upsert_raw_points(vec![RawPoint {
                id: image_id.to_string(),
                vector,
                payload,
            }])
            .await?;

        tracing::info!(%image_id, style_id = ?style_id, "Image indexed in Qdrant");
        Ok(())
    }
}
