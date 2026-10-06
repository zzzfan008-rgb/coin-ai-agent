//! Images module — CLIP image encoding, indexing, and image search (T-019).
//!
//! Storage layout (T-027, increment 3 phase C):
//!   - Every CLIP provider mode writes its OWN collection whose name
//!     records provider, model and dim, and whose collection metadata
//!     (Qdrant 1.19+) repeats those fields:
//!       Local      → style_images_local_{model-tag}_{dim}
//!       DashScope  → style_images_dashscope_{model-tag}_{dim}
//!       Generic    → style_images_generic_{model-tag}_{dim}
//!       Hybrid     → local dim named collection (writable)
//!   - The legacy `style_images` collection (1024) is referenced ONLY
//!     read-only: as the transitional fallback while a mode's new
//!     collection is missing or empty, and as the source of the one-off
//!     migration script. Hybrid's 1024 fallback route stays bound to it
//!     for the whole transition.
//!   - `fashion_knowledge` is a separate document-RAG chain (collection
//!     name from knowledge config) and is exempted from migration.
//!   - Payload: `{ image_path, style_id, org_id, dept_id }`
//!   - Tenant isolation: `must` filter on `org_id` AND `dept_id` at search time.

pub mod clip;
pub mod indexer;
pub mod migrate;
pub mod reindex;

use anyhow::{bail, Result};
use uuid::Uuid;

use crate::rag::{QdrantStore, RawPoint};

use clip::{ClipClient, ClipMode};

/// A single image search hit (before style-name enrichment).
#[derive(Debug, Clone)]
pub struct ImageHit {
    pub image_path: String,
    pub style_id: Option<String>,
    pub score: f32,
}

/// Outcome of `search_similar`: hits plus which CLIP provider actually
/// served the query embedding (T-027 observability; surfaced to callers as
/// `provider_used`).
#[derive(Debug, Clone)]
pub struct SimilarSearchOutcome {
    pub hits: Vec<ImageHit>,
    /// Wire name from `clip::ClipProviderId::as_str()`.
    pub provider_used: String,
}

// ── Collection naming ────────────────────────────────────────────────────────

/// Short, collection-name-safe model identifier. Known models get explicit
/// short tags (the raw model name contains hyphens and is long); an unknown
/// model falls back to its ASCII-alphanumeric characters lowercased.
fn model_tag(model: &str) -> String {
    match model {
        "clip-vit-base-patch32" => "clipvitb32".into(),
        "multimodal-embedding-v1" => "mmembedv1".into(),
        other => other
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase(),
    }
}

/// Name of a style-image collection:
/// `style_images_{provider}_{model-tag}_{dim}`.
///
/// The dim is the trailing token so logs and listings sort by embedding
/// space and a 512 collection is visually unmistakeable.
pub fn style_collection_name(provider: &str, model: &str, dim: usize) -> String {
    format!("style_images_{provider}_{}_{dim}", model_tag(model))
}

// ── Routes ───────────────────────────────────────────────────────────────────

/// One physical collection route for one vector dim.
#[derive(Clone)]
pub struct StyleRoute {
    /// Vector dimension this route accepts.
    pub dim: usize,
    /// Bound collection name.
    pub collection: String,
    /// Writable routes may be created and receive upserts; read-only routes
    /// back searches onto legacy collections and fail every mutation.
    pub writable: bool,
    store: QdrantStore,
}

/// High-level service composing the CLIP client and dim-routed Qdrant stores.
#[derive(Clone)]
pub struct ImageSearchService {
    clip: ClipClient,
    routes: Vec<StyleRoute>,
}

/// Resolve the `style_images` collection vector dim at startup.
///
/// Priority (T-027): an explicit `CLIP_VECTOR_DIM` env value always wins —
/// existing deployments that set it in `.env` behave exactly as before.
/// Without it the dim follows the configured provider:
///   - DashScope `multimodal-embedding-v1` emits 1024 dims (the pre-T-027
///     default of 512 was wrong for it — the config pitfall this fixes);
///   - local CLIP ViT-B/32, Generic, and Hybrid (local is primary) are 512.
pub fn resolve_vector_dim(explicit: Option<usize>, provider: &str) -> usize {
    if let Some(d) = explicit {
        return d;
    }
    match provider {
        "dashscope" | "qwen" => 1024,
        _ => 512,
    }
}

/// Pure route-table construction. Each spec is
/// `(vector_dim, collection_name, writable)`.
///
/// Split out from `from_env` so the table is testable without process
/// environment variables (which race under `cargo test`).
///
/// Writable collections (increment 3 phase C): every mode writes a
/// dim+model-named collection —
///   Local   → style_images_local_{model-tag}_{dim}
///   DashScope → style_images_dashscope_mmembedv1_{dim}
///   Generic → style_images_generic_{model-tag}_{dim}
/// Read-only fallback routes:
///   - DashScope/Generic at dim 1024 additionally bind the legacy
///     `style_images` so searches fail-safe to it while the new
///     collection is missing or empty. The fallback is added only when
///     dimensionally compatible: the legacy collection is 1024, and a
///     512 vector structurally cannot search it.
///   - Hybrid keeps the increment-2 table unchanged: 512 writable named
///     collection plus a 1024 read-only legacy route (its DashScope
///     fallback route stays on the legacy collection for the whole
///     transition, per the card).
pub fn route_specs(mode: ClipMode, model: &str, dim: usize) -> Vec<(usize, String, bool)> {
    match mode {
        ClipMode::Local => vec![(dim, style_collection_name("local", model, dim), true)],
        ClipMode::Hybrid => vec![
            (dim, style_collection_name("local", model, dim), true),
            (1024, "style_images".into(), false),
        ],
        ClipMode::DashScope => {
            let mut specs = vec![(
                dim,
                style_collection_name("dashscope", "multimodal-embedding-v1", dim),
                true,
            )];
            if dim == 1024 {
                specs.push((dim, "style_images".into(), false));
            }
            specs
        }
        ClipMode::Generic => {
            let mut specs = vec![(dim, style_collection_name("generic", model, dim), true)];
            if dim == 1024 {
                specs.push((dim, "style_images".into(), false));
            }
            specs
        }
    }
}

impl ImageSearchService {
    /// Build from environment configuration.
    pub fn from_env() -> Self {
        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());

        let config = clip::ClipConfig::from_env();
        let mode = config.mode();
        let dim = resolve_vector_dim(
            std::env::var("CLIP_VECTOR_DIM")
                .ok()
                .and_then(|v| v.parse().ok()),
            &config.provider,
        );
        let clip = ClipClient::new(config);
        let model = clip.model_name();

        // Route table per mode (pure construction in `route_specs`).
        let specs = route_specs(mode, model, dim);
        Self::new(clip, &qdrant_url, specs)
    }

    /// Build from an explicit client and route specs — production uses
    /// `from_env`; tests inject dead ports/wiremock this way so `cargo test`
    /// never touches a real Qdrant or CLIP service.
    ///
    /// Each spec is `(vector_dim, collection_name, writable)`.
    pub fn new(clip: ClipClient, qdrant_url: &str, specs: Vec<(usize, String, bool)>) -> Self {
        let routes = specs
            .into_iter()
            .map(|(dim, collection, writable)| {
                let store = if writable {
                    QdrantStore::new(qdrant_url, &collection)
                } else {
                    QdrantStore::new_read_only(qdrant_url, &collection)
                };
                StyleRoute {
                    dim,
                    collection,
                    writable,
                    store,
                }
            })
            .collect();
        Self { clip, routes }
    }

    /// Ensure writable collections exist with dim+model metadata; verify
    /// read-only legacy routes without creating or touching them.
    pub async fn ensure_collection(&self) -> Result<()> {
        for route in &self.routes {
            if route.writable {
                // Metadata identity is derived from the mode actually in
                // effect, never hardcoded: DashScope routes must not be
                // stamped provider=local (F2).
                let (provider, metadata_model) = self.clip.collection_identity();
                let metadata = serde_json::json!({
                    "vector_dim": route.dim,
                    "model": metadata_model,
                    "provider": provider,
                    "created_by": "api-core",
                    "task": "T-027",
                });
                route
                    .store
                    .ensure_collection(route.dim, Some(metadata))
                    .await?;
            } else {
                match route.store.verify_collection_dim(route.dim).await? {
                    true => {}
                    false => tracing::debug!(
                        collection = %route.collection,
                        dim = route.dim,
                        "legacy collection missing — fallback searches return empty until it exists"
                    ),
                }
            }
        }
        Ok(())
    }

    /// Access the CLIP client (used by the background indexing task).
    pub fn clip(&self) -> &ClipClient {
        &self.clip
    }

    /// Whether a CLIP endpoint is configured.
    pub fn configured(&self) -> bool {
        self.clip.configured()
    }

    /// CLIP model name (for startup logging).
    pub fn model_name(&self) -> &str {
        self.clip.model_name()
    }

    /// Active mode wire name (health surface).
    pub fn mode_str(&self) -> &'static str {
        self.clip.mode().as_str()
    }

    /// Resolve the route for a concrete vector dim. Fails closed:
    ///   - no route for the dim (an embedding space this service has no
    ///     collection for) → error;
    ///   - write requested against a read-only legacy route → error.
    fn route_for_dim(&self, dim: usize, for_write: bool) -> Result<&StyleRoute> {
        match self.routes.iter().find(|r| r.dim == dim) {
            Some(route) if !for_write || route.writable => Ok(route),
            Some(route) => bail!(
                "vector dim {dim} maps to collection '{}' which is read-only/legacy — writes refused",
                route.collection
            ),
            None => bail!(
                "no style collection route for vector dim {dim} (routes: [{}])",
                self.routes
                    .iter()
                    .map(|r| format!("{}:{}", r.dim, r.collection))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    /// Resolve the effective READ route for a dim, applying the
    /// transitional fail-safe semantics (increment 3 phase C).
    ///
    /// A dim may have up to two routes: the primary writable
    /// dim+model-named collection and a read-only legacy fallback:
    ///   - named collection exists with points_count > 0 → search it;
    ///   - named collection missing or empty → search the read-only
    ///     legacy route when present (fail-safe);
    ///   - a single route for the dim (e.g. Local's 512 collection, or
    ///     Hybrid's legacy-only 1024 route) is used directly with no
    ///     extra HTTP;
    ///   - no route at all fails closed with the same error as
    ///     `route_for_dim`.
    ///
    /// The points-count GET is the only added request (one per search on
    /// a two-route dim); transport errors propagate rather than silently
    /// degrading.
    async fn resolve_read_route(&self, dim: usize) -> Result<&StyleRoute> {
        let matching: Vec<&StyleRoute> = self.routes.iter().filter(|r| r.dim == dim).collect();
        if matching.is_empty() {
            bail!(
                "no style collection route for vector dim {dim} (routes: [{}])",
                self.routes
                    .iter()
                    .map(|r| format!("{}:{}", r.dim, r.collection))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let named = matching.iter().copied().find(|r| r.writable);
        let legacy = matching.iter().copied().find(|r| !r.writable);
        match (named, legacy) {
            (Some(primary), Some(fallback)) => {
                let populated = primary.store.points_count().await?;
                match populated {
                    Some(count) if count > 0 => Ok(primary),
                    _ => Ok(fallback),
                }
            }
            (Some(only), None) | (None, Some(only)) => Ok(only),
            (None, None) => unreachable!("non-empty matching set"),
        }
    }

    /// Upsert one indexed point, routed by the vector's actual dimension.
    /// A dim with no writable route (e.g. a 1024 vector against the
    /// read-only legacy collection) fails before any HTTP request.
    pub async fn publish_raw_point(&self, point: RawPoint) -> Result<()> {
        let dim = point.vector.len();
        let route = self.route_for_dim(dim, true)?;
        route.store.upsert_raw_points(vec![point]).await
    }

    /// Encode an image and run an ANN search scoped to org + dept. The
    /// query goes to the collection matching the EMBEDDING's actual dim —
    /// a 512 vector can never reach a 1024 collection — with transitional
    /// fail-safe routing (named collection preferred, read-only legacy
    /// fallback; see `resolve_read_route`). The returned outcome also
    /// reports which CLIP provider served the query embedding
    /// (`provider_used`), so callers can surface it in responses.
    pub async fn search_similar(
        &self,
        image_bytes: &[u8],
        org_id: &str,
        dept_id: &str,
        limit: usize,
    ) -> Result<SimilarSearchOutcome> {
        let embedding = self.clip.encode_image(image_bytes).await?;
        let route = self.resolve_read_route(embedding.dimension()).await?;
        tracing::debug!(
            dim = embedding.dimension(),
            collection = %route.collection,
            "style search collection resolved"
        );
        let hits = route
            .store
            .search(&embedding.vector, org_id, dept_id, limit)
            .await?;

        let results = hits
            .into_iter()
            .map(|h| {
                let p = &h.payload;
                ImageHit {
                    image_path: p
                        .get("image_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    style_id: p
                        .get("style_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    score: h.score,
                }
            })
            .collect();

        Ok(SimilarSearchOutcome {
            hits: results,
            provider_used: embedding.provider_str().to_string(),
        })
    }
}

/// Convenience re-export for handler call sites.
pub type StyleId = Option<Uuid>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_vector_dim_follows_provider() {
        // Local CLIP ViT-B/32 → 512.
        assert_eq!(resolve_vector_dim(None, ""), 512);
        assert_eq!(resolve_vector_dim(None, "generic"), 512);
        assert_eq!(resolve_vector_dim(None, "local"), 512);
        // Hybrid: local is the primary provider → 512.
        assert_eq!(resolve_vector_dim(None, "hybrid"), 512);
        // DashScope multimodal-embedding-v1 → 1024 (the corrected default).
        assert_eq!(resolve_vector_dim(None, "dashscope"), 1024);
        assert_eq!(resolve_vector_dim(None, "qwen"), 1024);
        // Unknown values keep the pre-T-027 512 behaviour.
        assert_eq!(resolve_vector_dim(None, "some-future-provider"), 512);
    }

    #[test]
    fn explicit_vector_dim_always_wins() {
        // Highest priority: explicit CLIP_VECTOR_DIM overrides every
        // provider default, including DashScope's 1024.
        assert_eq!(resolve_vector_dim(Some(768), "dashscope"), 768);
        assert_eq!(resolve_vector_dim(Some(512), "dashscope"), 512);
        assert_eq!(resolve_vector_dim(Some(1024), "local"), 1024);
        assert_eq!(resolve_vector_dim(Some(42), "hybrid"), 42);
    }

    #[test]
    fn collection_names_encode_provider_model_and_dim() {
        assert_eq!(
            style_collection_name("local", "clip-vit-base-patch32", 512),
            "style_images_local_clipvitb32_512"
        );
        assert_eq!(
            style_collection_name("dashscope", "multimodal-embedding-v1", 1024),
            "style_images_dashscope_mmembedv1_1024"
        );
        // Unknown models sanitise to ascii-alphanumeric lowercase.
        assert_eq!(
            style_collection_name("local", "Future-Model/2", 256),
            "style_images_local_futuremodel2_256"
        );
    }

    #[test]
    fn generated_collection_names_are_never_protected_legacy() {
        // The naming function cannot produce a protected legacy name for
        // any provider/model/dim combination — pipeline targets derived
        // from it therefore fail the legacy guard only via a bug elsewhere.
        for (provider, model, dim) in [
            ("local", "clip-vit-base-patch32", 512),
            ("dashscope", "multimodal-embedding-v1", 1024),
            ("local", "", 0),
        ] {
            let name = style_collection_name(provider, model, dim);
            assert!(
                !crate::rag::is_protected_legacy(&name),
                "{name} must not be a protected legacy name"
            );
        }
    }

    // ── Dim-routed search/publish (increment 2) ────────────────────────────

    use std::time::Duration;

    use crate::images::clip::{ClipConfig, ClipProviderId};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const LOCAL_COLLECTION: &str = "style_images_local_clipvitb32_512";
    const LEGACY_COLLECTION: &str = "style_images";

    fn fake_vector_value(n: usize) -> serde_json::Value {
        let vals: Vec<f64> = (0..n).map(|i| (i as f64) * 0.001).collect();
        serde_json::to_value(vals).unwrap()
    }

    /// CLIP server serving an embedding of `dim` at /encode/image.
    async fn clip_server(dim: usize) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/encode/image"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embedding": fake_vector_value(dim), "dim": dim,
            })))
            .mount(&server)
            .await;
        server
    }

    /// Qdrant responding to search on both routes.
    async fn qdrant_search_server() -> MockServer {
        let server = MockServer::start().await;
        for collection in [LOCAL_COLLECTION, LEGACY_COLLECTION] {
            Mock::given(method("POST"))
                .and(path(format!("/collections/{collection}/points/search")))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"result": []})),
                )
                .mount(&server)
                .await;
        }
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

    /// Hybrid-route service: 512 writable new collection, 1024 read-only
    /// legacy.
    fn service_with_hybrid_routes(clip: ClipClient, qdrant_url: &str) -> ImageSearchService {
        ImageSearchService::new(
            clip,
            qdrant_url,
            vec![
                (512, LOCAL_COLLECTION.into(), true),
                (1024, LEGACY_COLLECTION.into(), false),
            ],
        )
    }

    #[tokio::test]
    async fn search_routes_512_vector_to_dim_named_collection() {
        let clip_server = clip_server(512).await;
        let qdrant = qdrant_search_server().await;

        let service = service_with_hybrid_routes(local_client(&clip_server.uri()), &qdrant.uri());
        let outcome = service
            .search_similar(b"x", "org", "dept", 5)
            .await
            .expect("512 search");
        assert_eq!(outcome.provider_used, ClipProviderId::Local.as_str());

        let seen = qdrant.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0]
                .url
                .path()
                .ends_with("/collections/style_images_local_clipvitb32_512/points/search"),
            "512 query must reach the 512 collection: {}",
            seen[0].url.path()
        );
    }

    #[tokio::test]
    async fn search_routes_1024_vector_to_legacy_collection() {
        // Simulates a DashScope-fallback embedding (1024): it must search
        // the legacy collection, still read-only.
        let clip_server = clip_server(1024).await;
        let qdrant = qdrant_search_server().await;

        let service = service_with_hybrid_routes(local_client(&clip_server.uri()), &qdrant.uri());
        let outcome = service
            .search_similar(b"x", "org", "dept", 5)
            .await
            .expect("1024 search");
        assert_eq!(outcome.provider_used, ClipProviderId::Local.as_str());

        let seen = qdrant.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0]
                .url
                .path()
                .ends_with("/collections/style_images/points/search"),
            "1024 query must reach the legacy collection: {}",
            seen[0].url.path()
        );
        assert!(
            !seen[0].url.path().contains("clipvitb32"),
            "1024 query must not reach the 512 collection"
        );
    }

    #[tokio::test]
    async fn publish_with_1024_vector_is_refused_on_readonly_route() {
        let qdrant = MockServer::start().await;
        let service =
            service_with_hybrid_routes(local_client("http://127.0.0.1:18399"), &qdrant.uri());

        let err = service
            .publish_raw_point(RawPoint {
                id: "1".into(),
                vector: vec![0.1; 1024],
                payload: serde_json::json!({}),
            })
            .await
            .expect_err("legacy route must refuse writes");
        assert!(format!("{err:#}").contains("read-only/legacy"), "{err:#}");
        assert_eq!(
            qdrant.received_requests().await.unwrap().len(),
            0,
            "no HTTP write may leave"
        );
    }

    #[tokio::test]
    async fn unknown_dim_fails_closed_for_search_and_publish() {
        let clip_server = clip_server(768).await;
        let qdrant = qdrant_search_server().await;

        let service = service_with_hybrid_routes(local_client(&clip_server.uri()), &qdrant.uri());
        let err = service
            .search_similar(b"x", "org", "dept", 5)
            .await
            .expect_err("768 has no route");
        assert!(
            format!("{err:#}").contains("no style collection route for vector dim 768"),
            "{err:#}"
        );

        let err = service
            .publish_raw_point(RawPoint {
                id: "2".into(),
                vector: vec![0.1; 768],
                payload: serde_json::json!({}),
            })
            .await
            .expect_err("768 publish has no route");
        assert!(
            format!("{err:#}").contains("no style collection route"),
            "{err:#}"
        );
        assert_eq!(qdrant.received_requests().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn ensure_collection_metadata_provider_follows_dashscope_mode() {
        // F2: in DashScope mode a fresh-env creation (GET 404 → PUT) must
        // stamp the collection metadata with the actually-effective
        // provider, never the hardcoded "local".
        let qdrant = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/collections/style_images"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&qdrant)
            .await;
        Mock::given(method("PUT"))
            .and(path("/collections/style_images"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": true, "status": "ok",
            })))
            .mount(&qdrant)
            .await;

        let clip = ClipClient::new(ClipConfig {
            provider: "dashscope".into(),
            api_endpoint: "http://127.0.0.1:9".into(),
            api_key: String::new(),
            model: String::new(),
            local_base_url: String::new(),
            local_timeout: Duration::from_secs(5),
        });
        let service = ImageSearchService::new(
            clip,
            &qdrant.uri(),
            vec![(1024, "style_images".into(), true)],
        );
        service
            .ensure_collection()
            .await
            .expect("ensure in dashscope mode");

        let put = qdrant
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.method.to_string() == "PUT")
            .expect("a PUT create request must be sent");
        let body: serde_json::Value = serde_json::from_slice(&put.body).unwrap();
        assert_eq!(body["vectors"]["size"], 1024);
        assert_eq!(body["metadata"]["vector_dim"], 1024);
        assert_eq!(body["metadata"]["provider"], "dashscope");
        assert_eq!(body["metadata"]["model"], "multimodal-embedding-v1");
    }

    // ── Increment 3 phase C: pure route table ──────────────────────────────

    const DASH_NAMED_1024: &str = "style_images_dashscope_mmembedv1_1024";
    const GENERIC_NAMED_512: &str = "style_images_generic_clipvitb32_512";
    const GENERIC_NAMED_1024: &str = "style_images_generic_clipvitb32_1024";

    #[test]
    fn route_specs_dashscope_targets_named_collection_with_legacy_fallback() {
        let specs = route_specs(ClipMode::DashScope, "multimodal-embedding-v1", 1024);
        assert_eq!(
            specs,
            vec![
                (1024, DASH_NAMED_1024.into(), true),
                (1024, LEGACY_COLLECTION.into(), false),
            ]
        );
    }

    #[test]
    fn route_specs_generic_512_is_named_only() {
        // Default generic mode (512): legacy fallback is dimensionally
        // incompatible so it must not be bound.
        let specs = route_specs(ClipMode::Generic, "clip-vit-base-patch32", 512);
        assert_eq!(specs, vec![(512, GENERIC_NAMED_512.into(), true)]);
    }

    #[test]
    fn route_specs_generic_1024_gets_legacy_fallback() {
        // Explicit CLIP_VECTOR_DIM=1024 generic deployment.
        let specs = route_specs(ClipMode::Generic, "clip-vit-base-patch32", 1024);
        assert_eq!(
            specs,
            vec![
                (1024, GENERIC_NAMED_1024.into(), true),
                (1024, LEGACY_COLLECTION.into(), false),
            ]
        );
    }

    #[test]
    fn route_specs_local_and_hybrid_keep_increment2_table() {
        let local = route_specs(ClipMode::Local, "clip-vit-base-patch32", 512);
        assert_eq!(local, vec![(512, LOCAL_COLLECTION.into(), true)]);
        let hybrid = route_specs(ClipMode::Hybrid, "clip-vit-base-patch32", 512);
        assert_eq!(
            hybrid,
            vec![
                (512, LOCAL_COLLECTION.into(), true),
                (1024, LEGACY_COLLECTION.into(), false),
            ],
            "hybrid's 1024 fallback route stays on the legacy collection"
        );
    }

    // ── Increment 3 phase C: transitional search two states ────────────────

    /// Two-route 1024 service: writable named collection + read-only
    /// legacy fallback (DashScope/Generic 1024 table).
    fn service_with_transition_routes(clip: ClipClient, url: &str) -> ImageSearchService {
        ImageSearchService::new(
            clip,
            url,
            vec![
                (1024, DASH_NAMED_1024.into(), true),
                (1024, LEGACY_COLLECTION.into(), false),
            ],
        )
    }

    fn collection_info_with_count(count: u64) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "result": {"points_count": count},
        }))
    }

    fn search_hit_response(image_path: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "result": [{
                "id": "p1", "score": 0.91,
                "payload": {"image_path": image_path, "style_id": null},
            }]
        }))
    }

    #[tokio::test]
    async fn search_prefers_named_collection_when_nonempty() {
        let clip_server = clip_server(1024).await;
        let qdrant = MockServer::start().await;

        // Named collection: populated and serving a hit.
        Mock::given(method("GET"))
            .and(path(format!("/collections/{DASH_NAMED_1024}")))
            .respond_with(collection_info_with_count(2))
            .mount(&qdrant)
            .await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/collections/{DASH_NAMED_1024}/points/search"
            )))
            .respond_with(search_hit_response("from-new.jpg"))
            .mount(&qdrant)
            .await;

        let service =
            service_with_transition_routes(local_client(&clip_server.uri()), &qdrant.uri());
        let outcome = service
            .search_similar(b"x", "org", "dept", 5)
            .await
            .expect("search via named collection");
        assert_eq!(outcome.hits.len(), 1);
        assert_eq!(outcome.hits[0].image_path, "from-new.jpg");

        let seen = qdrant.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2, "points-count GET + search POST");
        assert!(seen[0]
            .url
            .path()
            .ends_with(&format!("/collections/{DASH_NAMED_1024}")));
        assert!(
            seen[1]
                .url
                .path()
                .ends_with(&format!("/collections/{DASH_NAMED_1024}/points/search")),
            "search must target the named collection: {}",
            seen[1].url.path()
        );
        assert!(
            seen.iter()
                .all(|r| !r.url.path().contains("/collections/style_images/")),
            "legacy collection must not be touched while the new one is populated"
        );
    }

    #[tokio::test]
    async fn search_falls_back_to_legacy_when_named_missing() {
        let clip_server = clip_server(1024).await;
        let qdrant = MockServer::start().await;

        // Named collection does not exist (GET 404).
        Mock::given(method("GET"))
            .and(path(format!("/collections/{DASH_NAMED_1024}")))
            .respond_with(ResponseTemplate::new(404))
            .mount(&qdrant)
            .await;
        // Legacy serves the hit.
        Mock::given(method("POST"))
            .and(path("/collections/style_images/points/search"))
            .respond_with(search_hit_response("from-legacy.jpg"))
            .mount(&qdrant)
            .await;

        let service =
            service_with_transition_routes(local_client(&clip_server.uri()), &qdrant.uri());
        let outcome = service
            .search_similar(b"x", "org", "dept", 5)
            .await
            .expect("search fails-safe to legacy");
        assert_eq!(outcome.hits[0].image_path, "from-legacy.jpg");

        let seen = qdrant.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2, "points-count GET + legacy search POST");
        assert!(
            seen[1]
                .url
                .path()
                .ends_with("/collections/style_images/points/search"),
            "search must target the legacy fallback: {}",
            seen[1].url.path()
        );
    }

    #[tokio::test]
    async fn search_falls_back_to_legacy_when_named_empty() {
        let clip_server = clip_server(1024).await;
        let qdrant = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path(format!("/collections/{DASH_NAMED_1024}")))
            .respond_with(collection_info_with_count(0))
            .mount(&qdrant)
            .await;
        Mock::given(method("POST"))
            .and(path("/collections/style_images/points/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": []
            })))
            .mount(&qdrant)
            .await;

        let service =
            service_with_transition_routes(local_client(&clip_server.uri()), &qdrant.uri());
        let outcome = service
            .search_similar(b"x", "org", "dept", 5)
            .await
            .expect("empty named collection → legacy route");

        let seen = qdrant.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2);
        assert!(
            seen[1]
                .url
                .path()
                .ends_with("/collections/style_images/points/search"),
            "{}",
            seen[1].url.path()
        );
        assert!(outcome.hits.is_empty());
    }

    #[tokio::test]
    async fn publish_1024_routes_to_writable_named_collection() {
        let qdrant = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path(format!("/collections/{DASH_NAMED_1024}/points")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {"operation_id": 0, "status": "completed"},
                "status": "ok",
            })))
            .mount(&qdrant)
            .await;

        let service =
            service_with_transition_routes(local_client("http://127.0.0.1:18399"), &qdrant.uri());
        service
            .publish_raw_point(RawPoint {
                id: "p1".into(),
                vector: vec![0.1; 1024],
                payload: serde_json::json!({"image_path": "x.jpg"}),
            })
            .await
            .expect("write into named collection");

        let seen = qdrant.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0]
                .url
                .path()
                .ends_with("/collections/style_images_dashscope_mmembedv1_1024/points"),
            "writes must target the named collection: {}",
            seen[0].url.path()
        );
    }
}
