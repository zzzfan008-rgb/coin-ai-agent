-- name: CheckKBPermission :one
SELECT id FROM kb_permissions
WHERE org_id                  = $1
  AND knowledge_collection_id = $2
  AND (user_id                = $3 OR dept_id = $4)
  AND permission              = $5
  AND (expires_at             IS NULL OR expires_at > NOW());

-- name: ListKBPermissionsByUser :many
SELECT kp.id, kp.org_id, kp.knowledge_collection_id, kp.dept_id,
       kp.user_id, kp.permission, kp.granted_by, kp.expires_at,
       kp.created_at, kc.name AS collection_name
FROM kb_permissions kp
JOIN knowledge_collections kc ON kc.id = kp.knowledge_collection_id
WHERE kp.org_id = $1
  AND (kp.user_id = $2 OR kp.dept_id = $3)
ORDER BY kc.name;

-- name: GrantKBPermission :one
INSERT INTO kb_permissions (org_id, knowledge_collection_id, dept_id,
                             user_id, permission, granted_by, expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
RETURNING id, org_id, knowledge_collection_id, dept_id, user_id,
          permission, granted_by, expires_at, created_at;

-- name: RevokeKBPermission :exec
DELETE FROM kb_permissions WHERE id = $1;

-- name: RevokeKBPermissionByTarget :exec
DELETE FROM kb_permissions
WHERE org_id = $1 AND knowledge_collection_id = $2
  AND (user_id = $3 OR dept_id = $4);
