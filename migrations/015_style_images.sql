-- =============================================================================
-- Fashion AI Platform — Phase 1B T-019 CLIP 以图搜图
-- 版本: 015_style_images.sql
-- 描述: 款式图片表；图片经第三方 CLIP API 向量化后存入 Qdrant
--       collection `style_images`（文本/图像共享向量空间，支持跨模态）
-- 依赖: 001_init_schema.sql (orgs, depts, users), 012_style.sql (styles)
-- =============================================================================

CREATE TABLE style_images (
    id              UUID            PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID            NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id         UUID            NOT NULL REFERENCES depts(id) ON DELETE CASCADE,
    style_id        UUID            REFERENCES styles(id) ON DELETE SET NULL,
    image_path      VARCHAR(1000)   NOT NULL,
    file_type       VARCHAR(10),
    uploaded_by     UUID            REFERENCES users(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE style_images IS '款式图片，CLIP 向量化后用于以图搜图';
COMMENT ON COLUMN style_images.image_path IS 'MinIO 对象路径或本地 uploads 路径，格式: style-images/{org_id}/{uuid}.{ext}';

CREATE INDEX idx_style_images_org_dept   ON style_images(org_id, dept_id);
CREATE INDEX idx_style_images_style      ON style_images(style_id);
