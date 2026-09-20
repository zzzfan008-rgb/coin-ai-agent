# Fashion AI Platform — 系统架构设计

> 版本：1.0.0
> 负责人：architect
> 基于 SPEC.md 定义

---

## 1. 系统概述

Fashion AI Platform 是面向服装设计团队的本地部署 AI SaaS，目标是最小闭环：**用户登录 → 发送对话 → AI 流式回复**。

Phase 1A 聚焦最小闭环交付，Phase 1B 引入 Skill/MCP 执行层。

---

## 2. 整体架构

```
┌─────────────────────────────────────────────────────────┐
│                      客户端层                            │
│  (Web Browser / Mobile / API Consumer)                  │
└─────────────────┬───────────────────────────────────────┘
                  │ HTTP/WebSocket
                  ▼
┌─────────────────────────────────────────────────────────┐
│                    Go API 网关                           │
│  · JWT 认证（HS256）                                    │
│  · 粗粒度 RBAC 路由权限                                  │
│  · 请求速率限制（Redis）                                │
│  · 审计日志中间件                                       │
│  · OpenAI 兼容端点（/v1/chat/completions）             │
│  · WebSocket 端点（/ws/chat）                          │
│  · SSE 流式响应                                         │
│  端口：8080                                             │
└────────┬──────────────────────────┬─────────────────────┘
         │ HTTP/gRPC               │ HTTP
         ▼                          ▼
┌──────────────────┐      ┌──────────────────────────────┐
│   Rust 核心服务   │      │        PostgreSQL 16         │
│  · Agent Loop    │◄────►│  + pgvector（向量存储）       │
│  · LLM 调用      │      │  端口：5432                   │
│  · SSE 流输出    │      └──────────────────────────────┘
│  · 会话管理      │
│  端口：8081      │
└────┬────┬───────┘
     │    │
     ▼    ▼
┌─────────┐  ┌────────┐  ┌────────┐  ┌──────────┐
│  Redis  │  │ MinIO  │  │ Qdrant │  │ LLM API  │
│  7      │  │ S3     │  │ 向量库  │  │ MiniMax  │
│  端口6379│  │ 端口9000│  │ 端口6333│  │ DeepSeek │
└─────────┘  └────────┘  └────────┘  └──────────┘
                                           （出公网）
```

---

## 3. 各层职责

### 3.1 客户端层

- **Web 前端**（React 18 + TypeScript + Vite）
  - 登录页：用户名 + 密码，JWT 存 localStorage
  - 对话窗口：消息列表 + 输入框 + 流式消息展示
  - 会话历史侧边栏
  - 响应式布局

### 3.2 Go API 网关

职责边界：**所有入站请求在此汇聚**，向下游 Rust 服务转发。

| 功能 | 实现 |
|------|------|
| JWT 认证 | HS256，密钥从 `JWT_SECRET` 环境变量加载 |
| 粗粒度 RBAC | 路由级权限校验（`org_id` + `role` 校验） |
| 速率限制 | Redis 滑动窗口 |
| 审计日志 | 请求元数据写入 `audit_logs` 表 |
| OpenAI 兼容 | `/v1/chat/completions` 100% 兼容格式 |
| WebSocket | `/ws/chat` 实时双向对话 |
| 健康检查 | `/health` |
| 代理转发 | 鉴权后转发到 Rust 核心服务 |

### 3.3 Rust 核心服务

职责边界：**AI 推理执行层**，仅 Go 网关可调用。

| 功能 | 实现 |
|------|------|
| Agent Loop | 循环调用 LLM + 工具（Phase 1B 接入 Skills） |
| LLM 调用 | MiniMax / DeepSeek（**仅此层出公网**） |
| SSE 流输出 | tokio + axum，流式返回 OpenAI SSE 格式 |
| 会话管理 | PostgreSQL（消息持久化）+ Redis（会话缓存） |
| Casbin Hook | 预留 PreToolCall 拦截（Phase 1B 接入） |

### 3.4 数据层

#### PostgreSQL 16 + pgvector

- **表**：orgs, depts, users, sessions, messages, roles, audit_logs
- **隔离**：`org_id` + `dept_id` 双重隔离
- **向量**：pgvector 扩展，支持服装图片/设计稿向量检索（Phase 1B+）

#### Redis 7

- 会话缓存（TTL 24h）
- 速率限制计数器
- 分布式锁（会话操作）

#### MinIO（S3 协议）

- 设计稿文件存储
- AI 生成内容存储

#### Qdrant（本地向量库）

- 服装知识库向量检索
- Phase 1B 接入

---

## 4. 数据流向

### 4.1 对话请求流

```
1. 浏览器 → POST /v1/chat/completions (Bearer JWT)
2. Go 网关：JWT 校验 → RBAC 路由检查 → Redis 速率检查 → 审计日志
3. Go 网关 → Rust 核心（内部 HTTP，携带 JWT claims）
4. Rust 核心：查询会话历史 → 组装 prompt → 调用 MiniMax/DeepSeek API
5. Rust 核心 ← LLM API（流式 response）
6. Rust 核心 → SSE 流写入响应体
7. Go 网关 → 浏览器（SSE 流）
8. Rust 核心：消息持久化到 PostgreSQL（异步）
```

### 4.2 WebSocket 实时对话

```
1. 浏览器 → WebSocket /ws/chat (JWT as query param)
2. Go 网关：握手校验 → 升级到 WebSocket
3. 双向消息：浏览器发送 {message, session_id} → Rust 核心处理
4. Rust 核心：流式响应写回 WebSocket
5. 浏览器显示实时 AI 回复
```

---

## 5. 安全模型

### 5.1 网络隔离

- 所有服务运行在企业内网
- **仅 Rust 核心调用 LLM API 出公网**（MiniMax / DeepSeek）
- Go 网关无公网出口

### 5.2 JWT Claims 结构

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

### 5.3 RBAC 三层隔离

| 维度 | 字段 | 说明 |
|------|------|------|
| 组织 | `org_id` | 数据隔离边界 |
| 部门 | `dept_id` | 团队隔离 |
| 角色 | `role` | admin / designer / viewer |

### 5.4 执行层权限（Phase 1B）

- Casbin PreToolCall Hook：tool 调用前拦截
- Skill 权限矩阵：`skill:fabric-query`、`mcp:search-engine` 等
- 配置化权限表，动态生效

---

## 6. 会话管理策略

- **永久保留**：会话不删除，支持归档（`is_archived` 标记）
- **消息持久化**：每条消息（含 AI 回复）立即写入 PostgreSQL
- **会话缓存**：Redis 缓存活跃会话元数据（TTL 24h）
- **会话列表**：支持按时间/标题/归档状态筛选

---

## 7. 部署架构

### 7.1 Docker Compose（Phase 1A）

所有服务通过 `docker-compose.yml` 一键启动：

```yaml
services:
  postgres:    # PG16 + pgvector
  redis:       # Redis 7
  minio:       # 对象存储
  qdrant:      # 向量库
  api-gateway: # Go 网关
  api-core:    # Rust 核心
  web-client:  # React 前端 (可选 nginx 部署)
```

### 7.2 端口分配

| 服务 | 端口 |
|------|------|
| Go 网关 | 8080 |
| Rust 核心 | 8081 |
| PostgreSQL | 5432 |
| Redis | 6379 |
| MinIO API | 9000 |
| MinIO Console | 9001 |
| Qdrant | 6333 |
| 前端 | 3000 |

---

## 8. 扩展路径（Phase 1B+）

| 功能 | 说明 |
|------|------|
| Skill 接入 | `skills/` 目录下 Skill MD 注册，Agent Loop 调用 |
| MCP Server | Model Context Protocol，多工具集成 |
| 服装知识库 | Qdrant + pgvector 向量检索 |
| 设计稿分析 | 图片上传 → 向量化 → 相似款推荐 |
| 多模态 | 图片输入 → 服装款式识别 |

---

## 9. 技术决策记录

| ID | 决策 | 理由 |
|----|------|------|
| ADR-001 | Go 网关 + Rust 核心分离 | 职责分离，Go 处理 I/O，Rust 处理计算密集 LLM |
| ADR-002 | SSE 而非 WebSocket 作为主协议 | SSE 更适合 Server→Client 流式推送，WebSocket 备用 |
| ADR-003 | 仅 LLM 层出公网 | 企业内网安全要求，最小攻击面 |
| ADR-004 | PostgreSQL + Redis 会话 | 消息持久化 + 活跃会话缓存平衡 |
| ADR-005 | JWT + Casbin 双层权限 | 粗粒度网关 + 细粒度执行层分离 |
