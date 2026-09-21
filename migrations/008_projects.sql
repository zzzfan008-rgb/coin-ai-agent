-- =============================================================================
-- Migration 008: projects 表 + 部门隔离索引
-- =============================================================================

-- projects 表已在 001_init_schema.sql 创建，这里只补充缺失的 dept_id 索引
-- 原有索引：idx_projects_org_id, idx_projects_user_id

CREATE INDEX IF NOT EXISTS idx_projects_dept_id ON projects(dept_id);
CREATE INDEX IF NOT EXISTS idx_projects_dept_archived ON projects(dept_id, is_archived);
CREATE INDEX IF NOT EXISTS idx_projects_owner ON projects(owner_id);
