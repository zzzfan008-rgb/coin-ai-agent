-- name: GetSessionByID :one
SELECT id, org_id, user_id, dept_id, title, model, skill_ids, mcp_server_ids,
       kb_ids, is_archived, last_message_at, created_at, updated_at
FROM sessions
WHERE id = $1;

-- name: ListSessionsByOrg :many
SELECT id, org_id, user_id, dept_id, title, model, skill_ids, mcp_server_ids,
       kb_ids, is_archived, last_message_at, created_at, updated_at
FROM sessions
WHERE org_id = $1 AND is_archived = false
ORDER BY last_message_at DESC NULLS LAST, updated_at DESC;

-- name: ListSessionsByUser :many
SELECT id, org_id, user_id, dept_id, title, model, skill_ids, mcp_server_ids,
       kb_ids, is_archived, last_message_at, created_at, updated_at
FROM sessions
WHERE user_id = $1 AND is_archived = false
ORDER BY last_message_at DESC NULLS LAST, updated_at DESC;

-- name: ListArchivedSessions :many
SELECT id, org_id, user_id, dept_id, title, model, skill_ids, mcp_server_ids,
       kb_ids, is_archived, last_message_at, created_at, updated_at
FROM sessions
WHERE user_id = $1 AND is_archived = true
ORDER BY updated_at DESC;

-- name: CreateSession :one
INSERT INTO sessions (org_id, user_id, dept_id, title, model, skill_ids, mcp_server_ids, kb_ids)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
RETURNING id, org_id, user_id, dept_id, title, model, skill_ids, mcp_server_ids,
          kb_ids, is_archived, last_message_at, created_at, updated_at;

-- name: UpdateSession :one
UPDATE sessions
SET title           = COALESCE($2, title),
    model          = COALESCE($3, model),
    skill_ids      = COALESCE($4, skill_ids),
    mcp_server_ids = COALESCE($5, mcp_server_ids),
    kb_ids         = COALESCE($6, kb_ids),
    last_message_at = NOW(),
    updated_at     = NOW()
WHERE id = $1
RETURNING id, org_id, user_id, dept_id, title, model, skill_ids, mcp_server_ids,
          kb_ids, is_archived, last_message_at, created_at, updated_at;

-- name: ArchiveSession :exec
UPDATE sessions SET is_archived = true, updated_at = NOW() WHERE id = $1;

-- name: TouchSession :exec
UPDATE sessions SET last_message_at = NOW(), updated_at = NOW() WHERE id = $1;

-- name: DeleteSession :exec
DELETE FROM sessions WHERE id = $1;
