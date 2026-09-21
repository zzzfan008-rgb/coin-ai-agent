-- =============================================================================
-- Fashion AI Platform — Phase 1B T-012
-- 版本: 012_style.sql
-- 描述: 款式库表
-- =============================================================================

CREATE TABLE IF NOT EXISTS styles (
    id                  UUID            PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id              UUID            NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id             UUID            NOT NULL REFERENCES depts(id) ON DELETE CASCADE,
    name                VARCHAR(255)    NOT NULL,
    description         TEXT,
    silhouette          VARCHAR(100),
    garment_type        VARCHAR(100),
    key_features        TEXT[],
    suitable_seasons    TEXT[],
    target_audience     TEXT[],
    created_at          TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE  styles IS '款式库';
COMMENT ON COLUMN styles.name IS '款式名称，如A字裙';
COMMENT ON COLUMN styles.silhouette IS '轮廓: A型/H型/X型/O型/T型/Y型/S型';
COMMENT ON COLUMN styles.garment_type IS '品类: 连衣裙/衬衫/外套/裤装/裙装/针织';
COMMENT ON COLUMN styles.key_features IS '关键设计特征';
COMMENT ON COLUMN styles.suitable_seasons IS '适用季节';
COMMENT ON COLUMN styles.target_audience IS '目标人群';

CREATE INDEX IF NOT EXISTS idx_styles_org_dept ON styles(org_id, dept_id);
