# Phase 1A Kanban Cards — Fashion AI Platform

## Board
- **Slug**: fashion-ai-platform
- **Repo**: ~/projects/fashion-ai-platform

## Card Definitions

### T-001: 项目骨架 + 技术规格定义
```
标题: [T-001] 项目骨架 + 技术规格定义
负责人: architect
依赖: 无
交付物:
  - docs/design/ 目录及技术设计文档
  - api/gateway.yml (OpenAPI 3.0)
  - api/core.yml (Rust 核心 API 规格)
  - Makefile (构建命令)
  - .gitignore (多语言忽略规则)
  - README.md (项目说明)
验收标准:
  - 目录结构符合 SPEC.md 规格
  - API 契约可被 Go/Rust 实现
  - Makefile 包含 build/test/run/dev 命令
  - architect review 通过
```

### T-002: Docker Compose 基础设施
```
标题: [T-002] Docker Compose 基础设施
负责人: backend
依赖: 无
交付物:
  - docker-compose.yml (PG16 + Redis7 + MinIO + Qdrant)
  - Dockerfile.api-gateway (Go 网关)
  - Dockerfile.api-core (Rust 核心)
  - .env.example (所有环境变量)
验收标准:
  - docker compose up -d 启动成功
  - PG 可连接，可执行迁移
  - Redis 可 ping
  - MinIO 控制台可访问
  - Qdrant 可访问
```

### T-003: 数据库迁移脚本
```
标题: [T-003] 数据库迁移脚本
负责人: backend
依赖: T-002 (Docker 就绪)
交付物:
  - migrations/001_init_schema.sql
  - api-gateway/internal/db/migrations.go
  - SQLx 编译时检查配置
验收标准:
  - 迁移脚本可独立执行
  - SQLx offline 检查通过
  - 表结构符合 SPEC.md Schema
```

### T-004: Go API 网关
```
标题: [T-004] Go API 网关
负责人: backend
依赖: T-001 (API 规格)
交付物:
  - api-gateway/ (完整 Go 服务)
  - JWT 认证中间件
  - /v1/chat/completions 端点 (OpenAI 兼容)
  - /ws/chat WebSocket 端点
  - /health 健康检查
验收标准:
  - curl 登录 → 获取 JWT
  - JWT 调用 /v1/chat/completions → 200 OK
  - stream=true → SSE 格式输出
  - 缺少 Authorization → 401
```

### T-005: Rust 核心服务
```
标题: [T-005] Rust 核心服务
负责人: backend
依赖: T-001 (API 规格)
交付物:
  - api-core/ (codex-rs fork 改造)
  - LLM API 接入 (MiniMax/DeepSeek)
  - 基础 Agent Loop
  - SSE 流式输出
  - 会话管理 (PostgreSQL + Redis)
验收标准:
  - 对接 Go 网关的 /v1/chat/completions 请求
  - 流式输出符合 OpenAI 格式
  - 会话消息持久化到 PostgreSQL
```

### T-006: 用户系统
```
标题: [T-006] 用户系统
负责人: backend
依赖: T-003 (DB schema)
交付物:
  - POST /auth/login
  - POST /auth/register
  - GET /api/depts, POST /api/depts
  - GET /api/roles, POST /api/roles
  - 用户密码 bcrypt 哈希
  - JWT 签发
验收标准:
  - 注册 → 登录 → 获取 token → 调用 /v1/chat/completions 完整链路
  - 部门层级支持 (parent_id + path)
  - RBAC 基础角色 (admin/designer/viewer)
```

### T-007: 前端最小闭环 UI
```
标题: [T-007] 前端最小闭环 UI
负责人: frontend
依赖: T-001 (API 契约)
交付物:
  - web-client/ (Vite + React + TS)
  - 登录页 (用户名 + 密码)
  - 对话窗口 (消息列表 + 输入框)
  - SSE 流式消息展示
  - 基础会话历史侧边栏
验收标准:
  - npm run dev 可启动
  - 登录成功后 JWT 存 localStorage
  - 发送消息 → 显示 AI 流式回复
  - 响应式布局 (移动端可读)
```

### T-008: 设计规范 + 基础 SKILL.md
```
标题: [T-008] 设计规范 + 基础 SKILL.md
负责人: designer
依赖: 无
交付物:
  - design/design-system.md (颜色/字体/间距/组件规范)
  - skills/fabric-query/SKILL.md
  - skills/color-matching/SKILL.md
验收标准:
  - 设计系统文档包含色板、组件规范
  - SKILL.md 符合规范格式 (frontmatter + 描述 + 工具)
  - designer review 通过
```

### T-009: E2E 测试框架 + 验收
```
标题: [T-009] E2E 测试框架 + 验收
负责人: ui-qa
依赖: T-004, T-005, T-006, T-007
交付物:
  - tests/e2e/login.spec.ts
  - tests/e2e/chat.spec.ts
  - tests/e2e/session-history.spec.ts
  - 门禁报告
验收标准:
  - Playwright E2E 可运行
  - 登录 → 对话 → AI 回复 E2E 通过
  - 门禁报告 (reviewer 签字 + ui-qa 结果)
```

### T-010: PR 审查 + 交付门禁
```
标题: [T-010] PR 审查 + 交付门禁
负责人: reviewer
依赖: 各 PR 完成后
交付物:
  - 各模块代码审查意见
  - 交付门禁报告
验收标准:
  - reviewer 审核通过所有 PR
  - 门禁通过 (reviewer + ui-qa)
```

## 派发顺序

**立即派发（无依赖）**：
- T-001 (architect)
- T-002 (backend)
- T-008 (designer)

**等待 T-001 完成后派发**：
- T-004 (backend) — 依赖 API 规格
- T-005 (backend) — 依赖 API 规格
- T-007 (frontend) — 依赖 API 契约

**等待 T-002 完成后派发**：
- T-003 (backend) — 依赖 Docker 就绪

**等待 T-003 完成后派发**：
- T-006 (backend) — 依赖 DB schema

**等待 T-004~T-007 完成后派发**：
- T-009 (ui-qa) — 依赖前后端完成

**所有 PR 完成后**：
- T-010 (reviewer) �� 最终门禁
