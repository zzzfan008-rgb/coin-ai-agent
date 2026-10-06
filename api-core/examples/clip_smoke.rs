//! Real-machine smoke for hybrid CLIP wiring (T-027 increment 1).
//!
//! NOT part of `cargo test` — on purpose this tool talks to real services
//! (a local CLIP POC on 127.0.0.1:8399 and/or DashScope), which the test
//! suite must never do.
//!
//! Usage (from api-core/):
//!   # Local success path (CLIP_PROVIDER defaults to hybrid):
//!   cargo run --example clip_smoke -- ../data/images/some-image.jpg
//!
//!   # Forced fallback path (local pointed at a dead port):
//!   CLIP_LOCAL_BASE_URL=http://127.0.0.1:18399 \
//!     RUST_LOG=info cargo run --example clip_smoke -- ../data/images/some-image.jpg
//!
//! Exit code 0 = encode succeeded (provider and dim reported);
//! 1 = encode failed (hybrid fallback logs the local failure, then the
//! DashScope attempt — which needs CLIP_API_ENDPOINT + DASHSCOPE_API_KEY).

use std::time::Instant;

use api_core::images::clip::{ClipClient, ClipConfig};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let path = std::env::args()
        .nth(1)
        .expect("usage: clip_smoke <image-file-path>");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));

    // Reuse the library's env→mode resolution (ClipConfig::mode()) instead
    // of showing only the raw CLIP_PROVIDER string, which is empty when unset.
    let config = ClipConfig::from_env();
    let mode = config.mode();
    let client = ClipClient::new(config);
    let raw_provider = std::env::var("CLIP_PROVIDER").unwrap_or_default();
    println!(
        "clip_smoke: mode={mode:?} raw_provider={raw_provider:?} model={} local_base_url set={} api_endpoint set={}",
        client.model_name(),
        std::env::var("CLIP_LOCAL_BASE_URL").is_ok(),
        std::env::var("CLIP_API_ENDPOINT").is_ok(),
    );

    let started = Instant::now();
    match client.encode_image(&bytes).await {
        Ok(embedding) => {
            println!(
                "encode OK: provider_used={} dim={} elapsed_ms={} vector[0..4]={:?}",
                embedding.provider_str(),
                embedding.dimension(),
                started.elapsed().as_millis(),
                &embedding.vector[..embedding.vector.len().min(4)],
            );
        }
        Err(e) => {
            eprintln!(
                "encode FAILED after {}ms: {e:#}",
                started.elapsed().as_millis()
            );
            std::process::exit(1);
        }
    }
}
