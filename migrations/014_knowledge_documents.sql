-- =============================================================================
-- Fashion AI Platform — Phase 1B T-015 知识库文档表
-- 版本: 014
-- 描述: 管理员上传文档存储表，支持 PDF/DOCX/TXT/MD，向量化后存入 Qdrant
-- 依赖: 001_init_schema.sql (orgs, depts, users 已存在)
-- =============================================================================

-- 知识库文档表
CREATE TABLE knowledge_documents (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id         UUID        NOT NULL REFERENCES depts(id) ON DELETE CASCADE,
    title           VARCHAR(500) NOT NULL,
    file_type       VARCHAR(20)  NOT NULL,  -- pdf | docx | txt | md
    file_path       VARCHAR(1000) NOT NULL,  -- MinIO 对象路径
    file_size       BIGINT,                  -- 字节
    chunk_count     INT         DEFAULT 0,   -- 解析后的 chunk 总数
    status          VARCHAR(20) DEFAULT 'pending',  -- pending | processing | ready | failed | deleted
    error_message   TEXT,
    uploaded_by      UUID        REFERENCES users(id) ON DELETE SET NULL,
    uploaded_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at     TIMESTAMPTZ,
    deleted_at       TIMESTAMPTZ,            -- 软删除

    CONSTRAINT chk_kb_file_type CHECK (file_type IN ('pdf', 'docx', 'txt', 'md'))
);

COMMENT ON TABLE knowledge_documents IS '管理员上传的文档，支持软删除（deleted_at IS NOT NULL 时视为已删除）';
COMMENT ON COLUMN knowledge_documents.file_path IS 'MinIO 路径，格式: knowledge/{org_id}/{uuid}.{ext}';
COMMENT ON COLUMN knowledge_documents.status IS 'pending=待处理 processing=解析中 ready=已就绪 failed=失败 deleted=软删除';

-- 索引
CREATE INDEX idx_kb_docs_org_dept   ON knowledge_documents(org_id, dept_id);
CREATE INDEX idx_kb_docs_status     ON knowledge_documents(status);
CREATE INDEX idx_kb_docs_uploaded   ON knowledge_documents(org_id, uploaded_at DESC);
CREATE INDEX idx_kb_docs_dept_status ON knowledge_documents(dept_id, status) WHERE deleted_at IS NULL;

-- 迁移记录
INSERT INTO schema_migrations (version, dirty, applied_at) VALUES (14, false, NOW());
