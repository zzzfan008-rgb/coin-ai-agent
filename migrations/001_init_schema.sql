-- =============================================================================
-- Fashion AI Platform — Phase 1A 数据库初始化迁移
-- 版本: 001_init_schema.sql
-- 描述: 创建所有核心表、索引、约束、初始数据
-- 执行前提: PostgreSQL 16 + pgvector 容器运行中
-- =============================================================================

-- -----------------------------------------------------------------------------
-- 1. 启用 UUID 生成和 pgvector 扩展
-- -----------------------------------------------------------------------------
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

-- -----------------------------------------------------------------------------
-- 2. 组织表
-- -----------------------------------------------------------------------------
CREATE TABLE orgs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name            VARCHAR(255) NOT NULL,
    slug            VARCHAR(100) NOT NULL UNIQUE,
    is_active       BOOLEAN     NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE orgs IS '组织/租户，每个独立团队对应一个 org';
COMMENT ON COLUMN orgs.slug IS 'URL-safe 标识，如 fashion-team';

-- -----------------------------------------------------------------------------
-- 3. 部门表（树形层级，含 path）
-- -----------------------------------------------------------------------------
CREATE TABLE depts (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    parent_id       UUID        REFERENCES depts(id) ON DELETE SET NULL,
    name            VARCHAR(255) NOT NULL,
    path            VARCHAR(1000) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_dept_name_under_org UNIQUE (org_id, parent_id, name)
);

COMMENT ON TABLE depts IS '部门树形结构，path 如 /root/design-team';
COMMENT ON COLUMN depts.path IS '层级路径，从根到当前节点，例: /root/design-team/summer-group';

-- -----------------------------------------------------------------------------
-- 4. 用户表
-- -----------------------------------------------------------------------------
CREATE TABLE users (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id         UUID        NOT NULL REFERENCES depts(id) ON DELETE RESTRICT,
    username        VARCHAR(100) NOT NULL,
    password_hash   VARCHAR(255) NOT NULL,
    display_name    VARCHAR(255),
    email           VARCHAR(255),
    avatar_url      VARCHAR(500),
    role            VARCHAR(50)  NOT NULL DEFAULT 'designer',
    is_active       BOOLEAN     NOT NULL DEFAULT true,
    last_login_at   TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_user_username_org UNIQUE (org_id, username),
    CONSTRAINT uq_user_email_org    UNIQUE (org_id, email)
);

COMMENT ON TABLE users IS '用户表，role: admin|designer|viewer';
COMMENT ON COLUMN users.role IS 'admin=超级管理员 designer=设计师 viewer=只读访客';

-- -----------------------------------------------------------------------------
-- 5. Skills 表（Agent 可用工具/Skill 注册）
-- -----------------------------------------------------------------------------
CREATE TABLE skills (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name            VARCHAR(200) NOT NULL,
    version         VARCHAR(50)  NOT NULL DEFAULT '1.0.0',
    description     TEXT,
    prompt_template TEXT,
    tool_definitions JSONB      NOT NULL DEFAULT '[]',
    is_active       BOOLEAN     NOT NULL DEFAULT true,
    created_by      UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_skill_name_org UNIQUE (org_id, name)
);

COMMENT ON TABLE skills IS 'Skill 注册表，tool_definitions 为 JSON Schema 数组';

-- -----------------------------------------------------------------------------
-- 6. MCP Servers 表（Model Context Protocol 服务器）
-- -----------------------------------------------------------------------------
CREATE TABLE mcp_servers (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name            VARCHAR(200) NOT NULL,
    server_type     VARCHAR(50)  NOT NULL DEFAULT 'stdio',
    endpoint        VARCHAR(500),
    auth_token      VARCHAR(500),
    env_vars        JSONB       NOT NULL DEFAULT '{}',
    is_active       BOOLEAN     NOT NULL DEFAULT true,
    created_by      UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_mcp_server_name_org UNIQUE (org_id, name)
);

COMMENT ON TABLE mcp_servers IS 'MCP 服务器配置，server_type: stdio | http';
COMMENT ON COLUMN mcp_servers.endpoint IS 'HTTP MCP 的 URL；stdio 类型可为空';
COMMENT ON COLUMN mcp_servers.auth_token IS '加密存储，查询时自动解密';

-- -----------------------------------------------------------------------------
-- 7. Skill 权限表（用户 × Skill 授权）
-- -----------------------------------------------------------------------------
CREATE TABLE skill_permissions (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id         UUID        REFERENCES depts(id) ON DELETE CASCADE,
    user_id         UUID        REFERENCES users(id) ON DELETE CASCADE,
    skill_id        UUID        NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    permission      VARCHAR(50)  NOT NULL DEFAULT 'read',
    granted_by      UUID        REFERENCES users(id) ON DELETE SET NULL,
    expires_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT chk_skill_perm_target CHECK (
        (dept_id IS NOT NULL)::int + (user_id IS NOT NULL)::int = 1
    )
);

COMMENT ON TABLE skill_permissions IS 'Skill 权限，dept_id 为部门级授权，user_id 为个人授权';
COMMENT ON COLUMN skill_permissions.permission IS 'read=可调用 execute=可编辑配置';

-- -----------------------------------------------------------------------------
-- 8. MCP Server 权限表
-- -----------------------------------------------------------------------------
CREATE TABLE mcp_permissions (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id         UUID        REFERENCES depts(id) ON DELETE CASCADE,
    user_id         UUID        REFERENCES users(id) ON DELETE CASCADE,
    mcp_server_id   UUID        NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
    permission      VARCHAR(50)  NOT NULL DEFAULT 'use',
    granted_by      UUID        REFERENCES users(id) ON DELETE SET NULL,
    expires_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT chk_mcp_perm_target CHECK (
        (dept_id IS NOT NULL)::int + (user_id IS NOT NULL)::int = 1
    )
);

COMMENT ON TABLE mcp_permissions IS 'MCP Server 权限';

-- -----------------------------------------------------------------------------
-- 9. 角色定义表
-- -----------------------------------------------------------------------------
CREATE TABLE roles (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name            VARCHAR(100) NOT NULL,
    permissions     JSONB       NOT NULL DEFAULT '[]',
    description     TEXT,
    is_system       BOOLEAN     NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_role_name_org UNIQUE (org_id, name)
);

COMMENT ON TABLE roles IS '自定义角色，permissions 为权限列表 JSONB';

-- -----------------------------------------------------------------------------
-- 10. 会话表
-- -----------------------------------------------------------------------------
CREATE TABLE sessions (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    user_id         UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    dept_id         UUID        NOT NULL REFERENCES depts(id) ON DELETE CASCADE,
    title           VARCHAR(500),
    model           VARCHAR(100),
    skill_ids       JSONB       NOT NULL DEFAULT '[]',
    mcp_server_ids  JSONB       NOT NULL DEFAULT '[]',
    kb_ids          JSONB       NOT NULL DEFAULT '[]',
    is_archived     BOOLEAN     NOT NULL DEFAULT false,
    last_message_at TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE sessions IS '对话会话，支持归档（不删除历史）';

-- -----------------------------------------------------------------------------
-- 11. 消息表
-- -----------------------------------------------------------------------------
CREATE TABLE messages (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id      UUID        NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role            VARCHAR(50)  NOT NULL,
    content         TEXT        NOT NULL,
    model           VARCHAR(100),
    finish_reason   VARCHAR(50),
    token_count     INT,
    input_tokens    INT,
    output_tokens   INT,
    metadata        JSONB       NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE messages IS '会话消息，role: user|assistant|system|tool';
COMMENT ON COLUMN messages.token_count IS '总 token 数（input+output）';

-- -----------------------------------------------------------------------------
-- 12. 项目表（会话分组）
-- -----------------------------------------------------------------------------
CREATE TABLE projects (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id         UUID        NOT NULL REFERENCES depts(id) ON DELETE CASCADE,
    owner_id        UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL,
    description     TEXT,
    cover_color     VARCHAR(7)  NOT NULL DEFAULT '#6366F1',
    is_archived     BOOLEAN     NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_project_name_org UNIQUE (org_id, name)
);

COMMENT ON TABLE projects IS '服装项目/系列（按部门隔离）';

-- -----------------------------------------------------------------------------
-- 13. 知识库集合表
-- -----------------------------------------------------------------------------
CREATE TABLE knowledge_collections (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    dept_id         UUID        REFERENCES depts(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL,
    description     TEXT,
    collection_type VARCHAR(50)  NOT NULL DEFAULT 'document',
    embedding_model VARCHAR(100),
    vector_dim      INT,
    qdrant_collection VARCHAR(255),
    is_public       BOOLEAN     NOT NULL DEFAULT false,
    created_by      UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_kb_name_org UNIQUE (org_id, name)
);

COMMENT ON TABLE knowledge_collections IS '知识库向量集合，qdrant_collection 对应 Qdrant 中的 collection name';
COMMENT ON COLUMN knowledge_collections.collection_type IS 'document|qa|image';

-- -----------------------------------------------------------------------------
-- 14. 知识库访问权限表
-- -----------------------------------------------------------------------------
CREATE TABLE kb_permissions (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id              UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    knowledge_collection_id UUID    NOT NULL REFERENCES knowledge_collections(id) ON DELETE CASCADE,
    dept_id             UUID        REFERENCES depts(id) ON DELETE CASCADE,
    user_id             UUID        REFERENCES users(id) ON DELETE CASCADE,
    permission          VARCHAR(50)  NOT NULL DEFAULT 'read',
    granted_by          UUID        REFERENCES users(id) ON DELETE SET NULL,
    expires_at          TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT chk_kb_perm_target CHECK (
        (dept_id IS NOT NULL)::int + (user_id IS NOT NULL)::int = 1
    )
);

COMMENT ON TABLE kb_permissions IS '知识库权限';

-- -----------------------------------------------------------------------------
-- 15. 审计日志表
-- -----------------------------------------------------------------------------
CREATE TABLE audit_logs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    user_id         UUID        REFERENCES users(id) ON DELETE SET NULL,
    action          VARCHAR(100) NOT NULL,
    resource_type   VARCHAR(100),
    resource_id     UUID,
    details         JSONB       NOT NULL DEFAULT '{}',
    ip_address      INET,
    user_agent      VARCHAR(500),
    session_id      UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE audit_logs IS '全量审计日志，action 如 login|logout|chat|skill_call|mcp_call';

-- -----------------------------------------------------------------------------
-- 16. 迁移记录表
-- -----------------------------------------------------------------------------
-- schema_migrations 表由迁移运行器（api-gateway/internal/db/migrations.go）
-- 独占创建与维护，迁移文件内不再建表/插入，避免与运行器在同一事务内冲突
-- （曾导致 "relation schema_migrations already exists" 与 dirty=t 残留）。

-- -----------------------------------------------------------------------------
-- 17. 索引
-- -----------------------------------------------------------------------------

-- orgs
CREATE INDEX idx_orgs_slug ON orgs(slug);

-- depts
CREATE INDEX idx_depts_org_id     ON depts(org_id);
CREATE INDEX idx_depts_parent_id  ON depts(parent_id);
CREATE INDEX idx_depts_path       ON depts(org_id, path);

-- users
CREATE INDEX idx_users_org_id     ON users(org_id);
CREATE INDEX idx_users_dept_id    ON users(dept_id);
CREATE INDEX idx_users_username   ON users(org_id, username);
CREATE INDEX idx_users_email      ON users(org_id, email) WHERE email IS NOT NULL;

-- skills
CREATE INDEX idx_skills_org_id    ON skills(org_id);
CREATE INDEX idx_skills_name      ON skills(org_id, name);

-- mcp_servers
CREATE INDEX idx_mcp_org_id       ON mcp_servers(org_id);
CREATE INDEX idx_mcp_name         ON mcp_servers(org_id, name);

-- skill_permissions
CREATE INDEX idx_sp_org_id        ON skill_permissions(org_id);
CREATE INDEX idx_sp_skill_id       ON skill_permissions(skill_id);
CREATE INDEX idx_sp_user_id       ON skill_permissions(org_id, user_id) WHERE user_id IS NOT NULL;
CREATE INDEX idx_sp_dept_id       ON skill_permissions(org_id, dept_id) WHERE dept_id IS NOT NULL;

-- mcp_permissions
CREATE INDEX idx_mp_org_id        ON mcp_permissions(org_id);
CREATE INDEX idx_mp_mcp_server_id ON mcp_permissions(mcp_server_id);
CREATE INDEX idx_mp_user_id       ON mcp_permissions(org_id, user_id) WHERE user_id IS NOT NULL;
CREATE INDEX idx_mp_dept_id       ON mcp_permissions(org_id, dept_id) WHERE dept_id IS NOT NULL;

-- roles
CREATE INDEX idx_roles_org_id     ON roles(org_id);

-- sessions
CREATE INDEX idx_sessions_org_user ON sessions(org_id, user_id);
CREATE INDEX idx_sessions_org     ON sessions(org_id);
CREATE INDEX idx_sessions_user    ON sessions(user_id);
CREATE INDEX idx_sessions_updated ON sessions(updated_at DESC);

-- messages
CREATE INDEX idx_messages_session ON messages(session_id, created_at);

-- projects
CREATE INDEX idx_projects_org_id  ON projects(org_id);
CREATE INDEX idx_projects_owner_id ON projects(owner_id);

-- knowledge_collections
CREATE INDEX idx_kb_org_id        ON knowledge_collections(org_id);
CREATE INDEX idx_kb_name          ON knowledge_collections(org_id, name);

-- kb_permissions
CREATE INDEX idx_kbp_org_id       ON kb_permissions(org_id);
CREATE INDEX idx_kbp_collection   ON kb_permissions(knowledge_collection_id);

-- audit_logs
CREATE INDEX idx_audit_org_user   ON audit_logs(org_id, user_id, created_at);
CREATE INDEX idx_audit_resource   ON audit_logs(org_id, resource_type, resource_id) WHERE resource_id IS NOT NULL;
CREATE INDEX idx_audit_action     ON audit_logs(org_id, action, created_at);

-- -----------------------------------------------------------------------------
-- 18. 行级安全策略（RLS）- Phase 1B 启用
--    目前由应用层 Casbin 执行，RLS 作为纵深防御
-- -----------------------------------------------------------------------------

-- -----------------------------------------------------------------------------
-- 19. 初始数据
-- -----------------------------------------------------------------------------

-- 初始组织
INSERT INTO orgs (id, name, slug) VALUES
    ('00000000-0000-0000-0000-000000000001', 'Fashion AI Team', 'fashion-team');

-- 初始部门（根）
INSERT INTO depts (id, org_id, parent_id, name, path) VALUES
    ('00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000001', NULL, 'Root', '/root'),
    ('00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000001', 'Design Team', '/root/design-team'),
    ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', 'Summer Collection', '/root/design-team/summer-collection');

-- 初始用户（密码: admin123，bcrypt hash）
-- hash: $2b$12$1zuEj1MrjeuIH5v1DpAk6.K56JYFFVbZALa3R5qna1JARP2OIZ8lC
INSERT INTO users (id, org_id, dept_id, username, password_hash, display_name, email, role) VALUES
    ('00000000-0000-0000-0000-000000000001',
     '00000000-0000-0000-0000-000000000001',
     '00000000-0000-0000-0000-000000000001',
     'admin',
     '$2b$12$1zuEj1MrjeuIH5v1DpAk6.K56JYFFVbZALa3R5qna1JARP2OIZ8lC',
     '管理员',
     'admin@fashionai.local',
     'admin'),
    ('00000000-0000-0000-0000-000000000002',
     '00000000-0000-0000-0000-000000000001',
     '00000000-0000-0000-0000-000000000002',
     'designer1',
     '$2b$12$1zuEj1MrjeuIH5v1DpAk6.K56JYFFVbZALa3R5qna1JARP2OIZ8lC',
     '设计师张三',
     'designer1@fashionai.local',
     'designer');

-- schema_migrations 版本行由运行器在事务提交时写入，此处不再插入。

-- -----------------------------------------------------------------------------
-- 19. 注释 end
-- -----------------------------------------------------------------------------
