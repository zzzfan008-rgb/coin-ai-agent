-- name: GetProjectByID :one
SELECT id, org_id, dept_id, owner_id, name, description, cover_color,
       is_archived, created_at, updated_at
FROM projects
WHERE id = $1;

-- name: GetProjectByIDForUser :one
SELECT id, org_id, dept_id, owner_id, name, description, cover_color,
       is_archived, created_at, updated_at
FROM projects
WHERE id = $1 AND dept_id = $2;

-- name: ListProjectsByDept :many
SELECT id, org_id, dept_id, owner_id, name, description, cover_color,
       is_archived, created_at, updated_at
FROM projects
WHERE dept_id = $1 AND is_archived = $2
ORDER BY updated_at DESC;

-- name: ListArchivedProjectsByDept :many
SELECT id, org_id, dept_id, owner_id, name, description, cover_color,
       is_archived, created_at, updated_at
FROM projects
WHERE dept_id = $1 AND is_archived = true
ORDER BY updated_at DESC;

-- name: CreateProject :one
INSERT INTO projects (org_id, dept_id, owner_id, name, description, cover_color)
VALUES ($1, $2, $3, $4, $5, $6)
RETURNING id, org_id, dept_id, owner_id, name, description, cover_color,
          is_archived, created_at, updated_at;

-- name: UpdateProject :one
UPDATE projects
SET name        = COALESCE($2, name),
    description  = COALESCE($3, description),
    cover_color  = COALESCE($4, cover_color),
    updated_at   = NOW()
WHERE id = $1 AND dept_id = $5
RETURNING id, org_id, dept_id, owner_id, name, description, cover_color,
          is_archived, created_at, updated_at;

-- name: SoftDeleteProject :one
UPDATE projects SET is_archived = true, updated_at = NOW()
WHERE id = $1 AND dept_id = $2
RETURNING id;

-- name: ArchiveProject :one
UPDATE projects SET is_archived = true, updated_at = NOW()
WHERE id = $1 AND dept_id = $2
RETURNING id, org_id, dept_id, owner_id, name, description, cover_color,
          is_archived, created_at, updated_at;

-- name: UnarchiveProject :one
UPDATE projects SET is_archived = false, updated_at = NOW()
WHERE id = $1 AND dept_id = $2
RETURNING id, org_id, dept_id, owner_id, name, description, cover_color,
          is_archived, created_at, updated_at;

-- name: ListProjectSessions :many
SELECT s.id, s.org_id, s.user_id, s.title, s.is_archived,
       (SELECT COUNT(*) FROM messages m WHERE m.session_id=s.id) AS message_count,
       s.created_at, s.updated_at,
       sp.added_at
FROM session_project sp
JOIN sessions s ON s.id = sp.session_id
WHERE sp.project_id = $1
ORDER BY sp.added_at DESC;

-- name: AddSessionToProject :exec
INSERT INTO session_project (session_id, project_id)
VALUES ($1, $2)
ON CONFLICT (session_id, project_id) DO NOTHING;

-- name: RemoveSessionFromProject :exec
DELETE FROM session_project WHERE session_id = $1 AND project_id = $2;

-- name: GetProjectsForSession :many
SELECT p.id, p.name, p.description, p.cover_color, p.is_archived, p.updated_at,
       sp.added_at
FROM session_project sp
JOIN projects p ON p.id = sp.project_id
WHERE sp.session_id = $1 AND p.dept_id = $2
ORDER BY sp.added_at DESC;
