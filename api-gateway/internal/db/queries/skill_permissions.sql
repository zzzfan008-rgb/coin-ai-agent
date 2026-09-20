-- name: CheckSkillPermission :one
-- Check if a user or their dept has a given permission on a skill.
SELECT id FROM skill_permissions
WHERE org_id       = $1
  AND skill_id     = $2
  AND (user_id     = $3 OR dept_id = $4)
  AND permission   = $5
  AND (expires_at  IS NULL OR expires_at > NOW());

-- name: ListSkillPermissionsByUser :many
-- List all skill permissions for a specific user (personal + dept-level).
SELECT sp.id, sp.org_id, sp.dept_id, sp.user_id, sp.skill_id,
       sp.permission, sp.granted_by, sp.expires_at, sp.created_at,
       s.name AS skill_name
FROM skill_permissions sp
JOIN skills s ON s.id = sp.skill_id
WHERE sp.org_id = $1
  AND (sp.user_id = $2 OR sp.dept_id = $3)
ORDER BY s.name;

-- name: GrantSkillPermission :one
INSERT INTO skill_permissions (org_id, dept_id, user_id, skill_id, permission, granted_by, expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
RETURNING id, org_id, dept_id, user_id, skill_id, permission, granted_by, expires_at, created_at;

-- name: RevokeSkillPermission :exec
DELETE FROM skill_permissions WHERE id = $1;

-- name: RevokeSkillPermissionByTarget :exec
DELETE FROM skill_permissions
WHERE org_id = $1 AND skill_id = $2
  AND (user_id = $3 OR dept_id = $4);
