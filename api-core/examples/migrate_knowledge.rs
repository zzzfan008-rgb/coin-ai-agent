//! Knowledge base migration: move points from the legacy `fashion_knowledge`
//! collection into a provider/model/dim-named collection.
//!
//! The named collection is chosen by the application configuration (the
//! `EMBEDDING_*` environment) — NOT hardcoded here. The default target below
//! is DERIVED at runtime from the same derivation chain the service uses
//! (`RagConfig::default()` reads `EMBEDDING_*`; the endpoint host selects the
//! provider; `knowledge_collection_name` composes the name), so the migration
//! always targets exactly the collection the live service would write to.
//!
//! Usage:
//! ```sh
//! cargo run --example migrate_knowledge -- <SOURCE> [<TARGET>]
//! # defaults:
//! #   SOURCE = fashion_knowledge
//! #   TARGET = derived from current EMBEDDING_* config
//! ```
//!
//! The migration is a READ-COPY — vectors are copied verbatim (no re-embedding,
//! no CLIP call) so the collection dimension never shifts. The legacy source
//! collection is left untouched: this is a transition, not a destructive
//! rewrite.
//!
//! Points in the target are written as the same UUIDs as the source, so
//! re-running the migration is idempotent (upsert replaces by ID).

use anyhow::{Context, Result};
use api_core::images::migrate::migrate_points;
use api_core::rag::{
    is_protected_legacy, knowledge_collection_name, EmbeddingService, QdrantStore, RagConfig,
};

/// Legacy read-only collection (points are copied out; nothing is deleted).
const LEGACY_SOURCE: &str = "fashion_knowledge";

#[tokio::main]
async fn main() -> Result<()> {
    // ── Derive the default target from the live embedding configuration ────
    // Mirrors RagRetriever::new: RagConfig::default() reads EMBEDDING_* env,
    // provider falls out of the endpoint host (apiyi/generic → "generic").
    let rag_cfg = RagConfig::default();
    let embedding = EmbeddingService::new(
        rag_cfg.embedding_api_key.clone(),
        rag_cfg.embedding_base_url.clone(),
        rag_cfg.embedding_model.clone(),
        rag_cfg.embedding_dim,
    );
    let (provider, model) = embedding.identity();
    let dim = rag_cfg.embedding_dim;
    let derived_target = knowledge_collection_name(provider, model, dim);
    let source_arg = std::env::args().nth(1);
    let target_arg = std::env::args().nth(2);
    let source = source_arg.unwrap_or_else(|| LEGACY_SOURCE.to_string());
    let target_explicit = target_arg.is_some();
    let target = target_arg.unwrap_or_else(|| derived_target.clone());

    println!(
        "migrate_knowledge: embedding endpoint={} provider={provider} model={model} dim={dim}",
        rag_cfg.embedding_base_url
    );
    println!("migrate_knowledge: source={source}");
    if target_explicit {
        println!("migrate_knowledge: target={target} (explicit)");
    } else {
        println!("migrate_knowledge: target={target} (derived from config)");
    }
    // The derived target must equal the target the service would bind.
    if target == derived_target {
        println!("migrate_knowledge: target matches service derivation ({derived_target})");
    } else {
        println!(
            "migrate_knowledge: WARNING — explicit target differs from config-derived {derived_target}"
        );
    }
    if is_protected_legacy(&source) {
        println!("migrate_knowledge: source '{source}' is protected legacy (read-only, ok)");
    }
    if is_protected_legacy(&target) {
        anyhow::bail!(
            "target '{target}' is a PROTECTED LEGACY collection name — writes into legacy \
             collections are forbidden. Pass a generated name or fix the embedding config."
        );
    }

    // ── Ensure target exists (upsert is idempotent) ───────────────────────
    let target_store = QdrantStore::new(&rag_cfg.qdrant_url, &target);
    target_store
        .ensure_collection(
            dim,
            Some(serde_json::json!({
                "vector_dim": dim,
                "model": model,
                "provider": provider,
                "origin": "migrate_knowledge",
                "vector_type": "dense",
            })),
        )
        .await
        .with_context(|| format!("ensure target collection '{target}'"))?;
    println!("migrate_knowledge: ensured target collection '{target}'");

    // ── Copy points verbatim (no re-embed — dimension cannot shift) ────────
    let source_store = QdrantStore::new(&rag_cfg.qdrant_url, &source);
    let stats = migrate_points(&source_store, &target_store, &target, 64, 64)
        .await
        .with_context(|| format!("migrate {source} -> {target}"))?;

    println!(
        "migrate_knowledge: DONE source={} target={} scrolled={} copied={} batches={} elapsed_ms={}",
        source, target, stats.scrolled, stats.copied, stats.batches, stats.elapsed_ms
    );
    Ok(())
}
