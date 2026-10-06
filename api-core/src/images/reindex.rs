//! Batch re-index pipeline — local CLIP encode → upsert into a dedicated
//! dim+model-named Qdrant collection (T-027 increment 2).
//!
//! The filesystem-aware entry point lives in
//! `examples/reindex_clip.rs`; the logic here is library code so the
//! pipeline gets hermetic tests (wiremock CLIP + Qdrant) like everything
//! else.
//!
//! Safety invariants:
//!   - the target collection must never be a protected legacy name
//!     (`is_protected_legacy`), checked here and again in the example;
//!   - every embedding must be produced by the LOCAL provider
//!     (`provider_used=local`), otherwise the run aborts;
//!   - point ids are deterministic per source filename (UUID v5), so
//!     repeated runs replace points rather than duplicate them.

use std::time::Instant;

use anyhow::bail;
use uuid::Uuid;

use crate::rag::{is_protected_legacy, QdrantStore, RawPoint};

use super::clip::{ClipClient, ClipProviderId};

/// One source file to re-index: a display name (drives the stable point
/// id) and its raw bytes.
pub struct Reinput {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A per-file failure collected during the run.
#[derive(Debug, Clone)]
pub struct ReindexFailure {
    pub name: String,
    pub error: String,
}

/// Run statistics, printed by the example.
#[derive(Debug)]
pub struct ReindexStats {
    pub collection: String,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    /// Common vector dim (absent when nothing embedded).
    pub dim: Option<usize>,
    pub model: String,
    /// Number of upsert batches flushed.
    pub batches: usize,
    pub elapsed_ms: u128,
    /// Point ids in processing order — checked for stability on rerun.
    pub point_ids: Vec<String>,
    pub failures: Vec<ReindexFailure>,
}

/// Fixed namespace for deterministic per-file UUIDs.
///
/// DO NOT change this value: it is part of the idempotency contract —
/// changing it would make every existing file resolve to a fresh point id.
const REINDEX_NAMESPACE: Uuid = Uuid::from_u128(0x6d43_4c49_5000_5230_3237_0000_0000_0001);

/// Deterministic point id for a source filename (UUID v5 over a fixed
/// namespace). The same filename always maps to the same id regardless
/// of machine or run, which makes the pipeline idempotent.
pub fn stable_point_id(source_name: &str) -> Uuid {
    Uuid::new_v5(&REINDEX_NAMESPACE, source_name.as_bytes())
}

/// Re-index all inputs: encode each via the local CLIP provider and
/// upsert into `collection` in batches of `batch_size`.
///
/// Aborts (`Err`) on configuration-level violations:
///   - target collection is a protected legacy name;
///   - any embedding came from a non-local provider.
/// Per-file encode failures are collected into `stats.failures` and the
/// run completes so one bad file does not lose the batch.
pub async fn run_reindex(
    clip: &ClipClient,
    store: &QdrantStore,
    collection: &str,
    inputs: Vec<Reinput>,
    batch_size: usize,
) -> anyhow::Result<ReindexStats> {
    if is_protected_legacy(collection) {
        bail!("refusing to reindex into protected legacy collection '{collection}'");
    }
    let batch_size = batch_size.max(1);

    let started = Instant::now();
    let model = clip.model_name().to_string();
    let total = inputs.len();

    let mut dim: Option<usize> = None;
    let mut point_ids: Vec<String> = Vec::new();
    let mut failures: Vec<ReindexFailure> = Vec::new();
    let mut succeeded = 0usize;
    let mut batches = 0usize;
    let mut pending: Vec<RawPoint> = Vec::with_capacity(batch_size);

    for input in inputs {
        let Reinput { name, bytes } = input;
        let point_id = stable_point_id(&name);

        let embedding = match clip.encode_image(&bytes).await {
            Ok(embedding) => embedding,
            Err(e) => {
                failures.push(ReindexFailure {
                    name,
                    error: format!("{e:#}"),
                });
                continue;
            }
        };

        // Hard provider contract: the reindex set must be embedded
        // locally. A fallback-served embedding (provider != local) aborts
        // the run so data never lands via an unverified provider.
        if embedding.provider != ClipProviderId::Local {
            bail!(
                "reindex refused: '{name}' was embedded by provider '{}', expected local — aborting",
                embedding.provider_str()
            );
        }

        let got_dim = embedding.dimension();
        if let Some(expected) = dim {
            if got_dim != expected {
                failures.push(ReindexFailure {
                    name,
                    error: format!("dim drift within batch: {got_dim} != {expected}"),
                });
                continue;
            }
        } else {
            dim = Some(got_dim);
        }

        let payload = serde_json::json!({
            "image_path": name,
            "source_file": name,
            "provider": "local",
            "model": model,
            "vector_dim": got_dim,
            "kind": "reindex-poc",
        });
        pending.push(RawPoint {
            id: point_id.to_string(),
            vector: embedding.vector,
            payload,
        });
        point_ids.push(point_id.to_string());
        succeeded += 1;

        if pending.len() >= batch_size {
            store
                .upsert_raw_points(std::mem::take(&mut pending))
                .await?;
            batches += 1;
        }
    }

    if !pending.is_empty() {
        store.upsert_raw_points(pending).await?;
        batches += 1;
    }

    Ok(ReindexStats {
        collection: collection.to_string(),
        total,
        succeeded,
        failed: failures.len(),
        dim,
        model,
        batches,
        elapsed_ms: started.elapsed().as_millis(),
        point_ids,
        failures,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::images::clip::ClipConfig;

    const LOCAL_COLLECTION: &str = "style_images_local_clipvitb32_512";

    fn fake_vector(n: usize) -> serde_json::Value {
        let vals: Vec<f64> = (0..n).map(|i| (i as f64) * 0.001).collect();
        serde_json::to_value(vals).unwrap()
    }

    /// Local CLIP server: /encode/image → 512 proxy-shape.
    async fn local_clip_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/encode/image"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embedding": fake_vector(512), "dim": 512, "device": "cpu",
            })))
            .mount(&server)
            .await;
        server
    }

    /// Qdrant: collection GET (512, so ensure/verify drift checks pass)
    /// and points PUT accepted.
    async fn qdrant_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/collections/{LOCAL_COLLECTION}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"config": {"params": {"vectors": {
                    "size": 512, "distance": "Cosine",
                }}}},
            })))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(format!("/collections/{LOCAL_COLLECTION}/points")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"operation_id": 0, "status": "completed"},
                "status": "ok",
            })))
            .mount(&server)
            .await;
        server
    }

    fn local_client(base_url: &str) -> ClipClient {
        ClipClient::new(ClipConfig {
            provider: "local".into(),
            api_endpoint: String::new(),
            api_key: String::new(),
            model: String::new(),
            local_base_url: base_url.into(),
            local_timeout: Duration::from_secs(5),
        })
    }

    fn inputs(names: &[&str]) -> Vec<Reinput> {
        names
            .iter()
            .map(|name| Reinput {
                name: (*name).into(),
                bytes: b"fake-jpeg".to_vec(),
            })
            .collect()
    }

    #[test]
    fn stable_point_ids_are_deterministic_and_distinct() {
        assert_eq!(
            stable_point_id("a.jpg"),
            stable_point_id("a.jpg"),
            "same filename must resolve to the same id across runs"
        );
        assert_ne!(
            stable_point_id("a.jpg"),
            stable_point_id("b.jpg"),
            "different filenames must resolve differently"
        );
    }

    #[tokio::test]
    async fn reindexes_all_files_via_local_provider() {
        let clip_server = local_clip_server().await;
        let qdrant = qdrant_server().await;

        let clip = local_client(&clip_server.uri());
        let store = QdrantStore::new(&qdrant.uri(), LOCAL_COLLECTION);
        let stats = run_reindex(
            &clip,
            &store,
            LOCAL_COLLECTION,
            inputs(&["one.jpg", "two.jpg", "three.jpg"]),
            2,
        )
        .await
        .expect("reindex run");

        assert_eq!(stats.total, 3);
        assert_eq!(stats.succeeded, 3);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.dim, Some(512));
        assert_eq!(stats.model, "clip-vit-base-patch32");
        // batch_size=2 over 3 files → 2 upsert batches.
        assert_eq!(stats.batches, 2);
        assert_eq!(stats.point_ids.len(), 3);
        assert_eq!(
            stats.point_ids,
            ["one.jpg", "two.jpg", "three.jpg"]
                .iter()
                .map(|n| stable_point_id(n).to_string())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn rerun_produces_identical_point_ids() {
        // Idempotency at the identity level: a second run over the same
        // files must produce exactly the same id set in the same order, so
        // Qdrant upserts replace rather than append.
        let clip_server = local_clip_server().await;
        let qdrant = qdrant_server().await;

        let clip = local_client(&clip_server.uri());
        let store = QdrantStore::new(&qdrant.uri(), LOCAL_COLLECTION);
        let first = run_reindex(
            &clip,
            &store,
            LOCAL_COLLECTION,
            inputs(&["one.jpg", "two.jpg"]),
            16,
        )
        .await
        .expect("first run");
        let second = run_reindex(
            &clip,
            &store,
            LOCAL_COLLECTION,
            inputs(&["one.jpg", "two.jpg"]),
            16,
        )
        .await
        .expect("second run");

        assert_eq!(first.point_ids, second.point_ids);
        assert_eq!(first.succeeded, second.succeeded);

        // Both runs upserted (replacement is still an upsert): two PUTs
        // total, same ids both times.
        let puts = qdrant
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.to_string() == "PUT")
            .count();
        assert_eq!(puts, 2);
    }

    #[tokio::test]
    async fn rejects_non_local_provider_embeddings() {
        // A DashScope-configured client (1024 dashscope shape): run_reindex
        // must abort rather than accept the non-local embeddings.
        let dashscope = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(
                "/api/v1/services/embeddings/multimodal-embedding/multimodal-embedding",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "output": {"embeddings": [{"embedding": fake_vector(1024)}]},
            })))
            .mount(&dashscope)
            .await;
        let qdrant = qdrant_server().await;

        let clip = ClipClient::new(ClipConfig {
            provider: "dashscope".into(),
            api_endpoint: dashscope.uri(),
            api_key: String::new(),
            model: String::new(),
            local_base_url: String::new(),
            local_timeout: Duration::from_secs(5),
        });
        let store = QdrantStore::new(&qdrant.uri(), LOCAL_COLLECTION);
        let err = run_reindex(&clip, &store, LOCAL_COLLECTION, inputs(&["one.jpg"]), 16)
            .await
            .expect_err("non-local embeddings must abort the run");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("provider 'dashscope'"),
            "error must name the offending provider: {msg}"
        );

        // And nothing was written.
        let puts = qdrant
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.to_string() == "PUT")
            .count();
        assert_eq!(puts, 0);
    }

    #[tokio::test]
    async fn refuses_protected_legacy_target() {
        let clip_server = local_clip_server().await;
        let qdrant = qdrant_server().await;
        let clip = local_client(&clip_server.uri());
        let store = QdrantStore::new(&qdrant.uri(), "style_images");
        let err = run_reindex(&clip, &store, "style_images", inputs(&["one.jpg"]), 16)
            .await
            .expect_err("legacy target must be refused");
        assert!(
            format!("{err:#}").contains("protected legacy collection"),
            "unexpected error: {err:#}"
        );
    }

    #[tokio::test]
    async fn collect_failures_does_not_lose_the_batch() {
        // No CLIP server (dead port): every file fails to encode, but the
        // run completes with honest failure records and zero points.
        let qdrant = qdrant_server().await;
        let clip = local_client("http://127.0.0.1:18399");
        let store = QdrantStore::new(&qdrant.uri(), LOCAL_COLLECTION);
        let stats = run_reindex(
            &clip,
            &store,
            LOCAL_COLLECTION,
            inputs(&["one.jpg", "two.jpg"]),
            16,
        )
        .await
        .expect("run completes with per-file failures");
        assert_eq!(stats.total, 2);
        assert_eq!(stats.succeeded, 0);
        assert_eq!(stats.failed, 2);
        assert_eq!(stats.failures.len(), 2);
        assert!(stats.point_ids.is_empty());
    }
}
