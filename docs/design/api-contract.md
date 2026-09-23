# Fashion AI Platform — API 契约文档

> 版本：1.0.0
> 基于 SPEC.md 核心接口清单

---

## 1. 认证

### 1.1 JWT 认证

所有需要认证的接口，支持两种认证方式：

**方式一：Bearer Token（推荐用于 API 消费者）**

```
Authorization: Bearer ***
```

**方式二：HttpOnly Cookie（Web 前端默认）**

登录成功后，服务器通过 `Set-Cookie` 设置 HttpOnly Cookie，前端 JS 无法访问，防 XSS 窃取。

Cookie 策略：`HttpOnly; SameSite=Lax; Secure`（生产环境需 HTTPS）

### 1.2 认证优先级

网关鉴权中间件优先检查 Cookie，Cookie 不存在或无效时回退检查 Authorization Bearer。

> **注意**：OpenAI 兼容接口 `/v1/chat/completions` 继续只接受 Bearer Token，不接受 Cookie（API 消费者场景）。

### 1.3 登出

```
POST /auth/logout
```

清除 HttpOnly Cookie。响应：`200 OK`（无 Body）。

### 1.4 JWT Claims 结构

```json
{
  "sub": "user_id_uuid",
  "org_id": "org_uuid",
  "dept_id": "dept_uuid",
  "role": "designer | admin | viewer",
  "exp": 1234567890,
  "iat": 1234567800
}
```

### 1.5 错误响应格式

```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Invalid or expired token",
    "details": {}
  }
}
```

| HTTP 状态码 | 错误码 | 说明 |
|-------------|--------|------|
| 400 | BAD_REQUEST | 请求参数错误 |
| 401 | UNAUTHORIZED | 未认证或 Token 无效 |
| 403 | FORBIDDEN | 无权限 |
| 404 | NOT_FOUND | 资源不存在 |
| 429 | RATE_LIMITED | 速率超限 |
| 500 | INTERNAL_ERROR | 服务器错误 |

---

## 2. 认证接口

### 2.1 POST /auth/register

注册用户（同时创建组织和默认部门）。

**请求**

```json
{
  "username": "designer1",
  "password": "password123",
  "display_name": "设计师甲",
  "email": "designer@example.com",
  "org_name": "我的团队",
  "dept_name": "设计部"
}
```

**响应** `201 Created`

```json
{
  "user_id": "uuid",
  "org_id": "uuid",
  "dept_id": "uuid",
  "username": "designer1",
  "display_name": "设计师甲",
  "role": "admin",
  "token": "eyJhbGciOiJIUzI1NiIs..."
}
```

### 2.2 POST /auth/login

用户登录。

**请求**

```json
{
  "username": "designer1",
  "password": "password123"
}
```

**响应** `200 OK`

```json
{
  "user_id": "uuid",
  "org_id": "uuid",
  "dept_id": "uuid",
  "username": "designer1",
  "display_name": "设计师甲",
  "role": "designer",
  "token": "eyJhbGciOiJIUzI1NiIs..."
}
```

---

## 3. OpenAI 兼容接口

### 3.1 POST /v1/chat/completions

核心对话接口，100% OpenAI 兼容格式。

**请求头**

```
Authorization: Bearer <jwt_token>
Content-Type: application/json
```

**请求体**（标准 OpenAI 格式 + 扩展）

```json
{
  "model": "gpt-4o",
  "messages": [
    {"role": "system", "content": "你是一个服装设计助手。"},
    {"role": "user", "content": "帮我设计一款春装连衣裙"}
  ],
  "stream": true,
  "temperature": 0.7,
  "max_tokens": 2000,
  "extra_body": {
    "session_id": "sess_xxx",
    "skill_ids": ["fabric-query"],
    "mcp_server_ids": [],
    "knowledge_collections": ["fabric-db"]
  }
}
```

**扩展字段**（`extra_body`）：

| 字段 | 类型 | 说明 |
|------|------|------|
| `session_id` | string | 会话 ID（可选，创建新会话时省略） |
| `skill_ids` | string[] | 使用的 Skill 列表 |
| `mcp_server_ids` | string[] | 使用的 MCP Server 列表 |
| `knowledge_collections` | string[] | 知识库集合 |

**响应** `200 OK`（stream=true，SSE 格式）

```
data: {"id":"chatcmpl_xxx","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"chatcmpl_xxx","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"你好"},"finish_reason":null}]}

data: {"id":"chatcmpl_xxx","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"，我是"},"finish_reason":null}]}

data: [DONE]
```

**响应** `200 OK`（stream=false）

```json
{
  "id": "chatcmpl_xxx",
  "object": "chat.completion",
  "created": 1234567890,
  "model": "gpt-4o",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "你好，我是你的服装设计助手..."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 100,
    "completion_tokens": 200,
    "total_tokens": 300
  }
}
```

### 3.2 GET /v1/models

模型列表。

**响应** `200 OK`

```json
{
  "object": "list",
  "data": [
    {
      "id": "gpt-4o",
      "object": "model",
      "created": 1234567890,
      "owned_by": "minimax"
    },
    {
      "id": "deepseek-chat",
      "object": "model",
      "created": 1234567890,
      "owned_by": "deepseek"
    }
  ]
}
```

---

## 4. 用户管理接口

### 4.1 GET /api/users

获取当前组织用户列表（管理员）。

**响应** `200 OK`

```json
{
  "users": [
    {
      "id": "uuid",
      "username": "designer1",
      "display_name": "设计师甲",
      "email": "designer@example.com",
      "dept_id": "uuid",
      "dept_name": "设计部",
      "role": "admin",
      "is_active": true,
      "created_at": "2024-01-01T00:00:00Z"
    }
  ],
  "total": 10
}
```

---

## 5. 部门管理接口

### 5.1 GET /api/depts

部门列表（当前组织）。

**响应** `200 OK`

```json
{
  "depts": [
    {
      "id": "uuid",
      "name": "设计部",
      "parent_id": null,
      "path": "/root/设计部",
      "created_at": "2024-01-01T00:00:00Z"
    },
    {
      "id": "uuid",
      "name": "男装组",
      "parent_id": "dept_uuid",
      "path": "/root/设计部/男装组",
      "created_at": "2024-01-01T00:00:00Z"
    }
  ]
}
```

### 5.2 POST /api/depts

创建部门。

**请求**

```json
{
  "name": "女装组",
  "parent_id": "dept_uuid"
}
```

**响应** `201 Created`

```json
{
  "id": "uuid",
  "name": "女装组",
  "parent_id": "dept_uuid",
  "path": "/root/设计部/女装组",
  "created_at": "2024-01-01T00:00:00Z"
}
```

---

## 6. 角色管理接口

### 6.1 GET /api/roles

角色列表。

**响应** `200 OK`

```json
{
  "roles": [
    {
      "id": "uuid",
      "name": "admin",
      "permissions": ["*"],
      "created_at": "2024-01-01T00:00:00Z"
    },
    {
      "id": "uuid",
      "name": "designer",
      "permissions": ["chat:create", "session:read", "session:write"],
      "created_at": "2024-01-01T00:00:00Z"
    },
    {
      "id": "uuid",
      "name": "viewer",
      "permissions": ["chat:create", "session:read"],
      "created_at": "2024-01-01T00:00:00Z"
    }
  ]
}
```

### 6.2 POST /api/roles

创建角色。

**请求**

```json
{
  "name": "senior-designer",
  "permissions": ["chat:create", "session:read", "session:write", "skill:fabric-query"]
}
```

**响应** `201 Created`

```json
{
  "id": "uuid",
  "name": "senior-designer",
  "permissions": ["chat:create", "session:read", "session:write", "skill:fabric-query"],
  "created_at": "2024-01-01T00:00:00Z"
}
```

---

## 7. 会话管理接口

### 7.1 POST /api/sessions

创建会话。

**请求**

```json
{
  "title": "2024 春夏连衣裙设计"
}
```

**响应** `201 Created`

```json
{
  "id": "uuid",
  "title": "2024 春夏连衣裙设计",
  "is_archived": false,
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z"
}
```

### 7.2 GET /api/sessions

会话列表。

**查询参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `archived` | boolean | 是否归档（默认 false） |
| `limit` | int | 每页数量（默认 20） |
| `offset` | int | 偏移量 |

**响应** `200 OK`

```json
{
  "sessions": [
    {
      "id": "uuid",
      "title": "2024 春夏连衣裙设计",
      "is_archived": false,
      "message_count": 15,
      "created_at": "2024-01-01T00:00:00Z",
      "updated_at": "2024-01-01T01:00:00Z"
    }
  ],
  "total": 50
}
```

### 7.3 GET /api/sessions/:id

获取会话详情。

**响应** `200 OK`

```json
{
  "id": "uuid",
  "title": "2024 春夏连衣裙设计",
  "is_archived": false,
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T01:00:00Z"
}
```

### 7.4 GET /api/sessions/:id/messages

获取会话消息历史。

**查询参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `limit` | int | 每页数量（默认 50） |
| `before` | string | 游标（最后一条消息的 id） |

**响应** `200 OK`

```json
{
  "messages": [
    {
      "id": "uuid",
      "role": "user",
      "content": "帮我设计一款春装连衣裙",
      "model": null,
      "token_count": null,
      "created_at": "2024-01-01T00:00:00Z"
    },
    {
      "id": "uuid",
      "role": "assistant",
      "content": "好的，我来帮你设计一款适合春天的连衣裙...",
      "model": "gpt-4o",
      "token_count": 350,
      "created_at": "2024-01-01T00:00:01Z"
    }
  ],
  "has_more": true,
  "next_cursor": "uuid_of_last_message"
}
```

### 7.5 PATCH /api/sessions/:id

更新会话（如归档）。

**请求**

```json
{
  "title": "2024 春夏连衣裙设计（定稿）",
  "is_archived": true
}
```

---

## 8. WebSocket 接口

### 8.1 GET /ws/chat

WebSocket 实时对话。

**连接** `ws://localhost:8080/ws/chat?token=<jwt_token>`

**客户端发送**

```json
{
  "type": "message",
  "session_id": "sess_xxx",
  "content": "帮我设计一款春装连衣裙",
  "model": "gpt-4o",
  "stream": true,
  "extra_body": {
    "skill_ids": ["fabric-query"]
  }
}
```

**服务端推送**

```json
{
  "type": "chunk",
  "session_id": "sess_xxx",
  "content": "好的，"
}
```

```json
{
  "type": "chunk",
  "session_id": "sess_xxx",
  "content": "我来帮你设计..."
}
```

```json
{
  "type": "done",
  "session_id": "sess_xxx",
  "message_id": "msg_uuid"
}
```

```json
{
  "type": "error",
  "error": "RATE_LIMITED",
  "message": "请求过于频繁"
}
```

---

## 9. 健康检查

### 9.1 GET /health

**响应** `200 OK`

```json
{
  "status": "ok",
  "version": "1.0.0",
  "services": {
    "database": "ok",
    "redis": "ok",
    "rust_core": "ok"
  }
}
```

---

## 10. 路由汇总

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| POST | /auth/login | 否 | 用户登录 |
| POST | /auth/register | 否 | 用户注册 |
| GET | /health | 否 | 健康检查 |
| GET | /v1/models | 是 | 模型列表 |
| POST | /v1/chat/completions | 是 | 对话（核心） |
| GET | /ws/chat | 是（URL 参数）| WebSocket 实时对话 |
| GET | /api/users | 是（admin）| 用户列表 |
| GET | /api/depts | 是 | 部门列表 |
| POST | /api/depts | 是 | 创建部门 |
| GET | /api/roles | 是 | 角色列表 |
| POST | /api/roles | 是 | 创建角色 |
| GET | /api/sessions | 是 | 会话列表 |
| POST | /api/sessions | 是 | 创建会话 |
| GET | /api/sessions/:id | 是 | 会话详情 |
| GET | /api/sessions/:id/messages | 是 | 消息历史 |
| PATCH | /api/sessions/:id | 是 | 更新会话 |
