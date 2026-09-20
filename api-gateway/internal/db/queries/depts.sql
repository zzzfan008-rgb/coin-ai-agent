-- name: GetDeptByID :one
SELECT id, org_id, parent_id, name, path, created_at, updated_at
FROM depts
WHERE id = $1;

-- name: ListDeptsByOrg :many
SELECT id, org_id, parent_id, name, path, created_at, updated_at
FROM depts
WHERE org_id = $1
ORDER BY path;

-- name: ListDeptsByParent :many
SELECT id, org_id, parent_id, name, path, created_at, updated_at
FROM depts
WHERE parent_id = $1
ORDER BY name;

-- name: GetDeptChildren :many
-- Get all descendant depts by path prefix (e.g. /root/design-team/%)
SELECT id, org_id, parent_id, name, path, created_at, updated_at
FROM depts
WHERE org_id = $1 AND path LIKE ($2 || '/%')
ORDER BY path;

-- name: CreateDept :one
INSERT INTO depts (org_id, parent_id, name, path)
VALUES ($1, $2, $3, $4)
RETURNING id, org_id, parent_id, name, path, created_at, updated_at;

-- name: UpdateDept :one
UPDATE depts
SET name = COALESCE($2, name),
    parent_id = COALESCE($3, parent_id),
    path = COALESCE($4, path),
    updated_at = NOW()
WHERE id = $1
RETURNING id, org_id, parent_id, name, path, created_at, updated_at;

-- name: DeleteDept :exec
DELETE FROM depts WHERE id = $1;
