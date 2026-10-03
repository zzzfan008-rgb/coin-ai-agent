//! Route regression tests — skill endpoints (P0-2 / T-022).
//!
//! Loads the real repo `skills/` directory into the process-global engine
//! and initialises Casbin RBAC, then asserts:
//!   - GET /internal/skills/list returns the real loaded skills
//!   - POST /internal/skills/execute with an unknown skill → 404
//!   - POST /internal/skills/execute without permission → 403

mod common;

use api_core::skill_engine::{SkillEngine, SKILL_ENGINE};

/// Load the production skill engine (repo `skills/`) and the production
/// Casbin RBAC service into the process globals, exactly like `run()` does
/// at boot. Safe to call from every test in this binary.
async fn init_globals() {
    let engine = match SkillEngine::load_from_dir(&common::repo_skills_dir()) {
        Ok(engine) => engine,
        Err(e) => panic!("failed to load skill engine from repo skills/: {e}"),
    };
    {
        let mut global = SKILL_ENGINE.write().expect("SKILL_ENGINE poisoned");
        *global = Some(engine);
    }
    // OnceCell: first test wins; later calls are no-ops.
    let _ =
        api_core::rbac::RBAC_SERVICE.set(api_core::rbac::RbacService::new().await.expect("rbac"));
}

/// Skills list must serve the skills actually loaded from disk —
/// the exact ids the Casbin policies grant (T-016/T-017).
#[tokio::test]
async fn skills_list_returns_loaded_engine_skills() {
    init_globals().await;
    let base = common::spawn_app(common::test_state().await).await;

    let resp = common::client()
        .get(format!("{base}/internal/skills/list"))
        .send()
        .await
        .expect("GET /internal/skills/list");
    assert_eq!(resp.status().as_u16(), 200);

    let body: serde_json::Value = resp.json().await.expect("skills JSON");
    let skills = body["skills"]
        .as_array()
        .unwrap_or_else(|| panic!("skills must be an array, got: {body}"));
    assert!(
        !skills.is_empty(),
        "engine loaded but skills list empty: {body}"
    );

    let ids: Vec<&str> = skills.iter().filter_map(|s| s["id"].as_str()).collect();
    for expected in ["fabric-query", "color-matching", "style-inspiration"] {
        assert!(
            ids.contains(&expected),
            "skill {expected:?} missing from list, got ids: {ids:?}"
        );
    }
    // Every summary carries the contract fields the web client renders.
    for s in skills {
        assert!(s["version"].is_string(), "skill missing version: {s}");
        assert!(s["tools"].is_array(), "skill missing tools array: {s}");
    }
}

/// Unknown skill must be 404 (not 500, not silent success) even for admin.
#[tokio::test]
async fn execute_unknown_skill_returns_404() {
    init_globals().await;
    let base = common::spawn_app(common::test_state().await).await;

    let body = serde_json::json!({
        "skill_id": "no-such-skill",
        "tool_name": "run",
        "parameters": {},
        "user_context": common::user_context_json("admin"),
    });
    let resp = common::client()
        .post(format!("{base}/internal/skills/execute"))
        .json(&body)
        .send()
        .await
        .expect("POST /internal/skills/execute");

    assert_eq!(
        resp.status().as_u16(),
        404,
        "unknown skill must be 404, body: {}",
        resp.text().await.unwrap_or_default()
    );
}

/// Viewer has no `skill:fabric-query` policy in Casbin — the request must
/// be refused with 403 before any execution happens.
#[tokio::test]
async fn execute_skill_without_permission_returns_403() {
    init_globals().await;
    let base = common::spawn_app(common::test_state().await).await;

    let body = serde_json::json!({
        "skill_id": "fabric-query",
        "tool_name": "run_db_query",
        "parameters": {"sql": "SELECT 1"},
        "user_context": common::user_context_json("viewer"),
    });
    let resp = common::client()
        .post(format!("{base}/internal/skills/execute"))
        .json(&body)
        .send()
        .await
        .expect("POST /internal/skills/execute");

    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    assert_eq!(status, 403, "viewer must get 403, body: {text}");
    assert!(
        text.contains("not allowed"),
        "403 body must explain the denial, got: {text}"
    );
}
