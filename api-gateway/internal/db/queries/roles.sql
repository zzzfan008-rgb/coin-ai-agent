-- name: GetRoleByID :one
SELECT id, org_id, name, permissions, description, is_system, created_at, updated_at
FROM roles
WHERE id = $1;

-- name: ListRolesByOrg :many
SELECT id, org_id, name, permissions, description, is_system, created_at, updated_at
FROM roles
WHERE org_id = $1
ORDER BY is_system DESC, name;

-- name: GetRoleByName :one
SELECT id, org_id, name, permissions, description, is_system, created_at, updated_at
FROM roles
WHERE org_id = $1 AND name = $2;

-- name: CreateRole :one
INSERT INTO roles (org_id, name, permissions, description)
VALUES ($1, $2, $3, $4)
RETURNING id, org_id, name, permissions, description, is_system, created_at, updated_at;

-- name: UpdateRole :one
UPDATE roles
SET permissions = COALESCE($2, permissions),
    description = COALESCE($3, description),
    updated_at  = NOW()
WHERE id = $1 AND is_system = false
RETURNING id, org_id, name, permissions, description, is_system, created_at, updated_at;

-- name: DeleteRole :exec
DELETE FROM roles WHERE id = $1 AND is_system = false;
