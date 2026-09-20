-- name: GetKnowledgeCollectionByID :one
SELECT id, org_id, dept_id, name, description, collection_type,
       embedding_model, vector_dim, qdrant_collection, is_public,
       created_by, created_at, updated_at
FROM knowledge_collections
WHERE id = $1;

-- name: ListKnowledgeCollectionsByOrg :many
SELECT id, org_id, dept_id, name, description, collection_type,
       embedding_model, vector_dim, qdrant_collection, is_public,
       created_by, created_at, updated_at
FROM knowledge_collections
WHERE org_id = $1
ORDER BY name;

-- name: GetKnowledgeCollectionByName :one
SELECT id, org_id, dept_id, name, description, collection_type,
       embedding_model, vector_dim, qdrant_collection, is_public,
       created_by, created_at, updated_at
FROM knowledge_collections
WHERE org_id = $1 AND name = $2;

-- name: CreateKnowledgeCollection :one
INSERT INTO knowledge_collections (org_id, dept_id, name, description,
                                   collection_type, embedding_model,
                                   vector_dim, qdrant_collection,
                                   is_public, created_by)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
RETURNING id, org_id, dept_id, name, description, collection_type,
          embedding_model, vector_dim, qdrant_collection, is_public,
          created_by, created_at, updated_at;

-- name: UpdateKnowledgeCollection :one
UPDATE knowledge_collections
SET name               = COALESCE($2, name),
    description        = COALESCE($3, description),
    collection_type    = COALESCE($4, collection_type),
    embedding_model    = COALESCE($5, embedding_model),
    vector_dim         = COALESCE($6, vector_dim),
    qdrant_collection = COALESCE($7, qdrant_collection),
    is_public          = COALESCE($8, is_public),
    updated_at         = NOW()
WHERE id = $1
RETURNING id, org_id, dept_id, name, description, collection_type,
          embedding_model, vector_dim, qdrant_collection, is_public,
          created_by, created_at, updated_at;

-- name: DeleteKnowledgeCollection :exec
DELETE FROM knowledge_collections WHERE id = $1;
