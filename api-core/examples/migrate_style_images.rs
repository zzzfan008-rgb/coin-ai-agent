//! Style image collection migration (P2-2): move points from the legacy
//! `style_images` collection into the dim-named writable collection.
//!
//! The named target is DERIVED at runtime from the live `CLIP_*` configuration
//! — the same chain `ImageSearchService::from_env()` uses (mode → provider,
//! model + resolved dim → collection name, first writable route wins). Nothing
//! is hardcoded here, so the migration always targets exactly the collection
//! the service would write to.
//!
//! Usage:
//! ```sh
//! cargo run --example migrate_style_images -- <SOURCE> [<TARGET>]
//! # defaults:
//! #   SOURCE = style_images
//! #   TARGET = derived from current CLIP_* config
//! ```
//!
//! The migration is a READ-COPY — vectors are copied verbatim (no re-embed,
//! no CLIP call) so the collection dimension never shifts. The legacy source
//! collection is left untouched: this is a transition, not a destructive
//! rewrite. Points in the target are written as the same UUIDs as the source,
//! so re-running the migration is idempotent.

use anyhow::{Context, Result};
use api_core::images::clip::{ClipClient, ClipConfig};
use api_core::images::{migrate::migrate_points, resolve_vector_dim, route_specs};
use api_core::rag::{is_protected_legacy, QdrantStore};

/// Legacy read-only collection (points are copied out; nothing is deleted).
const LEGACY_SOURCE: &str = "style_images";

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env the same way the service does (config.rs AppConfig::load),
    // so TARGET derivation reads the exact same CLIP_* the service binds.
    let _ = dotenvy::dotenv();
    // ── Derive the target from the live CLIP configuration ─────────────────
    // Mirrors ImageSearchService::from_env(): config → mode/provider/model,
    // dim resolution, route_specs → first writable route.
    let clip_config = ClipConfig::from_env();
    let mode = clip_config.mode();
    let dim = resolve_vector_dim(
        std::env::var("CLIP_VECTOR_DIM")
            .ok()
            .and_then(|v| v.parse().ok()),
        &clip_config.provider,
    );
    let clip = ClipClient::new(clip_config);
    let (m_provider, m_model) = clip.collection_identity();
    let model = clip.model_name().to_string();
    let specs = route_specs(mode, &model, dim);
    let derived_target = specs
        .iter()
        .find(|(_, _, w)| *w)
        .map(|(_, name, _)| name.clone());
    let Some(derived_target) = derived_target else {
        anyhow::bail!(
            "no writable route for mode/provider (mode={} provider={m_provider} model={m_model} \
             dim={dim}) — configure CLIP_* first",
            mode.as_str()
        );
    };
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://127.0.0.1:6333".into());
    let source_arg = std::env::args().nth(1);
    let target_arg = std::env::args().nth(2);
    let source = source_arg.unwrap_or_else(|| LEGACY_SOURCE.to_string());
    let target_explicit = target_arg.is_some();
    let target = target_arg.unwrap_or_else(|| derived_target.clone());

    println!(
        "migrate_style_images: clip mode={} provider={m_provider} model={m_model} dim={dim}",
        mode.as_str()
    );
    println!("migrate_style_images: source={source}");
    if target_explicit {
        println!("migrate_style_images: target={target} (explicit)");
    } else {
        println!("migrate_style_images: target={target} (derived from config)");
    }
    if target == derived_target {
        println!("migrate_style_images: target matches service derivation ({derived_target})");
    } else {
        println!(
            "migrate_style_images: WARNING — explicit target differs from config-derived {derived_target}"
        );
    }
    if is_protected_legacy(&source) {
        println!("migrate_style_images: source '{source}' is protected legacy (read-only, ok)");
    }
    if is_protected_legacy(&target) {
        anyhow::bail!(
            "target '{target}' is a PROTECTED LEGACY collection name — writes into legacy \
             collections are forbidden. Pass a generated name or fix the CLIP config."
        );
    }
    if source == target {
        anyhow::bail!("refusing to migrate: source and target are the same collection '{source}'");
    }

    // ── Ensure target exists (upsert is idempotent) ───────────────────────
    let target_store = QdrantStore::new(&qdrant_url, &target);
    target_store
        .ensure_collection(
            dim,
            Some(serde_json::json!({
                "vector_dim": dim,
                "model": m_model,
                "provider": m_provider,
                "origin": "migrate_style_images",
                "vector_type": "dense",
            })),
        )
        .await
        .with_context(|| format!("ensure target collection '{target}'"))?;
    println!("migrate_style_images: ensured target collection '{target}'");

    // ── Copy points verbatim (no re-embed — dimension cannot shift) ────────
    let source_store = QdrantStore::new(&qdrant_url, &source);
    let stats = migrate_points(&source_store, &target_store, &target, 64, 64)
        .await
        .with_context(|| format!("migrate {source} -> {target}"))?;

    println!(
        "migrate_style_images: DONE source={} target={} scrolled={} copied={} batches={} elapsed_ms={}",
        source, target, stats.scrolled, stats.copied, stats.batches, stats.elapsed_ms
    );
    Ok(())
}
