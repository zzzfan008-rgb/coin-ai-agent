-- name: GetMessageByID :one
SELECT id, session_id, role, content, model, finish_reason,
       token_count, input_tokens, output_tokens, metadata, created_at
FROM messages
WHERE id = $1;

-- name: ListMessagesBySession :many
SELECT id, session_id, role, content, model, finish_reason,
       token_count, input_tokens, output_tokens, metadata, created_at
FROM messages
WHERE session_id = $1
ORDER BY created_at ASC;

-- name: CreateMessage :one
INSERT INTO messages (session_id, role, content, model, finish_reason,
                      token_count, input_tokens, output_tokens, metadata)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
RETURNING id, session_id, role, content, model, finish_reason,
          token_count, input_tokens, output_tokens, metadata, created_at;

-- name: CountMessagesBySession :one
SELECT COUNT(*) AS count FROM messages WHERE session_id = $1;

-- name: DeleteMessage :exec
DELETE FROM messages WHERE id = $1;
