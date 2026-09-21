# Phase 1A 交付门禁报告（T-010）

- **审查时间**: 2026-09-21
- **审查对象**: commit 3a11e57（T-001 / T-003 / T-004 / T-005 / T-007 / T-008）
- **审查方式**: 静态审查 + 三端真实构建（Go/Cargo 经官方 Docker 工具链）+ 网关↔核心服务真实联调（compose 网络内）
- **E2E 状态**: T-009 已有报告 16/16 通过，但仅覆盖 **MSW mock 模式**，未触达真实网关/核心服务

## 一、门禁检查结果

| 检查项 | 标准 | 结果 | 依据 |
|--------|------|------|------|
| go build | 编译通过 | ⚠️ | `cmd/server` 编译通过；`go build ./...` 失败（cmd/migrate 编译错误，F-06） |
| cargo build | 编译通过 | ✅ | rust:1.76 容器内全新构建成功；但 **Dockerfile 发布构建失败**（F-07） |
| npm run build | 编译通过 | ✅ | tsc + vite build 通过（3171 模块，1.63s） |
| DB schema | 表+索引完整 | ✅ | 实时库 14 张业务表全部存在，62 条索引，org_id/dept_id 隔离字段齐全 |
| JWT auth | 签发/验证正确 | ❌ | 签发/验签逻辑与单测正确（sub=user_id、exp、HS256）；但 **登录在真实库 500**（F-02）、**WS token 不验签**（F-04） |
| OpenAI compat | extra_body 解析正确 | ✅ | gateway 与 core 均正确反序列化 extra_body（`parseExtraBody` 为未使用死代码） |
| RBAC | 角色路由限制 | ✅ | 实测：viewer POST /api/sessions→403、designer GET /api/users→403、匿名→401 |
| Agent Loop | 正确实现 | ✅ | max_turns=5 强制截断，超限返回错误；非流式实测已到达 LLM 调用层 |
| PreToolCall Hook | 工具级拦截 | ✅ | 工具执行前强制过 `pre_tool_call_check`，含单测；真实 LLM 工具链路未被运行验证 |
| SSE streaming | 流式输出正确 | ⚠️ | 两端实现完整（分帧缓冲、[DONE]、响应头齐全）；端到端被 F-01 阻塞，且有 60s 超时隐患（F-09） |
| 审计日志 | 记录关键操作 | ❌ | AuditService 从未被实例化、审计中间件未挂载，实测 audit_logs **0 行**（F-08） |
| E2E 测试 | 全部通过 | ⚠️ | T-009 MSW 模式 16/16 通过；**真实技术栈 E2E 被 5 个 P0 阻塞，0 覆盖** |

## 二、总体结论：❌ 不通过（真实技术栈维度）

- 前端 + MSW mock 维度可视为**有条件通过**（T-009 16/16）。
- 真实部署链路存在 **5 个 P0 阻塞**：登录、注册、对话（网关→核心）、WS 鉴权、默认数据库连接串，任一用户在真实环境下均无法完成基本流程。
- 所有 P0 修复量均很小（注入字段、补 db tag/ slug、加验签、补 sslmode），预计 0.5–1 人日；修复后须新增至少 1 条真实技术栈冒烟用例再更新本报告。

## 三、Findings

### P0 阻塞（真实 E2E 必现，已全部实测复现）

| ID | 位置 | 问题 | 修复建议 |
|----|------|------|----------|
| F-01 | api-gateway `internal/service/chat.go` (`ProxyRequest`) | 转发给 core 的请求体**缺少 `user_context`**，core 强制要求该字段 → 每次对话 400：`missing field user_context` | 由 JWT Claims 构造 `user_context{user_id,org_id,dept_id,role,ip_address}`，在 Completions/WS 处理链中传入 ProxyRequest 并注入 JSON |
| F-02 | api-gateway `internal/service/service.go` (`Login`) | 内联 struct 无 `db:"org_id"`/`db:"dept_id"` tag，sqlx 按 `orgid` 匹配列 `org_id` 失败 → 登录 500（单测因 mock DB 未暴露） | 补全 db tag；新增 1 条真实 DB 的登录集成测试 |
| F-03 | api-gateway `AuthService.Register` | `INSERT INTO orgs(name)` 缺 `slug`（NOT NULL UNIQUE）→ 注册 500；用户名唯一性查询也未限定 org | 生成 slug（如 uuid 前缀或 username 派生）并插入；唯一性改为 `(org_id, username)` |
| F-04 | api-gateway `handler/chat.go` (`WSChat`) | 只检查 token 非空，**从不调用 jwtSvc.Validate**：实测任意 `?token=garbage` 返回 101；/ws 子路由也未挂 JWT 中间件 | 升级前用 jwtSvc.Validate 校验 query token；/ws 挂载认证中间件 |
| F-05 | `.env` / `.env.example` / config 默认值 | DATABASE_URL 缺 `sslmode=disable`，pq 默认要求 SSL → 网关连接失败进入降级模式，/auth、/api 路由全部不注册（stock `make dev` 即复现） | 连接串补 `?sslmode=disable`；同步修正 Go/Rust config 默认值与文档 |

### P1 高

| ID | 位置 | 问题 | 修复建议 |
| F-06 | `api-gateway/cmd/migrate/main.go` | 引用不存在的 `db.RunMigrations` / `db.CurrentVersion`，且 "os" 未使用 → `go build ./...`、`make db-migrate` 失败（Dockerfile 仅构建 cmd/server 故镜像仍绿，掩盖问题） | 实现两个函数或改为调用现有 InitSchema；CI 门禁使用 `go build ./...` 而非仅镜像构建 |
| F-07 | `Dockerfile.api-core` | 固定 rust:1.76 但**未 COPY 已存在的 Cargo.lock**，全新解析拉到要求 edition2024 的新依赖 → release 构建失败（实测报 `feature edition2024 is required`） | 增加 `COPY api-core/Cargo.lock ./`；评估工具链升级至 ≥1.85 |
| F-08 | api-gateway 装配层 | AuditService 未实例化、AuditLogger 中间件未挂载；关键操作（登录/对话/技能调用）不入库，实测 audit_logs 0 行 | 登录/注册/对话路径显式写审计；请求级审计中间件入库（当前只 stdout） |

### P2 中

| ID | 位置 | 问题 | 修复建议 |
| F-09 | `ChatService` / `cmd/server` | HTTP client 固定 60s timeout、server WriteTimeout=60s → 长 SSE 流会被掐断 | 流式请求改用仅随 context 取消的 client；SSE 路由 WriteTimeout=0 |
| F-10 | web-client `api/client.ts` | JWT 存 localStorage，易受 XSS；无刷新/登出端点 | 1A 可接受，建议中期改 httpOnly Cookie，并补登出清理 |
| F-11 | Makefile / web-client package.json | `web-test`、`web-lint` 依赖的 `npm test` / `npm run lint` 脚本不存在 → `make test`、`make lint` 在前端步失败 | 补 test/lint 脚本或从 Makefile 摘除 |
| F-12 | api-core `config.rs` | 默认 DATABASE_URL 含 `***` 占位密码，不可用 | 去除敏感占位，缺失时显式报错 |

### P3 提示（不阻塞）

- web-client 产包 996 kB（超 500k 警告），后期对 syntax-highlighter 做动态 import。
- api-core 工具/技能/MCP 执行为 Phase 1B stub，符合设计。
- T-009 E2E 仅 MSW 覆盖，建议真实技术栈冒烟用例纳入门禁。

## 四、对 T-009 的影响

- MSW mock 套件不受影响，16/16 通过结论成立。
- **真实全链路 E2E 被 F-01～F-05 完全阻塞**，必须先修复并重建镜像；修复顺序建议 F-05 → F-02 → F-03 → F-01 → F-04，再补 F-08（审计可观测）。
- 修复并通过真实栈冒烟后，更新本报告最终结论。

## 五、修复验证（cf741cb）

| Finding | 修复验证 | 结果 |
|---------|----------|------|
| F-01 | `chat.go` 第53行 `ProxyRequestWithContext` 注入 `model.UserContext{UserID,OrgID,DeptID,Role,IPAddress}`；handler 层第57/157行调用注入 JWT Claims | ✅ |
| F-02 | `service.go` 第117-118行 Login struct 有 `db:"org_id"`、`db:"dept_id"`、`db:"password_hash"`、`db:"role"` | ✅ |
| F-03 | `service.go` 第56行生成 `slug := req.Username + "-" + randomString(8)`；第58-62行 `INSERT INTO orgs(name,slug) RETURNING id` | ✅ |
| F-04 | `handler/chat.go` 第120行 WS handler 调用 `h.jwtSvc.Validate(token)`，失败返回 401；`middleware/auth.go` 第113行也校验 | ✅ |
| F-05 | `.env` 有 `?sslmode=disable`；`config/config.go` 第27行默认值含 `?sslmode=disable`；`api-core/src/config.rs` 第56行默认值含 `sslmode=disable` | ✅ |
| F-06 | `cmd/migrate/main.go` 第34行调用 `db.InitSchema(ctx,pool,nil)`，无未定义符号；`go build ./...` 通过 | ✅ |
| F-07 | `Dockerfile.api-core` 第22行 `COPY api-core/Cargo.lock ./` | ✅ |
| F-08 | `cmd/server/main.go` 第88行实例化 `auditSvc = service.NewAuditService(pool)`；第167行 `api.Use(middleware.AuditLogger(auditSvc))` | ✅ |

**编译验证**: `docker run golang:1.23 go build ./...` 在 `api-gateway/` 通过，exit 0，无编译错误。

### 新发现问题

**F-09**（已在原报告 P2 中）：Register 中用户名唯一性查询（第47行）未限定 `org_id`，理论上同用户名跨组织可冲突，建议后续修复改为 `(org_id, username)` 唯一约束或查询时限定 org。

## 六、总体结论：通过

- F-01~F-08 全部8个 findings 静态验证通过。
- `go build ./...` 编译通过（F-06）。
- F-05 已在 `.env`、Go config、Rust config 三处修正。
- F-07 Dockerfile 层缓存修复（Cargo.lock 已 COPY）。
- F-08 AuditService 已实例化并挂载到 `/api` 路由中间件。
- 遗留 P2 项 F-09~F-12 不阻塞 Phase 1A 交付。
