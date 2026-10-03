//! Fashion AI Platform — Core Inference Service (codex-rs fork)
//!
//! Entry point: thin wrapper over the `api_core` library crate.
//! All wiring lives in `src/lib.rs::run`; integration tests in `tests/`
//! exercise the same library target through real HTTP.

use api_core::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}
