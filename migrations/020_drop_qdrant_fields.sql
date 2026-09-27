-- =============================================================================
-- Fashion AI Platform — Phase 1C 清理 Qdrant 残留字段
-- 版本: 020
-- 描述: 
--   1. knowledge_collections 表：移除 qdrant_collection 字段（不再需要）
--   2. 更新注释：明确告知 pgvector 是主向量存储
--   3. 更新 knowledge_documents 表注释：明确 chunk_embedding 存 PostgreSQL
-- 依赖: 019_pgvector_main.sql
-- =============================================================================

BEGIN;

-- -----------------------------------------------------------------------------
-- 1. knowledge_collections：移除 qdrant_collection 字段
-- -----------------------------------------------------------------------------

ALTER TABLE knowledge_collections
  DROP COLUMN IF EXISTS qdrant_collection;

-- 更新 vector_dim 默认值（1024 对应新 embedding 模型）
ALTER TABLE knowledge_collections
  ALTER COLUMN vector_dim SET DEFAULT 1024;

-- 更新注释
COMMENT ON COLUMN knowledge_collections.vector_dim IS '向量维度，默认1024（pgvector 存储，Phase 1C 起使用）';

-- 更新表注释
COMMENT ON TABLE knowledge_collections IS '知识库向量集合，embedding 存入 PostgreSQL pgvector（Phase 1C 起），不再依赖 Qdrant';

-- -----------------------------------------------------------------------------
-- 2. 更新 knowledge_documents 注释（明确向量存储位置）
-- -----------------------------------------------------------------------------

COMMENT ON COLUMN knowledge_documents.chunk_embedding IS '分块文本向量，1024维，存入 PostgreSQL pgvector，用于语义检索';

COMMENT ON TABLE knowledge_documents IS '管理员上传的文档，支持软删除。向量存入 PostgreSQL pgvector（Phase 1C 起），不再依赖 Qdrant';

-- -----------------------------------------------------------------------------
-- 3. 更新 migrations 版本行（幂等检查）
-- 确保 019_pgvector_main.sql 的版本行也存在
-- -----------------------------------------------------------------------------

-- schema_migrations 由迁移运行器写入，此处只确保 idempotent
DO $$
BEGIN
  -- 如果表存在，可能需要手动插入版本行
  -- 迁移运行器会处理，这里只做安全检查
  IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'schema_migrations') THEN
    RAISE NOTICE 'schema_migrations table not found, migrations must be run manually';
  END IF;
END $$;

COMMIT;
