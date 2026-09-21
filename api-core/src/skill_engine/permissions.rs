//! Permission checking for skill tools.
//!
//! Reads the `permissions` field from a loaded SKILL.md and verifies the
//! requesting user holds the corresponding `skill:{id}` permission.
//!
//! The permission string format is `skill:{skill_id}` (e.g. `skill:fabric-query`).
//! User permissions are expected to be in `UserContext.metadata` or a JWT claim.
//! Phase 1B uses a simple allowlist check; Phase 2 swaps in Casbin enforcement.

use std::collections::HashSet;

use crate::api::handlers::UserContext;
use crate::error::{AppError, Result};
use crate::skill_engine::SkillMetadata;

/// Check whether `user_ctx` is allowed to invoke `skill_meta`.
///
/// Permission logic (Phase 1B):
///   - Admin role bypasses all permission checks
///   - Otherwise the user must hold `skill:<skill_id>` in their permission set
///
/// The user's permission set is read from `UserContext` — in Phase 1B we
/// extract it from the `permissions` field; Phase 2 will read from JWT
/// claims or a DB lookup.
pub fn check_skill_permission(
    user_ctx: &UserContext,
    skill_meta: &SkillMetadata,
) -> Result<()> {
    // Admin always allowed.
    if user_ctx.role == "admin" {
        return Ok(());
    }

    let required = format!("skill:{}", skill_meta.id);

    // Build the user's permission set.
    // Phase 1B: read from user_ctx.extra_permissions (serde default if absent).
    let user_permissions: HashSet<String> = user_ctx
        .extra_permissions
        .iter()
        .cloned()
        .collect();

    if user_permissions.contains(&required) {
        tracing::debug!(
            user_id = %user_ctx.user_id,
            skill_id = %skill_meta.id,
            "Skill permission granted"
        );
        Ok(())
    } else {
        tracing::warn!(
            user_id = %user_ctx.user_id,
            role = %user_ctx.role,
            skill_id = %skill_meta.id,
            required_permission = %required,
            "Skill permission denied"
        );
        Err(AppError::Forbidden(format!(
            "User '{}' (role: '{}') lacks permission '{}' required to invoke skill '{}'",
            user_ctx.user_id, user_ctx.role, required, skill_meta.id
        )))
    }
}

/// Check whether a user has permission for a specific tool within a skill.
pub fn check_tool_permission(
    user_ctx: &UserContext,
    skill_meta: &SkillMetadata,
    tool_name: &str,
) -> Result<()> {
    // First check the skill-level permission.
    check_skill_permission(user_ctx, skill_meta)?;

    // Check tool-level permission.
    let required = format!("tool:{}", tool_name);
    let user_permissions: HashSet<String> = user_ctx.extra_permissions.iter().cloned().collect();

    // Admin bypasses tool-level checks.
    if user_ctx.role == "admin" {
        return Ok(());
    }

    if user_permissions.contains(&required) {
        Ok(())
    } else {
        Err(AppError::Forbidden(format!(
            "User '{}' lacks tool permission '{}'",
            user_ctx.user_id, required
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(role: &str, permissions: Vec<String>) -> UserContext {
        UserContext {
            user_id: "user_001".to_string(),
            org_id: "org_001".to_string(),
            dept_id: "dept_001".to_string(),
            role: role.to_string(),
            ip_address: None,
            extra_permissions: permissions,
        }
    }

    fn make_skill(id: &str) -> SkillMetadata {
        SkillMetadata {
            id: id.to_string(),
            name: id.to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            long_description: None,
            author: None,
            tags: vec![],
            category: None,
            permissions: vec![format!("skill:{id}")],
            dependencies: vec![],
            tools: vec![],
        }
    }

    #[test]
    fn test_admin_bypasses() {
        let ctx = make_ctx("admin", vec![]);
        let skill = make_skill("fabric-query");
        assert!(check_skill_permission(&ctx, &skill).is_ok());
    }

    #[test]
    fn test_has_required_permission() {
        let ctx = make_ctx("designer", vec!["skill:fabric-query".to_string()]);
        let skill = make_skill("fabric-query");
        assert!(check_skill_permission(&ctx, &skill).is_ok());
    }

    #[test]
    fn test_missing_permission() {
        let ctx = make_ctx("designer", vec!["skill:other-skill".to_string()]);
        let skill = make_skill("fabric-query");
        let result = check_skill_permission(&ctx, &skill);
        assert!(result.is_err());
        match result {
            Err(AppError::Forbidden(msg)) => {
                assert!(msg.contains("skill:fabric-query"));
            }
            _ => panic!("expected Forbidden"),
        }
    }

    #[test]
    fn test_empty_permissions() {
        let ctx = make_ctx("viewer", vec![]);
        let skill = make_skill("fabric-query");
        let result = check_skill_permission(&ctx, &skill);
        assert!(result.is_err());
    }
}
