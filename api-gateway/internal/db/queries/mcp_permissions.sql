-- name: CheckMCPPermission :one
SELECT id FROM mcp_permissions
WHERE org_id         = $1
  AND mcp_server_id = $2
  AND (user_id      = $3 OR dept_id = $4)
  AND permission    = $5
  AND (expires_at   IS NULL OR expires_at > NOW());

-- name: ListMCPPermissionsByUser :many
SELECT mp.id, mp.org_id, mp.dept_id, mp.user_id, mp.mcp_server_id,
       mp.permission, mp.granted_by, mp.expires_at, mp.created_at,
       ms.name AS mcp_server_name
FROM mcp_permissions mp
JOIN mcp_servers ms ON ms.id = mp.mcp_server_id
WHERE mp.org_id = $1
  AND (mp.user_id = $2 OR mp.dept_id = $3)
ORDER BY ms.name;

-- name: GrantMCPPermission :one
INSERT INTO mcp_permissions (org_id, dept_id, user_id, mcp_server_id, permission, granted_by, expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
RETURNING id, org_id, dept_id, user_id, mcp_server_id, permission, granted_by, expires_at, created_at;

-- name: RevokeMCPPermission :exec
DELETE FROM mcp_permissions WHERE id = $1;

-- name: RevokeMCPPermissionByTarget :exec
DELETE FROM mcp_permissions
WHERE org_id = $1 AND mcp_server_id = $2
  AND (user_id = $3 OR dept_id = $4);
