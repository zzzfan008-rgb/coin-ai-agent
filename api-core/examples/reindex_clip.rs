//! Real-machine batch reindex — data/images via local CLIP into a new
//! 512-dim Qdrant collection (T-027 increment 2).
//!
//! NOT part of `cargo test` — deliberately talks to real services
//! (CLIP POC on 127.0.0.1:8399 and Qdrant on 127.0.0.1:6333).
//!
//! Usage (from api-core/):
//!   cargo run --example reindex_clip
//!   cargo run --example reindex_clip -- ../data/images 16
//!
//! Safety guards:
//!   - CLIP_PROVIDER defaults to local-only for this tool (unset env is
//!     forced to local); dashscope/generic modes are rejected;
//!   - the target collection name is derived from model+dim and cannot be
//!     a protected legacy name (fashion_knowledge/style_images);
//!   - the only QdrantStore instantiated is bound to that new collection;
//!   - run_reindex hard-asserts every embedding has provider_used=local.
//!
//! Exit 0 = all files indexed, dim consistent; 1 = run failed/aborted or
//! stats inconsistent; 2 = configuration error.

use std::time::Instant;

use api_core::images::clip::{ClipClient, ClipConfig, ClipMode};
use api_core::images::reindex::{run_reindex, Reinput};
use api_core::images::style_collection_name;
use api_core::rag::{is_protected_legacy, QdrantStore};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let image_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../data/images".into());
    let batch_size: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(16);

    // Force local when the provider is unset: the tool must not inherit
    // Generic mode from the shell.
    let mut config = ClipConfig::from_env();
    if config.provider.is_empty() {
        config.provider = "local".into();
    }
    if !matches!(config.mode(), ClipMode::Local | ClipMode::Hybrid) {
        eprintln!(
            "reindex_clip requires CLIP_PROVIDER local|hybrid, got {:?}",
            config.provider
        );
        std::process::exit(2);
    }

    let clip = ClipClient::new(config);
    let model = clip.model_name().to_string();
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://127.0.0.1:6333".into());

    // Collect + sort the image files (jpg/jpeg/png/webp).
    let mut paths: Vec<_> = std::fs::read_dir(&image_dir)
        .unwrap_or_else(|e| panic!("read_dir {image_dir}: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .map(|x| matches!(x.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();

    let mut inputs: Vec<Reinput> = Vec::with_capacity(paths.len());
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {name}: {e}"));
        inputs.push(Reinput { name, bytes });
    }

    // Target collection — T-026 local contract dim 512; the actual
    // embedding dim is asserted in run_reindex.
    let collection = style_collection_name("local", &model, 512);
    if is_protected_legacy(&collection) {
        eprintln!("internal guard tripped: derived a protected collection: {collection}");
        std::process::exit(2);
    }

    let store = QdrantStore::new(&qdrant_url, &collection);

    // Ensure the new collection exists (metadata-bearing; no-op with dim
    // verification when already present).
    store
        .ensure_collection(
            512,
            Some(serde_json::json!({
                "vector_dim": 512,
                "model": model,
                "provider": "local",
                "created_by": "api-core reindex_clip",
                "task": "T-027",
            })),
        )
        .await
        .unwrap_or_else(|e| panic!("ensure_collection {collection}: {e}"));

    let before = collection_points_count(&qdrant_url, &collection).await;

    println!(
        "reindex_clip: collection={collection} files={} batch_size={batch_size} model={model}",
        inputs.len()
    );
    let started = Instant::now();
    let stats = match run_reindex(&clip, &store, &collection, inputs, batch_size).await {
        Ok(stats) => stats,
        Err(e) => {
            eprintln!(
                "reindex ABORTED after {}ms: {e:#}",
                started.elapsed().as_millis()
            );
            std::process::exit(1);
        }
    };

    let after = collection_points_count(&qdrant_url, &collection).await;
    let unique = stats
        .point_ids
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();

    println!(
        "stats: total={total} succeeded={succeeded} failed={failed} \
dim={dim:?} batches={batches} elapsed_ms={elapsed_ms}",
        total = stats.total,
        succeeded = stats.succeeded,
        failed = stats.failed,
        dim = stats.dim,
        batches = stats.batches,
        elapsed_ms = stats.elapsed_ms,
    );
    println!("point_ids: {unique} unique of {}", stats.point_ids.len());
    println!("points_count: before={before:?} after={after:?}");
    for failure in &stats.failures {
        eprintln!("  FAIL {} — {}", failure.name, failure.error);
    }

    // Hard checks for a successful run.
    if stats.failed != 0
        || stats.succeeded != stats.total
        || stats.dim != Some(512)
        || unique != stats.point_ids.len()
    {
        std::process::exit(1);
    }
}

/// Read `points_count` for a collection; None when missing/unreadable.
async fn collection_points_count(qdrant_url: &str, collection: &str) -> Option<u64> {
    let url = format!("{qdrant_url}/collections/{collection}");
    let resp = reqwest::Client::new().get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("result")
        .and_then(|r| r.get("points_count"))
        .and_then(|v| v.as_u64())
}
