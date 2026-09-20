-- name: GetSkillByID :one
SELECT id, org_id, name, version, description, prompt_template,
       tool_definitions, is_active, created_by, created_at, updated_at
FROM skills
WHERE id = $1;

-- name: ListSkillsByOrg :many
SELECT id, org_id, name, version, description, prompt_template,
       tool_definitions, is_active, created_by, created_at, updated_at
FROM skills
WHERE org_id = $1 AND is_active = true
ORDER BY name;

-- name: GetSkillByName :one
SELECT id, org_id, name, version, description, prompt_template,
       tool_definitions, is_active, created_by, created_at, updated_at
FROM skills
WHERE org_id = $1 AND name = $2;

-- name: CreateSkill :one
INSERT INTO skills (org_id, name, version, description, prompt_template,
                    tool_definitions, created_by)
VALUES ($1, $2, $3, $4, $5, $6, $7)
RETURNING id, org_id, name, version, description, prompt_template,
          tool_definitions, is_active, created_by, created_at, updated_at;

-- name: UpdateSkill :one
UPDATE skills
SET version         = COALESCE($2, version),
    description     = COALESCE($3, description),
    prompt_template = COALESCE($4, prompt_template),
    tool_definitions = COALESCE($5, tool_definitions),
    is_active       = COALESCE($6, is_active),
    updated_at      = NOW()
WHERE id = $1
RETURNING id, org_id, name, version, description, prompt_template,
          tool_definitions, is_active, created_by, created_at, updated_at;

-- name: DeactivateSkill :exec
UPDATE skills SET is_active = false, updated_at = NOW() WHERE id = $1;
