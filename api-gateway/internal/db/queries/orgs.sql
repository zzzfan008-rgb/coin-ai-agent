-- name: GetOrgByID :one
-- Get a single org by ID.
SELECT id, name, slug, is_active, created_at, updated_at
FROM orgs
WHERE id = $1;

-- name: GetOrgBySlug :one
-- Get a single org by slug.
SELECT id, name, slug, is_active, created_at, updated_at
FROM orgs
WHERE slug = $1;

-- name: ListOrgs :many
-- List all active orgs.
SELECT id, name, slug, is_active, created_at, updated_at
FROM orgs
WHERE is_active = true
ORDER BY name;

-- name: CreateOrg :one
-- Create a new org. Returns the inserted row.
INSERT INTO orgs (name, slug)
VALUES ($1, $2)
RETURNING id, name, slug, is_active, created_at, updated_at;

-- name: UpdateOrg :one
-- Update org name and/or active status.
UPDATE orgs
SET name = COALESCE($2, name),
    is_active = COALESCE($3, is_active),
    updated_at = NOW()
WHERE id = $1
RETURNING id, name, slug, is_active, created_at, updated_at;
