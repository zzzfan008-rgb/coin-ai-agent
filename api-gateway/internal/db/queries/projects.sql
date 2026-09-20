-- name: GetProjectByID :one
SELECT id, org_id, dept_id, user_id, name, description, season, collection_year,
       tags, is_archived, created_at, updated_at
FROM projects
WHERE id = $1;

-- name: ListProjectsByOrg :many
SELECT id, org_id, dept_id, user_id, name, description, season, collection_year,
       tags, is_archived, created_at, updated_at
FROM projects
WHERE org_id = $1 AND is_archived = false
ORDER BY collection_year DESC, season, name;

-- name: ListProjectsByUser :many
SELECT id, org_id, dept_id, user_id, name, description, season, collection_year,
       tags, is_archived, created_at, updated_at
FROM projects
WHERE user_id = $1 AND is_archived = false
ORDER BY collection_year DESC, season, name;

-- name: ListProjectsByDept :many
SELECT id, org_id, dept_id, user_id, name, description, season, collection_year,
       tags, is_archived, created_at, updated_at
FROM projects
WHERE dept_id = $1 AND is_archived = false
ORDER BY collection_year DESC, season, name;

-- name: CreateProject :one
INSERT INTO projects (org_id, dept_id, user_id, name, description, season,
                      collection_year, tags)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
RETURNING id, org_id, dept_id, user_id, name, description, season, collection_year,
          tags, is_archived, created_at, updated_at;

-- name: UpdateProject :one
UPDATE projects
SET name           = COALESCE($2, name),
    description   = COALESCE($3, description),
    season         = COALESCE($4, season),
    collection_year = COALESCE($5, collection_year),
    tags           = COALESCE($6, tags),
    is_archived    = COALESCE($7, is_archived),
    updated_at     = NOW()
WHERE id = $1
RETURNING id, org_id, dept_id, user_id, name, description, season, collection_year,
          tags, is_archived, created_at, updated_at;

-- name: ArchiveProject :exec
UPDATE projects SET is_archived = true, updated_at = NOW() WHERE id = $1;
