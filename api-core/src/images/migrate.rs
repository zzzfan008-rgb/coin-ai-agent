//! Legacy point migration — copy points from the legacy `style_images`
//! collection (1024-dim) into a provider-specific dim+model-named
//! collection (T-027 increment 3, phase C).
//!
//! The copy is pure Qdrant-to-Qdrant: scroll the source collection and
//! upsert every point unchanged (same id, vector, payload). No embedding
//! provider is involved, so no DashScope API key is needed.
//!
//! Safety invariants:
//!   - the target collection must never be a protected legacy name
//!     (`is_protected_legacy`) — a legacy collection may be the SOURCE
//!     of a migration but never the write target;
//!   - the source store is bound read-only, so even a bug here cannot
//!     mutate the legacy collection;
//!   - point ids are written back unchanged, so upserts replace rather
//!     than duplicate: the migration is idempotent and resumable.
//!
//! The filesystem/CLI-aware wrapper lives in
//! `examples/migrate_style_images.rs`; this module is library code so the
//! pipeline gets hermetic wiremock tests.

use std::time::Instant;

use anyhow::bail;

use crate::rag::{is_protected_legacy, QdrantStore};

/// Migration statistics, printed by the example.
#[derive(Debug)]
pub struct MigrationStats {
    pub source: String,
    pub target: String,
    /// Points scrolled from the source.
    pub scrolled: usize,
    /// Points upserted into the target.
    pub copied: usize,
    /// Number of upsert batches flushed.
    pub batches: usize,
    pub elapsed_ms: u128,
    pub source_points_count: Option<u64>,
    pub target_points_count_before: Option<u64>,
    pub target_points_count_after: Option<u64>,
}

/// Scroll every point out of `source` and upsert it into the target store
/// (which is bound to the target collection).
///
/// Aborts (`Err`) on configuration-level violations:
///   - the target name is a protected legacy name;
///   - the target store's bound collection differs from `target`
///     (defensive: prevents an example wiring mistake from writing
///     somewhere unexpected).
/// Scroll/upsert HTTP errors propagate, and the run is idempotently
/// resumable by re-running (same ids replace).
pub async fn migrate_points(
    source: &QdrantStore,
    target_store: &QdrantStore,
    target: &str,
    page_size: usize,
    batch_size: usize,
) -> anyhow::Result<MigrationStats> {
    if is_protected_legacy(target) {
        bail!("refusing to migrate into protected legacy collection '{target}'");
    }
    if target_store.collection_name() != target {
        bail!(
            "target store is bound to '{}' but migration target is '{target}' — refusing",
            target_store.collection_name()
        );
    }

    let page_size = page_size.clamp(1, 256);
    let batch_size = batch_size.clamp(1, 256);
    let started = Instant::now();
    let source_name = source.collection_name().to_string();

    let source_points_count = source.points_count().await?;
    let target_points_count_before = target_store.points_count().await?;

    let mut offset = None;
    let mut scrolled = 0usize;
    let mut copied = 0usize;
    let mut batches = 0usize;
    let mut pending: Vec<_> = Vec::with_capacity(batch_size);

    loop {
        let page = source.scroll_points(offset, page_size).await?;
        scrolled += page.points.len();

        let page_len = page.points.len();
        pending.extend(page.points);

        // Flush full batches as pages arrive; empty pages still advance
        // pagination (they do occur).
        while pending.len() >= batch_size {
            let split = batch_size;
            let batch: Vec<_> = pending.drain(..split).collect();
            target_store.upsert_scrolled_points(batch).await?;
            copied += batch_size;
            batches += 1;
        }

        match page.next_offset {
            Some(next) => offset = Some(next),
            None => break,
        }
        // A zero-size last page means we are done; Qdrant normally omits
        // it, but guard regardless.
        if page_len == 0 {
            break;
        }
    }

    if !pending.is_empty() {
        let tail = pending.len();
        target_store.upsert_scrolled_points(pending).await?;
        copied += tail;
        batches += 1;
    }

    let target_points_count_after = target_store.points_count().await?;

    Ok(MigrationStats {
        source: source_name,
        target: target.to_string(),
        scrolled,
        copied,
        batches,
        elapsed_ms: started.elapsed().as_millis(),
        source_points_count,
        target_points_count_before,
        target_points_count_after,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SOURCE_COLLECTION: &str = "style_images";
    const TARGET_COLLECTION: &str = "style_images_dashscope_mmembedv1_1024";

    /// One scroll point with the given UUID id, 1024 vector and a small
    /// payload.
    fn scroll_point_json(id: &str) -> serde_json::Value {
        let vector: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.001).collect();
        serde_json::json!({
            "id": id,
            "vector": vector,
            "payload": {"image_path": format!("{id}.jpg"), "org_id": "org"},
        })
    }

    fn scroll_response(ids: &[&str], next_page_offset: Option<&str>) -> ResponseTemplate {
        let points: Vec<_> = ids.iter().map(|id| scroll_point_json(id)).collect();
        let mut body = serde_json::json!({"result": {"points": points}, "status": "ok"});
        if let Some(offset) = next_page_offset {
            body["result"]["next_page_offset"] = serde_json::json!(offset);
        }
        ResponseTemplate::new(200).set_body_json(body)
    }

    /// Mount scroll pages on the source collection. Pages are returned in
    /// order: each call pops the next queued response (the last page is
    /// repeated if called more times).
    async fn source_server(pages: Vec<ResponseTemplate>, count: u64) -> MockServer {
        let server = MockServer::start().await;
        // wiremock 0.5 responders are synchronous closures: hand out the
        // queued pages front-to-back, repeating the last one.
        let queue: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<ResponseTemplate>>> =
            std::sync::Arc::new(std::sync::Mutex::new(pages.into()));
        Mock::given(method("POST"))
            .and(path(format!(
                "/collections/{SOURCE_COLLECTION}/points/scroll"
            )))
            .respond_with(move |_req: &wiremock::Request| {
                let mut q = queue.lock().unwrap();
                if q.len() > 1 {
                    q.pop_front().unwrap()
                } else {
                    // Last page: keep and repeat it.
                    q.front().unwrap().clone()
                }
            })
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/collections/{SOURCE_COLLECTION}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"points_count": count},
            })))
            .mount(&server)
            .await;
        server
    }

    /// Target Qdrant: collection-info reports `count`, points PUT accepted.
    async fn target_server(count: u64) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/collections/{TARGET_COLLECTION}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"points_count": count},
            })))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(format!("/collections/{TARGET_COLLECTION}/points")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"operation_id": 0, "status": "completed"},
                "status": "ok",
            })))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn copies_all_points_single_page() {
        let source = source_server(vec![scroll_response(&["id-1", "id-2"], None)], 2).await;
        let target = target_server(0).await;

        let source_store = QdrantStore::new_read_only(&source.uri(), SOURCE_COLLECTION);
        let target_store = QdrantStore::new(&target.uri(), TARGET_COLLECTION);
        let stats = migrate_points(&source_store, &target_store, TARGET_COLLECTION, 64, 64)
            .await
            .expect("migration");

        assert_eq!(stats.scrolled, 2);
        assert_eq!(stats.copied, 2);
        assert_eq!(stats.batches, 1);
        assert_eq!(stats.source_points_count, Some(2));
        assert_eq!(stats.target_points_count_before, Some(0));

        // The upsert body must carry the original ids, vectors and
        // payloads unchanged — one batch containing both points.
        let puts: Vec<_> = target
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.to_string() == "PUT")
            .collect();
        assert_eq!(puts.len(), 1, "single batch upsert");
        let body: serde_json::Value = serde_json::from_slice(&puts[0].body).unwrap();
        let points = body["points"].as_array().unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0]["id"], "id-1");
        assert_eq!(points[1]["id"], "id-2");
        assert_eq!(
            points[0]["vector"].as_array().unwrap().len(),
            1024,
            "vector preserved"
        );
        assert_eq!(points[0]["payload"]["image_path"], "id-1.jpg");

        // And nothing mutating was attempted against the source.
        let source_requests = source.received_requests().await.unwrap();
        assert!(
            source_requests
                .iter()
                .all(|r| r.method.to_string() != "PUT" && r.method.to_string() != "DELETE"),
            "source must see only scroll reads"
        );
    }

    #[tokio::test]
    async fn scrolls_multiple_pages_and_flushes_batches() {
        // Two pages of two points, batch_size=3 → first flush takes 3
        // points, the last point goes as a tail batch.
        let source = source_server(
            vec![
                scroll_response(&["a", "b"], Some("offset-1")),
                scroll_response(&["c", "d"], None),
            ],
            4,
        )
        .await;
        let target = target_server(0).await;

        let source_store = QdrantStore::new_read_only(&source.uri(), SOURCE_COLLECTION);
        let target_store = QdrantStore::new(&target.uri(), TARGET_COLLECTION);
        let stats = migrate_points(&source_store, &target_store, TARGET_COLLECTION, 2, 3)
            .await
            .expect("paginated migration");

        assert_eq!(stats.scrolled, 4);
        assert_eq!(stats.copied, 4);
        assert_eq!(stats.batches, 2);

        // Second scroll request must carry the opaque next_page_offset.
        let scrolls: Vec<_> = source
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.url.path().ends_with("/points/scroll"))
            .collect();
        assert_eq!(scrolls.len(), 2);
        let second_body: serde_json::Value = serde_json::from_slice(&scrolls[1].body).unwrap();
        assert_eq!(second_body["offset"], "offset-1");
    }

    #[tokio::test]
    async fn refuses_protected_legacy_target() {
        // No server needed: refusal precedes any HTTP.
        let source = MockServer::start().await;
        let target = MockServer::start().await;
        let source_store = QdrantStore::new_read_only(&source.uri(), SOURCE_COLLECTION);
        let target_store = QdrantStore::new(&target.uri(), "style_images");
        let err = migrate_points(&source_store, &target_store, "style_images", 64, 64)
            .await
            .expect_err("legacy target must be refused");
        assert!(
            format!("{err:#}").contains("protected legacy collection"),
            "{err:#}"
        );
        assert_eq!(source.received_requests().await.unwrap().len(), 0);
        assert_eq!(target.received_requests().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn refuses_when_target_store_bound_elsewhere() {
        // Store bound to a different collection than the declared target.
        let source = MockServer::start().await;
        let target = MockServer::start().await;
        let source_store = QdrantStore::new_read_only(&source.uri(), SOURCE_COLLECTION);
        let target_store = QdrantStore::new(&target.uri(), "style_images_local_x_512");
        let err = migrate_points(&source_store, &target_store, TARGET_COLLECTION, 64, 64)
            .await
            .expect_err("mismatched target binding must be refused");
        assert!(format!("{err:#}").contains("migration target"), "{err:#}");
        assert_eq!(source.received_requests().await.unwrap().len(), 0);
        assert_eq!(target.received_requests().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn rerun_is_idempotent_same_ids() {
        // Two runs over the same single page: upserts replace, counts
        // stay equal.
        let source = source_server(
            vec![
                scroll_response(&["id-1", "id-2"], None),
                scroll_response(&["id-1", "id-2"], None),
            ],
            2,
        )
        .await;
        let target = target_server(0).await;

        let source_store = QdrantStore::new_read_only(&source.uri(), SOURCE_COLLECTION);
        let target_store = QdrantStore::new(&target.uri(), TARGET_COLLECTION);
        let first = migrate_points(&source_store, &target_store, TARGET_COLLECTION, 64, 64)
            .await
            .unwrap();
        let second = migrate_points(&source_store, &target_store, TARGET_COLLECTION, 64, 64)
            .await
            .unwrap();

        assert_eq!(first.copied, second.copied);
        assert_eq!(second.scrolled, 2);
        let puts = target
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.to_string() == "PUT")
            .count();
        assert_eq!(puts, 2, "one replacement upsert per run");
    }
}
