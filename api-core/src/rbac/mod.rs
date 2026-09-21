//! Casbin RBAC service — PreToolCall permission enforcement (T-017).
//!
//! Wraps a Casbin [`Enforcer`] and exposes a synchronous 5-field permission
//! check `(user_id, role, dept_id, resource, action)`.
//!
//! The model and seed policies are embedded at compile time (`model.conf` /
//! `policies.csv`) and copied into an in-memory [`StringAdapter`], so the
//! enforcer needs no external files at runtime.
//!
//! Two ways to reach the service:
//!   - [`AppServices::rbac`] — injected into HTTP handlers via `AppState`;
//!   - [`RBAC_SERVICE`] global — used by detached agent-loop free functions
//!     (`skill_engine::executor::route_tool_call`) that have no `State`.

#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use casbin::prelude::{CoreApi, DefaultModel, Enforcer, StringAdapter};
use once_cell::sync::OnceCell;

use crate::api::handlers::UserContext;
use crate::error::{AppError, Result};

/// Embedded Casbin model text (5-tuple request definition).
const MODEL_CONF: &str = include_str!("model.conf");
/// Embedded seed policies.
const POLICIES_CSV: &str = include_str!("policies.csv");

/// Process-global RBAC service, initialised once in `main` before the server
/// starts. Mirrors the `SKILL_ENGINE` global pattern.
pub static RBAC_SERVICE: OnceCell<RbacService> = OnceCell::new();

/// Obtain the initialised global RBAC service.
pub fn service() -> Option<&'static RbacService> {
    RBAC_SERVICE.get()
}

/// Thread-safe wrapper around a Casbin [`Enforcer`].
///
/// Cheap to clone (the enforcer sits behind an [`Arc`]).
#[derive(Clone)]
pub struct RbacService {
    enforcer: Arc<RwLock<Enforcer>>,
}

impl RbacService {
    /// Build the service from the embedded `model.conf` + `policies.csv`.
    pub async fn new() -> anyhow::Result<Self> {
        Self::from_strings(MODEL_CONF, POLICIES_CSV).await
    }

    /// Build the service from explicit model and policy text.
    pub async fn from_strings(model_conf: &str, policies_csv: &str) -> anyhow::Result<Self> {
        let model = DefaultModel::from_str(model_conf).await?;
        let adapter = StringAdapter::new(policies_csv);
        let enforcer = Enforcer::new(model, adapter).await?;
        tracing::info!("Casbin enforcer initialised (5-tuple RBAC model)");
        Ok(Self {
            enforcer: Arc::new(RwLock::new(enforcer)),
        })
    }

    /// Check whether a request is allowed.
    ///
    /// Fields map to the request definition `(sub, role, dept, obj, act)`:
    ///   - `user_id`  — subject (policies use `*` to match any user)
    ///   - `role`     — user's role from [`UserContext`]
    ///   - `dept_id`  — department id (`*` matches any dept)
    ///   - `resource` — e.g. `"skill:fabric-query"`, `"data:conversations"`
    ///   - `action`   — e.g. `"execute"`, `"read"`
    ///
    /// Fails *closed*: a poisoned lock or an enforcer error returns `false`.
    pub fn check_permission(
        &self,
        user_id: &str,
        role: &str,
        dept_id: &str,
        resource: &str,
        action: &str,
    ) -> bool {
        let guard = match self.enforcer.read() {
            Ok(g) => g,
            Err(_) => {
                tracing::error!("RbacService lock poisoned — denying request");
                return false;
            }
        };

        match guard.enforce((user_id, role, dept_id, resource, action)) {
            Ok(allowed) => {
                if !allowed {
                    tracing::warn!(
                        user_id = %user_id,
                        role = %role,
                        dept_id = %dept_id,
                        resource = %resource,
                        action = %action,
                        "Casbin denied"
                    );
                }
                allowed
            }
            Err(e) => {
                tracing::error!(error = %e, "Casbin enforce error — denying request");
                false
            }
        }
    }

    /// Reload policies from the embedded CSV and swap in a fresh enforcer.
    pub async fn reload_policy(&self) -> anyhow::Result<()> {
        let model = DefaultModel::from_str(MODEL_CONF).await?;
        let adapter = StringAdapter::new(POLICIES_CSV);
        let fresh = Enforcer::new(model, adapter).await?;

        let mut guard = self
            .enforcer
            .write()
            .map_err(|_| anyhow::anyhow!("RbacService lock poisoned"))?;
        *guard = fresh;
        tracing::info!("Casbin policies reloaded");
        Ok(())
    }
}

/// Enforce skill execution for a [`UserContext`].
///
/// `resource = "skill:{skill_id}"`, `action = "execute"`. Returns
/// [`AppError::Forbidden`] when the enforcer denies or is unavailable,
/// i.e. the gate is fail-closed.
pub fn enforce_skill(user_ctx: &UserContext, skill_id: &str) -> Result<()> {
    let resource = format!("skill:{skill_id}");
    let rbac = service().ok_or_else(|| {
        AppError::Internal("RBAC service not initialised".to_string())
    })?;

    if rbac.check_permission(
        &user_ctx.user_id,
        &user_ctx.role,
        &user_ctx.dept_id,
        &resource,
        "execute",
    ) {
        Ok(())
    } else {
        Err(AppError::Forbidden(format!(
            "User '{}' (role: '{}') is not allowed to execute skill '{}'",
            user_ctx.user_id, user_ctx.role, skill_id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(user: &str, role: &str, dept: &str) -> UserContext {
        UserContext {
            user_id: user.into(),
            org_id: "org1".into(),
            dept_id: dept.into(),
            role: role.into(),
            ip_address: None,
            extra_permissions: vec![],
        }
    }

    async fn svc() -> RbacService {
        RbacService::new().await.unwrap()
    }

    #[tokio::test]
    async fn admin_allowed_everything() {
        let s = svc().await;
        assert!(s.check_permission("u1", "admin", "d1", "skill:fabric-query", "execute"));
        assert!(s.check_permission("u2", "admin", "d2", "data:conversations", "delete"));
        assert!(s.check_permission("u3", "admin", "d9", "anything:at-all", "write"));
    }

    #[tokio::test]
    async fn designer_allowed_only_designer_skills() {
        let s = svc().await;
        assert!(s.check_permission("u1", "designer", "d1", "skill:fabric-query", "execute"));
        assert!(s.check_permission("u2", "designer", "d2", "skill:color-matching", "execute"));
        assert!(s.check_permission("u3", "designer", "d1", "skill:style-inspiration", "execute"));

        assert!(!s.check_permission("u1", "designer", "d1", "skill:other", "execute"));
        assert!(!s.check_permission("u1", "designer", "d1", "data:conversations", "read"));
    }

    #[tokio::test]
    async fn viewer_read_only() {
        let s = svc().await;
        assert!(s.check_permission("u1", "viewer", "d1", "data:conversations", "read"));
        assert!(!s.check_permission("u1", "viewer", "d1", "data:conversations", "write"));
        assert!(!s.check_permission("u1", "viewer", "d1", "skill:fabric-query", "execute"));
    }

    #[tokio::test]
    async fn wildcard_user_matches_any_subject() {
        let s = svc().await;
        assert!(s.check_permission("any-user-id", "admin", "any-dept", "skill:x", "execute"));
    }

    #[tokio::test]
    async fn reload_keeps_policy_valid() {
        let s = svc().await;
        s.reload_policy().await.unwrap();
        assert!(s.check_permission("u1", "designer", "d1", "skill:fabric-query", "execute"));
        assert!(!s.check_permission("u1", "viewer", "d1", "skill:fabric-query", "execute"));
    }

    #[test]
    fn enforce_skill_fails_closed_when_global_unset() {
        // RBAC_SERVICE is process-global and may be set by another test, so
        // only assert the error shape when it is genuinely unset.
        if service().is_none() {
            let err = enforce_skill(&ctx("u", "designer", "d"), "fabric-query").unwrap_err();
            assert!(matches!(err, AppError::Internal(_)));
        }
    }
}
