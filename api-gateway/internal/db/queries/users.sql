-- name: GetUserByID :one
SELECT id, org_id, dept_id, username, password_hash, display_name,
       email, avatar_url, role, is_active, last_login_at, created_at, updated_at
FROM users
WHERE id = $1;

-- name: GetUserByUsername :one
SELECT id, org_id, dept_id, username, password_hash, display_name,
       email, avatar_url, role, is_active, last_login_at, created_at, updated_at
FROM users
WHERE org_id = $1 AND username = $2;

-- name: GetUserByEmail :one
SELECT id, org_id, dept_id, username, password_hash, display_name,
       email, avatar_url, role, is_active, last_login_at, created_at, updated_at
FROM users
WHERE org_id = $1 AND email = $2;

-- name: ListUsersByOrg :many
SELECT id, org_id, dept_id, username, display_name, email, avatar_url,
       role, is_active, last_login_at, created_at, updated_at
FROM users
WHERE org_id = $1
ORDER BY display_name;

-- name: ListUsersByDept :many
SELECT id, org_id, dept_id, username, display_name, email, avatar_url,
       role, is_active, last_login_at, created_at, updated_at
FROM users
WHERE dept_id = $1
ORDER BY display_name;

-- name: CreateUser :one
INSERT INTO users (org_id, dept_id, username, password_hash, display_name,
                   email, avatar_url, role)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
RETURNING id, org_id, dept_id, username, password_hash, display_name,
          email, avatar_url, role, is_active, last_login_at, created_at, updated_at;

-- name: UpdateUser :one
UPDATE users
SET display_name = COALESCE($2, display_name),
    email        = COALESCE($3, email),
    avatar_url   = COALESCE($4, avatar_url),
    role         = COALESCE($5, role),
    is_active    = COALESCE($6, is_active),
    updated_at   = NOW()
WHERE id = $1
RETURNING id, org_id, dept_id, username, password_hash, display_name,
          email, avatar_url, role, is_active, last_login_at, created_at, updated_at;

-- name: UpdateUserPassword :exec
UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1;

-- name: UpdateLastLogin :exec
UPDATE users SET last_login_at = NOW() WHERE id = $1;

-- name: DeactivateUser :exec
UPDATE users SET is_active = false, updated_at = NOW() WHERE id = $1;
