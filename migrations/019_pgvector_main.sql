-- =============================================================================
-- Fashion AI Platform — Phase 1C pgvector 主迁移
-- 版本: 019
-- 描述: 
--   1. fabrics 表：补充 image_url + pantone_codes + description_embedding
--   2. colors 表：补充 pantone_codes + description_embedding
--   3. messages 表：补充 content_embedding + is_important（长期记忆标记）
--   4. knowledge_documents 表：补充 chunk_text（分块文本）+ chunk_embedding
--   5. 创建 pgvector HNSW 索引（语义搜索）
--   6. 创建 GIN 索引（关键词全文搜索）
-- 依赖: 001_init_schema.sql, 010_fabric.sql, 011_color.sql, 014_knowledge_documents.sql
-- =============================================================================

BEGIN;

-- -----------------------------------------------------------------------------
-- 1. fabrics 表：补充字段
-- -----------------------------------------------------------------------------

-- 图片 URL（MinIO 路径或 CDN）
ALTER TABLE fabrics
  ADD COLUMN IF NOT EXISTS image_url VARCHAR(1000) DEFAULT '';

COMMENT ON COLUMN fabrics.image_url IS '面料图片 URL，格式: knowledge/{org_id}/{uuid}.{ext} 或外部 CDN URL';

-- 潘通色号（逗号分隔，支持多色）
ALTER TABLE fabrics
  ADD COLUMN IF NOT EXISTS pantone_codes VARCHAR(500) DEFAULT '';

COMMENT ON COLUMN fabrics.pantone_codes IS '潘通色号，多个用逗号分隔，如 13-4304 TCX, 18-1663 TPX';

-- 详细描述（用于 embedding 生成）
ALTER TABLE fabrics
  ADD COLUMN IF NOT EXISTS description TEXT DEFAULT '';

COMMENT ON COLUMN fabrics.description IS '面料详细描述，包含成分/克重/特性/适用款式/保养说明，用于语义搜索';

-- 向量嵌入（pgvector）
ALTER TABLE fabrics
  ADD COLUMN IF NOT EXISTS description_embedding vector(1024);

COMMENT ON COLUMN fabrics.description_embedding IS '面料描述向量，1024维，由 embedding 模型生成，用于语义搜索';
COMMENT ON COLUMN fabrics.description_embedding IS 'cosine distance < 0.3 时视为语义相似';

-- -----------------------------------------------------------------------------
-- 2. colors 表：补充字段
-- -----------------------------------------------------------------------------

-- 潘通色号
ALTER TABLE colors
  ADD COLUMN IF NOT EXISTS pantone_codes VARCHAR(500) DEFAULT '';

COMMENT ON COLUMN colors.pantone_codes IS '潘通色号，如 13-4304 TCX';

-- 详细描述（用于 embedding）
ALTER TABLE colors
  ADD COLUMN IF NOT EXISTS description TEXT DEFAULT '';

COMMENT ON COLUMN colors.description IS '色彩描述，包含色系/季节/搭配建议，用于语义搜索';

-- 向量嵌入
ALTER TABLE colors
  ADD COLUMN IF NOT EXISTS description_embedding vector(1024);

COMMENT ON COLUMN colors.description_embedding IS '色彩描述向量，1024维，用于语义搜索';

-- -----------------------------------------------------------------------------
-- 3. messages 表：补充字段
-- -----------------------------------------------------------------------------

-- 消息内容向量（用于会话记忆语义搜索）
ALTER TABLE messages
  ADD COLUMN IF NOT EXISTS content_embedding vector(1024);

COMMENT ON COLUMN messages.content_embedding IS '消息内容向量，1024维，用于语义搜索会话历史';
COMMENT ON COLUMN messages.content_embedding IS 'cosine distance < 0.3 时视为语义相关';

-- 重要标记（长期记忆，不压缩）
ALTER TABLE messages
  ADD COLUMN IF NOT EXISTS is_important BOOLEAN NOT NULL DEFAULT false;

COMMENT ON COLUMN messages.is_important IS '重要消息标记，true 时永久保留，不参与上下文压缩';

-- -----------------------------------------------------------------------------
-- 4. knowledge_documents 表：补充字段
-- -----------------------------------------------------------------------------

-- 原始分块文本（chunk 级别，用于 RAG）
ALTER TABLE knowledge_documents
  ADD COLUMN IF NOT EXISTS chunk_text TEXT DEFAULT '';

COMMENT ON COLUMN knowledge_documents.chunk_text IS '文档分块后的原始文本内容，每块约500字，用于 RAG 检索';

-- 分块向量
ALTER TABLE knowledge_documents
  ADD COLUMN IF NOT EXISTS chunk_embedding vector(1024);

COMMENT ON COLUMN knowledge_documents.chunk_embedding IS '分块文本向量，1024维，用于语义检索';

-- -----------------------------------------------------------------------------
-- 5. 创建 HNSW 索引（语义搜索，cosine distance）
-- -----------------------------------------------------------------------------

-- fabrics 语义搜索索引
CREATE INDEX IF NOT EXISTS idx_fabrics_embedding_hnsw
  ON fabrics USING hnsw (description_embedding vector_cosine_ops)
  WHERE description_embedding IS NOT NULL;

-- colors 语义搜索索引
CREATE INDEX IF NOT EXISTS idx_colors_embedding_hnsw
  ON colors USING hnsw (description_embedding vector_cosine_ops)
  WHERE description_embedding IS NOT NULL;

-- messages 语义搜索索引（会话记忆）
CREATE INDEX IF NOT EXISTS idx_messages_embedding_hnsw
  ON messages USING hnsw (content_embedding vector_cosine_ops)
  WHERE content_embedding IS NOT NULL;

-- knowledge_documents 语义搜索索引
CREATE INDEX IF NOT EXISTS idx_kb_docs_embedding_hnsw
  ON knowledge_documents USING hnsw (chunk_embedding vector_cosine_ops)
  WHERE chunk_embedding IS NOT NULL;

-- -----------------------------------------------------------------------------
-- 6. 创建 GIN 索引（关键词全文搜索）
-- -----------------------------------------------------------------------------

-- fabrics 关键词搜索
CREATE INDEX IF NOT EXISTS idx_fabrics_fts
  ON fabrics USING gin (to_tsvector('simple',
    coalesce(name, '') || ' ' || coalesce(composition, '') || ' ' ||
    coalesce(season, '') || ' ' || coalesce(features, '') || ' ' ||
    coalesce(description, '')));

-- colors 关键词搜索
CREATE INDEX IF NOT EXISTS idx_colors_fts
  ON colors USING gin (to_tsvector('simple',
    coalesce(name, '') || ' ' || coalesce(hex, '') || ' ' ||
    coalesce(category, '') || ' ' || coalesce(season, '') || ' ' ||
    coalesce(description, '')));

COMMIT;

-- -----------------------------------------------------------------------------
-- 7. 辅助函数：语义搜索面料（带 org_id 隔离）
-- -----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION knn_search_fabrics(
  p_query_embedding vector(1024),
  p_org_id UUID,
  p_limit INT DEFAULT 10,
  p_max_distance FLOAT DEFAULT 0.3
)
RETURNS TABLE (
  id              UUID,
  name            VARCHAR,
  composition     VARCHAR,
  weight_gm2      INT,
  season          VARCHAR,
  image_url       VARCHAR,
  pantone_codes   VARCHAR,
  description     TEXT,
  care_instructions TEXT,
  distance        FLOAT
) AS $$
BEGIN
  RETURN QUERY
  SELECT
    f.id,
    f.name,
    f.composition,
    f.weight_gm2,
    f.season,
    f.image_url,
    f.pantone_codes,
    f.description,
    f.care_instructions,
    (f.description_embedding <=> p_query_embedding)::FLOAT AS distance
  FROM fabrics f
  WHERE f.org_id = p_org_id
    AND f.description_embedding IS NOT NULL
    AND (f.description_embedding <=> p_query_embedding) < p_max_distance
  ORDER BY f.description_embedding <=> p_query_embedding
  LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION knn_search_fabrics IS
'语义搜索面料，org_id 隔离，返回 cosine distance 最近的结果';

-- -----------------------------------------------------------------------------
-- 8. 辅助函数：语义搜索会话记忆
-- -----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION knn_search_messages(
  p_query_embedding vector(1024),
  p_session_id UUID,
  p_limit INT DEFAULT 5,
  p_max_distance FLOAT DEFAULT 0.3
)
RETURNS TABLE (
  id          UUID,
  role        VARCHAR,
  content     TEXT,
  distance    FLOAT
) AS $$
BEGIN
  RETURN QUERY
  SELECT
    m.id,
    m.role,
    m.content,
    (m.content_embedding <=> p_query_embedding)::FLOAT AS distance
  FROM messages m
  WHERE m.session_id = p_session_id
    AND m.content_embedding IS NOT NULL
    AND (m.content_embedding <=> p_query_embedding) < p_max_distance
  ORDER BY m.content_embedding <=> p_query_embedding
  LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION knn_search_messages IS
'语义搜索同会话内的历史消息，用于上下文补充';

-- -----------------------------------------------------------------------------
-- 9. 辅助函数：全文搜索面料（无 embedding 时降级）
-- -----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION fts_search_fabrics(
  p_keyword VARCHAR,
  p_org_id UUID,
  p_limit INT DEFAULT 10
)
RETURNS TABLE (
  id              UUID,
  name            VARCHAR,
  composition     VARCHAR,
  weight_gm2      INT,
  season          VARCHAR,
  image_url       VARCHAR,
  pantone_codes   VARCHAR,
  description     TEXT,
  care_instructions TEXT,
  rank            FLOAT
) AS $$
BEGIN
  RETURN QUERY
  SELECT
    f.id,
    f.name,
    f.composition,
    f.weight_gm2,
    f.season,
    f.image_url,
    f.pantone_codes,
    f.description,
    f.care_instructions,
    ts_rank(
      to_tsvector('simple',
        coalesce(f.name, '') || ' ' || coalesce(f.composition, '') || ' ' ||
        coalesce(f.season, '') || ' ' || coalesce(f.features, '') || ' ' ||
        coalesce(f.description, '')
      ),
      plainto_tsquery('simple', p_keyword)
    )::FLOAT AS rank
  FROM fabrics f
  WHERE f.org_id = p_org_id
    AND to_tsvector('simple',
      coalesce(f.name, '') || ' ' || coalesce(f.composition, '') || ' ' ||
      coalesce(f.season, '') || ' ' || coalesce(f.features, '') || ' ' ||
      coalesce(f.description, '')
    ) @@ plainto_tsquery('simple', p_keyword)
  ORDER BY rank DESC
  LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION fts_search_fabrics IS
'全文搜索面料（无 embedding 时的降级方案），按关键词相关性排序';
