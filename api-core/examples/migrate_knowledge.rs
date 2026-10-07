//! Real-machine legacy knowledge migration — copy every point of the
//! legacy `fashion_knowledge` collection into the document-RAG
//! dim+model-named collection (T-027 increment 3 addendum; user
//! decision: knowledge migration, exemption rejected).
//!
//! NOT part of `cargo test` — deliberately talks to the real Qdrant on
//! 127.0.0.1:6333. The copy is Qdrant-to-Qdrant only (scroll → upsert),
//! so no embedding API key is needed.
//!
//! Usage (from api-core/):
//!   cargo run --example migrate_knowledge
//!   cargo run --example migrate_knowledge -- fashion_knowledge \
//!       fashion_knowledge_dashscope_tev3_1536 128 128
//!
//! Positional args (all optional):
//!   1. source collection (default fashion_knowledge)
//!   2. target collection (default fashion_knowledge_dashscope_tev3_1536)
//!   3. scroll page size (default 128)
//!   4. upsert batch size (default 128)
//!
//! Safety guards:
//!   - the target is never a protected legacy name: a legacy collection
//!     may be the migration SOURCE but never the write target;
//!   - the source store is bound read-only, so the legacy collection
//!     cannot be mutated even by a bug in the copy;
//!   - the target collection is created first ("build then verify")
//!     with dim+model metadata; same-id upserts make the run idempotent
//!     and resumable.
//!
//! Exit 0 = copy complete and reconciled; 1 = run failed or counts do
//! not reconcile; 2 = configuration error.

use api_core::images::migrate::migrate_points;
use api_core::rag::{is_protected_legacy, knowledge_collection_name, QdrantStore};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let default_target = knowledge_collection_name("dashscope", "text-embedding-v3", 1536);

    let source: String = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fashion_knowledge".into());
    let target: String = std::env::args()
        .nth(2)
        .unwrap_or_else(|| default_target.clone());
    let page_size: usize = std::env::args()
        .nth(3)
        .and_then(|v| v.parse().ok())
        .unwrap_or(128);
    let batch_size: usize = std::env::args()
        .nth(4)
        .and_then(|v| v.parse().ok())
        .unwrap_or(128);

    // ── Configuration guards ───────────────────────────────────────────────
    if is_protected_legacy(&target) {
        eprintln!(
            "refusing to migrate: target '{target}' is a protected legacy collection \
             (legacy collections can only be migration sources, never targets)"
        );
        std::process::exit(2);
    }
    if source == target {
        eprintln!("refusing to migrate: source and target are the same collection '{source}'");
        std::process::exit(2);
    }

    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://127.0.0.1:6333".into());

    // Source: read-only binding (scroll only; cannot mutate the legacy
    // collection even accidentally).
    let source_store = QdrantStore::new_read_only(&qdrant_url, &source);
    // Target: writable, bound exactly to the named collection.
    let target_store = QdrantStore::new(&qdrant_url, &target);

    // ── Build the target collection first, then verify ─────────────────────
    target_store
        .ensure_collection(
            1536,
            Some(serde_json::json!({
                "vector_dim": 1536,
                "model": "text-embedding-v3",
                "provider": "dashscope",
                "created_by": "api-core migrate_knowledge",
                "task": "T-027",
            })),
        )
        .await
        .unwrap_or_else(|e| panic!("ensure_collection {target}: {e}"));

    println!(
        "migrate_knowledge: source={source} target={target} \
         page_size={page_size} batch_size={batch_size}"
    );

    let stats =
        match migrate_points(&source_store, &target_store, &target, page_size, batch_size).await {
            Ok(stats) => stats,
            Err(e) => {
                eprintln!("migration FAILED: {e:#}");
                std::process::exit(1);
            }
        };

    println!(
        "stats: scrolled={scrolled} copied={copied} batches={batches} elapsed_ms={elapsed_ms}",
        scrolled = stats.scrolled,
        copied = stats.copied,
        batches = stats.batches,
        elapsed_ms = stats.elapsed_ms,
    );
    println!(
        "source points_count: {source_count:?} (unchanged by migration)",
        source_count = stats.source_points_count
    );
    println!(
        "target points_count: before={before:?} after={after:?}",
        before = stats.target_points_count_before,
        after = stats.target_points_count_after
    );

    // Reconciliation: every scrolled point copied, and the target now
    // holds at least the source's point count.
    let reconciled = stats.scrolled == stats.copied
        && match (stats.source_points_count, stats.target_points_count_after) {
            (Some(src), Some(tgt)) => tgt >= src,
            _ => false,
        };
    if !reconciled {
        eprintln!(
            "migration did not reconcile — counts mismatch (run is idempotent, re-run to recover)"
        );
        std::process::exit(1);
    }
}
