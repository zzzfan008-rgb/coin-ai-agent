-- =============================================================================
-- Fashion AI Platform — Phase 1B T-012
-- 版本: 011_color.sql
-- 描述: 色彩库表
-- =============================================================================

CREATE TABLE colors (
    id          UUID            PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID            NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id     UUID            NOT NULL REFERENCES depts(id) ON DELETE CASCADE,
    hex         VARCHAR(7)      NOT NULL,
    name        VARCHAR(100),
    rgb_r       INT,
    rgb_g       INT,
    rgb_b       INT,
    hsl_h       INT,
    hsl_s       INT,
    hsl_l       INT,
    category    VARCHAR(50),
    season      VARCHAR(50),
    created_at  TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE  colors IS '色彩库';
COMMENT ON COLUMN colors.hex IS '十六进制色值 #RRGGBB';
COMMENT ON COLUMN colors.name IS '色彩名称（中国传统色名/Pantone参考名）';
COMMENT ON COLUMN colors.category IS '主色/辅色/点缀色';
COMMENT ON COLUMN colors.season IS '适用季节';

CREATE INDEX idx_colors_org_dept ON colors(org_id, dept_id);
