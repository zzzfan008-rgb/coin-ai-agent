-- name: GetAuditLogByID :one
SELECT id, org_id, user_id, action, resource_type, resource_id,
       details, ip_address, user_agent, session_id, created_at
FROM audit_logs
WHERE id = $1;

-- name: ListAuditLogsByOrg :many
SELECT id, org_id, user_id, action, resource_type, resource_id,
       details, ip_address, user_agent, session_id, created_at
FROM audit_logs
WHERE org_id = $1
ORDER BY created_at DESC
LIMIT $2 OFFSET $3;

-- name: ListAuditLogsByUser :many
SELECT id, org_id, user_id, action, resource_type, resource_id,
       details, ip_address, user_agent, session_id, created_at
FROM audit_logs
WHERE org_id = $1 AND user_id = $2
ORDER BY created_at DESC
LIMIT $3 OFFSET $4;

-- name: ListAuditLogsByAction :many
SELECT id, org_id, user_id, action, resource_type, resource_id,
       details, ip_address, user_agent, session_id, created_at
FROM audit_logs
WHERE org_id = $1 AND action = $2
ORDER BY created_at DESC
LIMIT $3 OFFSET $4;

-- name: ListAuditLogsByResource :many
SELECT id, org_id, user_id, action, resource_type, resource_id,
       details, ip_address, user_agent, session_id, created_at
FROM audit_logs
WHERE org_id = $1 AND resource_type = $2 AND resource_id = $3
ORDER BY created_at DESC
LIMIT $4 OFFSET $5;

-- name: CreateAuditLog :one
INSERT INTO audit_logs (org_id, user_id, action, resource_type, resource_id,
                        details, ip_address, user_agent, session_id)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
RETURNING id, org_id, user_id, action, resource_type, resource_id,
          details, ip_address, user_agent, session_id, created_at;
