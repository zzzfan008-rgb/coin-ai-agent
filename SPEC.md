# Fashion AI Platform — Phase 1A Specification

> 本文件为 architect 的输出规格，所有子任务以此为准。

## 项目信息
- **产品**：Fashion AI Platform — 服装设计行业本地部署 AI SaaS
- **目标用户**：20 人服装设计团队（无技术背景）
- **阶段**：Phase 1A（第 1-4 周）

## 技术栈
| 模块 | 技术 |
|------|------|
| API 网关 | Go 1.21+ |
| 核心服务 | Rust (codex-rs fork) |
| 前端 | React 18 + TypeScript |
| 数据库 | PostgreSQL 16 + pgvector |
| 向量库 | Qdrant（本地） |
| 缓存 | Redis 7 |
| 对象存储 | MinIO（S3 协议） |
| 沙盒 | Docker（容器级） |
| 权限 | Casbin（执行层拦截） |
| LLM API | MiniMax / DeepSeek（仅此层出公网） |

## 核心约束
1. 本地部署：所有服务运行在企业内网，不依赖境外云服务
2. 仅 LLM API 出公网：仅 MiniMax/DeepSeek 调用走外网
3. 会话永久保留：支持项目归档，不删除历史会话
4. RBAC 三层隔离：用户 / 角色 / 部门，org_id + dept_id 隔离
5. 执行层细粒度权限：tool 调用前拦截（Casbin PreToolCall Hook）

## Phase 1A 交付目标
**最小闭环**：用户登录 → 发送对话 → AI 流式回复

### 子任务清单

| ID | 任务 | 负责人 | 依赖 | 交付物 |
|----|------|--------|------|--------|
| T-001 | 项目骨架 + 技术规格 | architect | — | docs/design/、api/**、目录结构 |
| T-002 | Docker Compose 基础设施 | backend | — | docker-compose.yml、Dockerfile |
| T-003 | 数据库迁移脚本 | backend | T-002 | migrations/*.sql、SQLx 配置 |
| T-004 | Go API 网关 | backend | T-001 | api-gateway/**（JWT、OpenAI 兼容、WS） |
| T-005 | Rust 核心服务 | backend | T-001 | api-core/**（Agent Loop、SSE、会话） |
| T-006 | 用户系统 | backend | T-003 | 注册/登录/部门/角色 API |
| T-007 | 前端最小闭环 UI | frontend | T-001 | web-client/**（登录页 + 对话窗口） |
| T-008 | 设计规范 + 基础 SKILL.md | designer | — | design/、skills/fabric-query/SKILL.md |
| T-009 | E2E 测试框架 | ui-qa | T-004~T-007 | tests/e2e/**、门禁报告 |
| T-010 | PR 审查 + 交付门禁 | reviewer | 各 PR | 审查意见、门禁报告 |

### T-001 详细规格（architect 输出）

#### 目录结构
```
fashion-ai-platform/
├── docs/design/          # 技术设计文档
├── api/                  # API 契约（OpenAPI 3.0）
│   ├── gateway.yml       # Go 网关 API 规格
│   └── core.yml          # Rust 核心 API 规格
├── api-gateway/          # Go 网关服务
│   ├── cmd/server/
│   ├── internal/
│   └── pkg/
├── api-core/             # Rust 核心服务 (codex-rs fork)
│   ├── src/
│   └── Cargo.toml
├── web-client/           # React 前端
├── skills/               # Skills 注册表
├── migrations/           # 数据库迁移
├── docker-compose.yml
└── Makefile
```

#### API 契约（OpenAI 兼容）
```
POST /v1/chat/completions
Headers:
  Authorization: Bearer <JWT>
  Content-Type: application/json

Request Body（OpenAI 格式 + 扩展）:
{
  "model": "gpt-4o",
  "messages": [{"role": "user", "content": "..."}],
  "stream": true,
  "extra_body": {
    "session_id": "sess_xxx",      // 会话 ID
    "skill_ids": ["fabric-query"],  // 使用的 Skill
    "mcp_server_ids": [],          // 使用的 MCP Server
    "knowledge_collections": []     // 知识库集合
  }
}

Response（流式 SSE）:
data: {"id":"chatcmpl_xxx","choices":[{"delta":{"role":"assistant","content":"..."}}]}
data: {"id":"chatcmpl_xxx","choices":[{"delta":{"content":"..."}}]}
data: [DONE]
```

#### 核心接口清单
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | /v1/chat/completions | OpenAI 兼容对话（核心） |
| GET | /v1/models | 模型列表 |
| POST | /auth/login | 用户登录 |
| POST | /auth/register | 用户注册 |
| GET | /api/users | 用户列表（管理员） |
| GET | /api/depts | 部门列表 |
| POST | /api/depts | 创建部门 |
| GET | /api/roles | 角色列表 |
| POST | /api/roles | 创建角色 |
| POST | /api/sessions | 创建会话 |
| GET | /api/sessions/:id | 获取会话 |
| GET | /api/sessions/:id/messages | 获取消息历史 |

#### 数据库 Schema（Phase 1A 范围）
```sql
-- 组织
CREATE TABLE orgs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(255) NOT NULL,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 部门（含层级 path）
CREATE TABLE depts (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id UUID NOT NULL REFERENCES orgs(id),
  parent_id UUID REFERENCES depts(id),
  name VARCHAR(255) NOT NULL,
  path VARCHAR(1000) NOT NULL,  -- e.g., "/root/design-team"
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 用户
CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id UUID NOT NULL REFERENCES orgs(id),
  dept_id UUID NOT NULL REFERENCES depts(id),
  username VARCHAR(100) NOT NULL,
  password_hash VARCHAR(255) NOT NULL,
  display_name VARCHAR(255),
  email VARCHAR(255),
  role VARCHAR(50) NOT NULL DEFAULT 'designer',  -- designer | admin | viewer
  is_active BOOLEAN DEFAULT true,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 会话
CREATE TABLE sessions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id UUID NOT NULL REFERENCES orgs(id),
  user_id UUID NOT NULL REFERENCES users(id),
  title VARCHAR(500),
  is_archived BOOLEAN DEFAULT false,
  created_at TIMESTAMPTZ DEFAULT NOW(),
  updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- 消息
CREATE TABLE messages (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  session_id UUID NOT NULL REFERENCES sessions(id),
  role VARCHAR(50) NOT NULL,  -- user | assistant | system | tool
  content TEXT NOT NULL,
  model VARCHAR(100),
  token_count INT,
  metadata JSONB,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 角色定义表
CREATE TABLE roles (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id UUID NOT NULL REFERENCES orgs(id),
  name VARCHAR(100) NOT NULL,
  permissions JSONB NOT NULL DEFAULT '[]',
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 审计日志
CREATE TABLE audit_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id UUID NOT NULL REFERENCES orgs(id),
  user_id UUID REFERENCES users(id),
  action VARCHAR(100) NOT NULL,
  resource_type VARCHAR(100),
  resource_id UUID,
  details JSONB,
  ip_address INET,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 索引
CREATE INDEX idx_sessions_org_user ON sessions(org_id, user_id);
CREATE INDEX idx_messages_session ON messages(session_id, created_at);
CREATE INDEX idx_audit_org_user ON audit_logs(org_id, user_id, created_at);
CREATE INDEX idx_depts_path ON depts(org_id, path);
CREATE INDEX idx_users_org ON users(org_id);
```

### T-004 Go 网关详细规格

#### 技术要求
- Go 1.21+
- JWT 认证（HS256，密钥从环境变量加载）
- OpenAI 兼容 `/v1/chat/completions` 端点（100% 兼容格式）
- WebSocket 支持（`/ws/chat`）
- SSE 流式输出
- 粗粒度 RBAC（路由级权限校验）
- 请求速率限制（Redis）
- 审计日志中间件
- 健康检查端点（`/health`）

#### JWT Claims 结构
```json
{
  "sub": "user_id_uuid",
  "org_id": "org_uuid",
  "dept_id": "dept_uuid",
  "role": "designer",
  "exp": 1234567890,
  "iat": 1234567800
}
```

#### 环境变量
```
PORT=8080
JWT_SECRET=<min 32 chars>
DATABASE_URL=postgres://user:pass@localhost:5432/fashion_ai
REDIS_URL=redis://localhost:6379
MINIMAX_API_KEY=<for LLM calls>
DEEPSEEK_API_KEY=<for LLM calls>
RUST_CORE_URL=http://localhost:8081
```

### T-005 Rust 核心详细规格

#### 技术要求
- Rust 1.76+
- 基于 codex-rs fork（https://github.com/nickcoutsos/codex-rs 或 fork）
- 基础 Agent Loop（循环调用 LLM + 工具）
- SSE 流式输出到网关
- 会话管理（PostgreSQL + Redis）
- 支持流式 `/v1/chat/completions` 格式输出
- 预留 Casbin 权限拦截 Hook（Phase 1B 接入）

#### LLM 调用
- 调用 MiniMax/DeepSeek API（仅此层出公网）
- 支持 stream=true
- 模型名称映射配置

### T-007 前端详细规格

#### 技术要求
- React 18 + TypeScript
- Vite 构建
- 登录页（用户名 + 密码，JWT token 存 localStorage）
- 对话窗口（消息列表 + 输入框）
- 流式消息展示（SSE 接收）
- 基础会话历史侧边栏
- 响应式布局

### T-008 设计规范

#### 设计语言
- 简洁专业，适合设计师群体
- 深色主题为主
- 中文界面

#### SKILL.md 格式
```markdown
---
name: fabric-query
version: 1.0.0
description: 面料咨询 Skill，根据面料名称查询成分、适用季节、保养方式
author: Fashion AI Team
tags: [fabric, consultation, fashion]
permissions: [skill:fabric-query]
---

# Fabric Query Skill

## 描述
根据面料名称或成分，快速返回专业咨询建议。

## 工具
- fabric_search(query: string): 返回面料匹配结果
- fabric_detail(id: string): 返回面料详情

## 提示词模板
你是一个面料专家。用户询问面料相关信息时，使用 fabric_search 和 fabric_detail 工具回答。
```

## 验收标准
1. 用户注册 → 登录 → 获取 JWT token
2. JWT token 调用 `/v1/chat/completions` → 流式返回 AI 回复
3. WebSocket `/ws/chat` → 实时双向对话
4. 会话历史可查询
5. Docker Compose 一键启动所有服务
