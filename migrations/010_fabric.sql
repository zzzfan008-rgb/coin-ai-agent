-- =============================================================================
-- Fashion AI Platform — Phase 1B T-012
-- 版本: 010_fabric.sql
-- 描述: 面料库表
-- =============================================================================

CREATE TABLE fabrics (
    id                  UUID            PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id              UUID            NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id             UUID            NOT NULL REFERENCES depts(id) ON DELETE CASCADE,
    name                VARCHAR(255)    NOT NULL,
    composition         VARCHAR(255),
    weight_gm2          INT,
    season              VARCHAR(50),
    applicable_styles   TEXT[],
    care_instructions   TEXT,
    features            TEXT,
    created_at          TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE  fabrics IS '面料库';
COMMENT ON COLUMN fabrics.name IS '面料名称，如棉府绸';
COMMENT ON COLUMN fabrics.composition IS '成分，如100%棉';
COMMENT ON COLUMN fabrics.weight_gm2 IS '克重 g/m²';
COMMENT ON COLUMN fabrics.season IS '适用季节: spring/summer/autumn/winter/all-season';
COMMENT ON COLUMN fabrics.applicable_styles IS '适用款式标签';
COMMENT ON COLUMN fabrics.care_instructions IS '保养说明';
COMMENT ON COLUMN fabrics.features IS '面料特性';

CREATE INDEX idx_fabrics_org_dept ON fabrics(org_id, dept_id);
