-- =============================================================================
-- Migration 008: projects 前向对齐迁移（user_id → owner_id + cover_color）
-- =============================================================================
-- 背景：Phase 1A 已发布的 001 中 projects 以 user_id 作为归属列；
-- Phase 1B 起服务代码统一使用 owner_id + cover_color。
--
-- 本迁移幂等地把旧库 projects 升级到新形态：
--   1) 若 owner_id 不存在：新增 owner_id、从 user_id 回填、加 FK/NOT NULL
--   2) 若 cover_color 不存在：新增（NOT NULL DEFAULT）
--   3) 若 name/description/is_archived 缺失：补齐（历史形态差异）
--   4) 下线遗留列 user_id / season / collection_year / tags
-- 干净库（001 已是新形态）执行本文件为安全 no-op，最终结构与旧库升级一致。
-- =============================================================================

-- 1) owner_id：新增 → 回填 → FK → NOT NULL
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'projects' AND column_name = 'owner_id'
    ) THEN
        ALTER TABLE projects ADD COLUMN owner_id UUID;

        -- 优先从旧归属列 user_id 回填
        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'projects' AND column_name = 'user_id'
        ) THEN
            EXECUTE 'UPDATE projects SET owner_id = user_id WHERE owner_id IS NULL';
        END IF;

        -- 无法回填的行（理论上旧列 NOT NULL，不会出现）：直接报错，
        -- 保持 fail-closed，避免静默产生孤儿数据。
        IF EXISTS (SELECT 1 FROM projects WHERE owner_id IS NULL) THEN
            RAISE EXCEPTION 'projects.owner_id has NULL rows that cannot be backfilled from user_id';
        END IF;

        ALTER TABLE projects ALTER COLUMN owner_id SET NOT NULL;
        EXECUTE 'ALTER TABLE projects
                 ADD CONSTRAINT projects_owner_id_fkey
                 FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE CASCADE';
    END IF;
END $$;

-- 2) cover_color
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'projects' AND column_name = 'cover_color'
    ) THEN
        ALTER TABLE projects
            ADD COLUMN cover_color VARCHAR(7) NOT NULL DEFAULT '#6366F1';
    END IF;
END $$;

-- 3) 历史形态补齐：name / description / is_archived
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'projects' AND column_name = 'name'
    ) THEN
        ALTER TABLE projects ADD COLUMN name VARCHAR(255);
        -- 用 id 片段生成唯一名，避免唯一约束冲突
        EXECUTE $q$
            UPDATE projects
            SET name = COALESCE(NULLIF(season, ''), '项目 ' || substr(id::text, 1, 8))
            WHERE name IS NULL
        $q$;
        ALTER TABLE projects ALTER COLUMN name SET NOT NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'projects' AND column_name = 'description'
    ) THEN
        ALTER TABLE projects ADD COLUMN description TEXT;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'projects' AND column_name = 'is_archived'
    ) THEN
        ALTER TABLE projects ADD COLUMN is_archived BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

-- 名字唯一约束（新库 001 已带，旧库补齐）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_project_name_org'
    ) THEN
        ALTER TABLE projects ADD CONSTRAINT uq_project_name_org UNIQUE (org_id, name);
    END IF;
END $$;

-- 4) 下线遗留列（owner_id 已就位后再删除）
ALTER TABLE projects DROP COLUMN IF EXISTS user_id;
ALTER TABLE projects DROP COLUMN IF EXISTS season;
ALTER TABLE projects DROP COLUMN IF EXISTS collection_year;
ALTER TABLE projects DROP COLUMN IF EXISTS tags;

-- 5) 索引（幂等）
CREATE INDEX IF NOT EXISTS idx_projects_dept_id       ON projects(dept_id);
CREATE INDEX IF NOT EXISTS idx_projects_dept_archived ON projects(dept_id, is_archived);
-- owner 索引：仅当不存在任何 (owner_id) 索引时创建，避免与新库 001 的
-- idx_projects_owner_id 重复。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE tablename = 'projects' AND indexdef LIKE '%(owner_id)%'
    ) THEN
        CREATE INDEX idx_projects_owner ON projects(owner_id);
    END IF;
END $$;
