-- name: GetMCPServerByID :one
SELECT id, org_id, name, server_type, endpoint, auth_token, env_vars,
       is_active, created_by, created_at, updated_at
FROM mcp_servers
WHERE id = $1;

-- name: ListMCPServersByOrg :many
SELECT id, org_id, name, server_type, endpoint, auth_token, env_vars,
       is_active, created_by, created_at, updated_at
FROM mcp_servers
WHERE org_id = $1 AND is_active = true
ORDER BY name;

-- name: GetMCPServerByName :one
SELECT id, org_id, name, server_type, endpoint, auth_token, env_vars,
       is_active, created_by, created_at, updated_at
FROM mcp_servers
WHERE org_id = $1 AND name = $2;

-- name: CreateMCPServer :one
INSERT INTO mcp_servers (org_id, name, server_type, endpoint, auth_token,
                         env_vars, created_by)
VALUES ($1, $2, $3, $4, $5, $6, $7)
RETURNING id, org_id, name, server_type, endpoint, auth_token, env_vars,
          is_active, created_by, created_at, updated_at;

-- name: UpdateMCPServer :one
UPDATE mcp_servers
SET server_type  = COALESCE($2, server_type),
    endpoint     = COALESCE($3, endpoint),
    auth_token   = COALESCE($4, auth_token),
    env_vars     = COALESCE($5, env_vars),
    is_active    = COALESCE($6, is_active),
    updated_at  = NOW()
WHERE id = $1
RETURNING id, org_id, name, server_type, endpoint, auth_token, env_vars,
          is_active, created_by, created_at, updated_at;

-- name: DeactivateMCPServer :exec
UPDATE mcp_servers SET is_active = false, updated_at = NOW() WHERE id = $1;
