-- =============================================================================
-- Fashion AI Platform — T-020 并发边界收口：username 全局唯一索引
-- 版本: 021
-- 描述:
--   T-020 产品决策为 username 全局唯一（见 docs/phase2-tasks.md 实施记录：
--   登录按 username 单字段查找，跨 org 重名会产生登录歧义）。此前该语义
--   只靠应用层 SELECT EXISTS 查重实现（service.go:79），两个并发同名注册
--   可同时通过查重、各自建 org 后 INSERT 成功，突破全局唯一语义。
--
--   本迁移为全局唯一上数据库级兜底：
--     1. 先检测既有跨 org 重复 username，发现即中止并带出重复清单
--        （数据必须先修，索引才能建）；
--     2. 建全局唯一索引 uq_users_username_global。
--
--   保留 001_init_schema.sql 的 (org_id, username) 组合约束 uq_user_username_org
--   （叠加，保守）。
--
--   幂等：CREATE UNIQUE INDEX IF NOT EXISTS；重复执行不变。重复检测在
--   本迁移文件的应用层记账（schema_migrations）之下不会重跑（已应用版本
--   整体跳过），但若在其它环境重放，检测也只会命中真实存在的重复行。
-- =============================================================================

-- 1. 检测既有跨 org 重复 username（含软删/多 org）；存在则中止迁移
DO $$
DECLARE
    dup_count bigint;
    dup_lines text;
BEGIN
    SELECT COUNT(*) INTO dup_count
    FROM (SELECT username FROM users GROUP BY username HAVING COUNT(*) > 1) d;

    IF dup_count > 0 THEN
        SELECT string_agg(username || ' (x' || n || ')', ', ' ORDER BY username)
        INTO dup_lines
        FROM (
            SELECT username, COUNT(*) AS n
            FROM users
            GROUP BY username
            HAVING COUNT(*) > 1
        ) d;

        RAISE EXCEPTION
            'T-020 全局唯一索引前置检查失败：% 个 username 存在跨 org 重复：% 。'
            ' 请先人工合并/清理重复用户后再重跑本迁移。',
            dup_count, dup_lines;
    END IF;
END $$;

-- 2. 建立 username 全局唯一索引（空值不占位，PostgreSQL 默认 NULLS DISTINCT）
CREATE UNIQUE INDEX IF NOT EXISTS uq_users_username_global
    ON users (username);

-- 3. 确认索引归属，便于排查
COMMENT ON INDEX uq_users_username_global IS
    'T-020: username 全局唯一（产品决策），并发注册兜底见 021_user_username_global_unique.sql';