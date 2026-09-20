# Fashion AI Platform

服装设计行业本地部署 AI SaaS，面向 20 人设计团队。

## 技术栈

| 模块 | 技术 |
|------|------|
| API 网关 | Go 1.21+ |
| 核心服务 | Rust (codex-rs) |
| 前端 | React 18 + TypeScript |
| 数据库 | PostgreSQL 16 + pgvector |
| 向量库 | Qdrant（本地） |
| 缓存 | Redis 7 |
| 对象存储 | MinIO（S3 协议） |
| 沙盒 | Docker |
| LLM API | MiniMax / DeepSeek |

## 目录结构

```
fashion-ai-platform/
├── api/                  # OpenAPI 契约（gateway.yml, core.yml）
├── api-gateway/          # Go 网关服务
├── api-core/             # Rust 核心服务
├── web-client/           # React 前端
├── skills/               # Skills 注册表
├── migrations/            # 数据库迁移脚本
├── tests/e2e/            # E2E 测试
├── docs/design/          # 技术设计文档
├── docker-compose.yml    # 基础设施
└── Makefile
```

## 快速开始

### 前置依赖

- Docker & Docker Compose
- Go 1.21+
- Rust 1.76+
- Node.js 20+

### 1. 克隆并启动基础设施

```bash
git clone <repo-url>
cd fashion-ai-platform
make docker-up
```

等待所有服务就绪（约 10 秒）。

### 2. 初始化数据库

```bash
make db-migrate
```

### 3. 启动所有服务

```bash
make dev
```

服务地址：
- 前端：http://localhost:3000
- API 网关：http://localhost:8080
- Rust 核心：http://localhost:8081
- MinIO 控制台：http://localhost:9001

### 4. 验证最小闭环

```bash
# 注册用户
curl -X POST http://localhost:8080/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"designer1","password":"password123","display_name":"设计师甲","org_name":"我的团队","dept_name":"设计部"}'

# 登录获取 JWT
TOKEN=$(curl -s -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"designer1","password":"password123"}' | jq -r '.token')

# 发起对话（流式）
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "你好"}],
    "stream": true
  }'
```

## Makefile 命令

```bash
make help           # 显示所有命令
make docker-up      # 启动基础设施
make docker-down    # 停止所有服务
make db-migrate     # 运行数据库迁移
make dev            # 开发模式（启动所有服务）
make build          # 构建所有模块
make test           # 运行所有测试
make lint           # 代码质量检查
make fmt            # 代码格式化
make clean          # 清理构建产物
```

## 环境变量

复制 `.env.example` 为 `.env` 并填入实际值：

```bash
cp .env.example .env
```

关键变量：
- `JWT_SECRET`：JWT 签名密钥（≥32 字符）
- `DATABASE_URL`：PostgreSQL 连接串
- `REDIS_URL`：Redis 连接串
- `MINIMAX_API_KEY`：MiniMax API Key
- `DEEPSEEK_API_KEY`：DeepSeek API Key
- `RUST_CORE_URL`：Rust 核心服务地址

## 核心约束

1. **本地部署**：所有服务运行在企业内网，不依赖境外云服务
2. **仅 LLM API 出公网**：仅 MiniMax/DeepSeek 调用走外网
3. **会话永久保留**：支持项目归档，不删除历史会话
4. **RBAC 三层隔离**：用户 / 角色 / 部门，org_id + dept_id 隔离
5. **执行层细粒度权限**：tool 调用前拦截（Casbin PreToolCall Hook）

## API 契约

完整 OpenAPI 规格：
- 网关 API：`api/gateway.yml`
- 核心 API：`api/core.yml`

核心端点：
- `POST /v1/chat/completions` — OpenAI 兼容对话
- `POST /auth/login` — 用户登录
- `POST /auth/register` — 用户注册
- `GET /api/sessions` — 会话列表
- `GET /health` — 健康检查

## 开发指南

### Go 网关（api-gateway）

```bash
cd api-gateway
go mod tidy
go run ./cmd/server/main.go
```

### Rust 核心（api-core）

```bash
cd api-core
cargo build
cargo run
```

### 前端（web-client）

```bash
cd web-client
npm install
npm run dev
```

## License

Internal use only.
