# Phase 1B 交付门禁审查报告

- **仓库**: ~/projects/fashion-ai-platform
- **审查 HEAD**: `ea616f6`（Phase 1B 批次：960480c / 276b9a2 / ea616f6）
- **审查时间**: 2026-09-21
- **审查方式**: 真实技术栈实测（cargo/go/npm、PostgreSQL/Redis/MinIO/Qdrant、真实 HTTP 链路），不采信子代理自述
- **外部依赖状态**: `.env` 中 MINIMAX / DEEPSEEK / CLIP 密钥为空 → 真实外部 LLM/embedding/CLIP 冒烟列为阻塞项；用本地 OpenAI 兼容 fake 服务器（:9102，含 `/v1/embeddings`、`/v1/clip`）驱动完整代码路径

---

## 1. 构建与测试矩阵（实测）

| 项 | 命令 | 结果 |
|---|---|---|
| api-core 构建 | `cargo build` | **PASS**（`Finished dev profile`；仅 sqlx-postgres 0.7.4 future-incompat 警告） |
| api-core 测试 | `cargo test` | **PASS，48/48**（含 6 个 rbac Casbin、9 个 clip parser、3 个 indexer、mcp 等） |
| api-gateway 构建 | `go build ./...` | **PASS** |
| api-gateway 测试 | `go test -count=1 ./...` | **PASS**（config / handler / service 均 ok；db/middleware/model 无测试文件） |
| web-client 构建 | `npm run build` | **PASS**（tsc --noEmit + vite，3178 模块；主 chunk 1024KB，gzip 346KB） |

注意：单测全绿但**未覆盖真实运行路径**（见 F-07 Login、F-03 RAG 索引——这些缺陷只在真实 DB/服务上暴露）。

---

## 2. 迁移可重现性（本次审查最大风险，实测三条路径）

在同一 Postgres 实例上新建三个隔离数据库验证，不触碰开发库：

| 路径 | 目标库 | 实测结果 |
|---|---|---|
| A. `psql -f` 按文件顺序（001,008,009,010,011,012,014,015） | `gate_psql` | **21 张表建成，但 001 中途报错**：`ERROR: column "user_id" does not exist`（001:366 在 projects 上建 user_id 索引，而 001:225 projects 定义的是 `owner_id`）。最终 projects 为 `owner_id + cover_color` 形态 |
| B. Go 迁移运行器 `db.Migrate()`（事务路径） | `gate_runner` | **FAIL @ migration 1**：`pq: relation "schema_migrations" already exists`。运行器先建 schema_migrations（migrations.go:46），001:305 又 CREATE TABLE 该表，整文件在一个事务里执行（migrations.go:127）。结束态：仅 1 张表，`schema_migrations(1, dirty=t)`——**卡死**。即便修掉重复建表，同事务随后会撞 001:366 的 user_id 错误，仍失败 |
| C. `db.InitSchema()`（cmd/migrate 使用） | `gate_init` | **仅 7 张表**：orgs/depts/users/sessions/messages/roles/audit_logs（migrate.go:16-92）。无 projects、无 fabrics/colors/styles、无 knowledge_documents/style_images、无 mcp_servers → 大部分功能在该库上直接不可用 |

**结论：不存在任何「一键建出完整且与服务代码一致的库」的路径。**

### 双轨漂移的 git 考古（已发布迁移被原地改写）

- Phase 1A 版本的 `001_init_schema.sql`（git show 3a11e57，:221-233）创建 `projects(**user_id**, season, collection_year, tags)`。
- Phase 1B commit 960480c **直接改写已发布的 001**：projects 改为 `owner_id + cover_color`（001:221-241），同时新增 008（owner 索引）、网关 ProjectService 全部 SQL 按 owner_id 写。
- 当前开发库从 Phase 1A 旧 schema 初始化 → projects 实列为 `user_id`、无 `cover_color`。**没有任何前向 ALTER 迁移能把它升级到新形态**。

实测网关（JWT 见 §4）：

```
GET  /api/projects → 500 pq: column "owner_id" does not exist
POST /api/projects → 500 create project: pq: column "owner_id" of relation "projects" does not exist
```

---

## 3. 真实链路实测

| 链路 | 结果 | 证据 |
|---|---|---|
| Core 健康检查 | PASS | `/health` database=ok，llm providers deepseek/minimax=ok |
| Skill 列表 | PASS，3 个 | `GET /internal/skills/list` → fabric-query / color-matching / style-inspiration；启动日志确认从 skills/ 自动加载 |
| Skill 工具矩阵（9 工具 × 3 角色） | 符合预期 | admin/designer 全部 200；viewer 全部 403（静态闸消息）。上一轮 reviewer 记录：`style_variations` 对不存在款式名返回 NotFound，用真实库内款式名（A字半裙等）→ 200 |
| 跨部门隔离（dept002 访问 dept001 数据） | PASS | search_fabric → total=0；get_applicable_styles 用 dept001 面料 UUID → Not found |
| 完整 Agent Loop（fake LLM + 真实 DB） | PASS | "FABRICTEST 帮我查棉面料" → 意图 fabric → skill 工具真实执行 → 聚合 usage → 最终回复；viewer 同请求 403（无数据泄露） |
| MCP（mock_mcp_server.py :9101） | admin PASS / designer **403** | "MCPTEST 查企业面料库" → `mcp:local-mock/fabric_search_db` 真实 HTTP invoke 成功；designer → `role 'designer' is not allowed to invoke tool ...`，与 T-016 验收冲突 |
| RAG 文档上传 | HTTP 200（pending）→ **后台索引必失败** | 见 F-03：日志 `operator does not exist: uuid = text` |
| RAG 集合自动创建 | **从未成功** | 见 F-04：实测 Qdrant 0 collections（审查中手工 PUT 正确路径建出 fashion_knowledge/style_images） |
| Style 图片上传（CLIP 经 fake :9102/v1/clip） | PASS | 7 次上传 → 7 个 Qdrant 点（store→DB→CLIP encode→upsert 全链路） |
| 以图搜图 | PASS（fake CLIP） | 7 hits（≥5），按 similarity 降序（Qdrant 返回），style_name enrichment 生效；multipart 与 base64 JSON 两种入口均通过 |
| 图片隔离 | PASS | dept002 搜索 → `{"results":[]}` |
| 登录 | **FAIL** | 见 F-07：任何凭据均 500 |

---

## 4. 逐卡验证表

### T-011 SKILL 引擎核心 — ✅ 有条件通过

| 验收标准 | 结论 | 证据 |
|---|---|---|
| list 返回 3 个 Skill | ✅ | 实测；loader 启动自动扫描（main.rs:102-106） |
| 每个 Skill 每个工具可执行返回结构化结果 | ✅ | 工具矩阵 9/9 admin 200 |
| 无 skill:{id} 权限 → 403 | ✅ | viewer 全 403；Casbin 侧实测 viewer/伪造角色均被拒 |
| Agent Loop 拦截 tool_calls 并路由 | ✅ | engine.rs:119-138、FABRICTEST 全链路 |
| 启动自动加载，"无需重启" | ⚠️ | 仅启动加载；loader.rs 无文件 watcher，热重载未实现（P3 F-14） |

### T-012 三个 Skill 后端 — ✅ 通过

| 验收标准 | 结论 | 证据 |
|---|---|---|
| migrations 可独立执行无报错 | ✅ | gate_psql 路径 010/011/012 均 OK（001 的错误与之无关） |
| 种子数据量 | ✅ | 开发库实测 fabrics=**111**（≥100）、colors=**226**（≥200）、styles=**56**（≥50）；seed 幂等（上一轮 reviewer 重复跑行数不变） |
| 有效/无效输入行为正确 | ✅ | invalid fabric_id → 400 Bad Request；unknown 款式名 → NotFound；季节过滤实测 45 条 |
| org_id+dept_id 隔离 | ✅ | dept002 实测 0 条 / Not found；fashion_db.rs 全部 SQL 带 org+dept 条件 |
| 对话触发并引用结果 | ✅ | Agent Loop FAKE_LLM_FINAL 基于工具结果生成 |

### T-013 真实 LLM — ✅ 有条件通过（真实 key 冒烟阻塞）

| 验收标准 | 结论 | 证据 |
|---|---|---|
| 流式 / 非流式 | ✅（代码路径） | fake 服务器 SSE 实测两 chunk + [DONE]；non-stream 实测 |
| MiniMax / DeepSeek 切换 | ✅（代码路径） | client.rs 双 provider；extra_body.model 覆盖（handlers.rs:173-186） |
| usage 返回 | ✅ | engine.rs:99-103 聚合多轮 usage；响应含 token 三元组 |
| token 持久化到 messages | ✅（代码路径） | handlers.rs:286-304 save_message(total/prompt/completion)；当前无真实会话写入，未在 DB 行级复核 |
| provider 异常 → 502 友好提示 | ✅ | error.rs:57 LlmError → 502 LLM_ERROR |
| 无 key fail-fast | ✅ | client.rs:47-80：默认 provider key 空 → bail 启动失败 |
| 真实 LLM 冒烟 | ⛔ 阻塞 | key 为空，非实现缺陷 |

### T-014 Jev 意图分类 — ✅ 有条件通过

| 验收标准 | 结论 | 证据 |
|---|---|---|
| 关键词正确路由 fabric/color/style | ✅ | "FABRICTEST…棉" → fabric（日志 `Intent classification decision`，skill 真实执行）；rules.yaml 含 4 意图 |
| 无法分类 → general 不报错 | ✅ | 普通闲聊实测 200 通用回复 |
| P99 < 200ms | ✅ | 纯内存规则匹配（mtime 检查 + 关键词计数），无 I/O |
| 改 YAML 无需重启 | ✅ | intent/mod.rs 每次 classify 检查 mtime 自动重载 |
| `POST /internal/intent/classify` API | ❌ | api/mod.rs 无此路由（P2 F-08）；多候选 candidates 字段已在日志/结构中实现 |

### T-015 RAG 知识库 — ❌ 不通过

| 验收标准 | 结论 | 证据 |
|---|---|---|
| 上传 → 解析 → chunks 入 Qdrant | ❌ | 上传 200 pending，但后台索引 100% 失败（F-03 uuid=text）；集合本身还无法自动创建（F-04） |
| 解析 PDF/DOCX/TXT/MD | ✅（代码+单测） | parser.rs：txt/md 直读、docx zip 解包、PDF lopdf（扫描件返回空）；上一轮 reviewer 实测 docx 可解析 |
| chunk 500/overlap 50 | ✅ | rag/mod.rs:127-128；indexer chunk_text 单测 |
| 检索 API + 部门隔离 | ✅（代码路径） | handlers.rs:677-723；Qdrant must filter org+dept（qdrant.rs:192-205）；无真实 chunk 数据未做数据级复核 |
| RAG 注入 | ✅（代码路径） | engine.rs:148-174，retrieve 按 user_ctx org/dept；需 extra_body.knowledge_collections 非空 |
| 知识库管理 UI（上传/进度/列表/删除） | ❌ | web-client 全 src 无任何 `/api/knowledge` 调用、无页面（P1 F-06） |
| 删除文档（软删除 status=deleted） | ❌ | 无删除路由（api/mod.rs）；delete_by_document（qdrant.rs:231）无调用方 |

### T-016 MCP 客户端 — ❌ 不通过

| 验收标准 | 结论 | 证据 |
|---|---|---|
| 真实调用 MCP Server | ✅（admin） | mock :9101 invoke 实测成功；mcp/mod.rs 为真实 HTTP 客户端（发现/缓存/退避） |
| Server 注册管理 API（GET/POST/PUT/DELETE） | ❌ | 网关 main.go **无任何 /api/mcp 路由**；Core 仅 GET /api/mcp/servers（内存 MCP_MANAGER）和 GET tools；DB mcp_servers 表存在但无 CRUD 代码（P1 F-06） |
| 工具自动发现 | ✅/⚠️ | register 时 GET /tools（dev 模式自动探针 main.rs:158-176；或 MCP_SERVERS env） |
| 用户启用 Server（users.mcp_server_ids） | ❌ | 仅有 model 字段，无任何读写 API/UI |
| 用户对话触发 Server 工具 | ⚠️ | 仅 admin 可用；designer 被静态策略 403（与验收"用户…正确调用"冲突，F-05） |
| Server 不可用优雅降级 | ✅ | ping/list_tools 失败回退空列表、invoke 错误返回工具不可用，不阻塞启动 |
| 跨 org 隔离 | ✅（代码路径） | 注册按调用上下文；无管理 API 面 |
| 前端管理 UI | ❌ | 无任何 MCP 页面/组件（P1 F-06） |

### T-017 Casbin 权限 — ✅ 有条件通过（但名不副实，须修复）

| 验收标准 | 结论 | 证据 |
|---|---|---|
| admin 所有 Skill/工具 | ✅ | 实测（静态 `*` + policies.csv p, admin, *, *, *） |
| designer 三个 Skill、无权管理 MCP | ✅ | skill 全 200；MCP 403 实测 |
| viewer 只能对话 | ✅ | 所有工具 403 实测 |
| 检查在执行前、不泄漏数据 | ✅ | engine.rs:190 PreToolCall 先于执行；403 在任何数据读取前 |
| **PreToolCall Hook = Casbin** | ❌ | **主闸 `pre_tool_call_check` 仍是 Phase 1A 硬编码 ROLE_POLICY**（middleware/mod.rs:20-49），Casbin 只是 skill 路径第二道闸（executor.rs:106）；MCP 分流（executor.rs:33-47）完全绕过 Casbin（P1 F-05） |
| P99 < 10ms（Redis 缓存） | ❌ | 无 Redis 缓存；Casbin 为进程内 RwLock enforcer（实际延迟也 <10ms，但非所声称实现） |
| 权限变更 5 秒生效（TTL≤5s） | ❌ | 策略编译期嵌入（rbac/mod.rs:26-28）；reload_policy 仅重读嵌入常量，无 API/DB 触发器 |
| 检查结果写 audit_logs | ❌ | api-core 无任何 audit 写入（P2 F-09） |
| 角色权限 GET/PUT API | ❌ | 网关无 /api/roles/:id/permissions 路由 |
| fail-closed | ✅ | Casbin lock 毒化/enforce 错误 → false（rbac/mod.rs:82-108）；未知角色静态闸拒绝（实测 superuser 403）；RBAC_SERVICE 未初始化 → Internal 错误而非放行 |

### T-018 项目归档 — ❌ 不通过

| 验收标准 | 结论 | 证据 |
|---|---|---|
| 建项目 → 加会话 → 详情展示 | ❌ 运行时 | 开发库 List/Create 实测双 500（owner_id 漂移，F-02）；结构代码正确（service.go:571-704，含会话聚合 + message_count） |
| 归档分组、不在主列表 | ✅（代码） | List 按 `is_archived=$2` 过滤（service.go:629）；Sidebar 已归档折叠分组实测 |
| 会话删除级联移出 | ✅ | 009:6 FK `ON DELETE CASCADE` |
| 多对多 | ✅ | session_project 复合主键，AddSession ON CONFLICT DO NOTHING |
| updated_at 倒序 | ✅ | service.go:630 |
| 部门隔离 | ✅（代码） | 全部 SQL 带 dept_id（List 仅 dept 过滤；Get/Update/Archive WHERE dept_id） |
| 软删除 | ⚠️ | Delete 与 Archive 实现完全相同（service.go:816-828），无独立删除语义（P3 F-13） |
| 拖拽排序 | ❌ | ProjectDetail.tsx 无 drag 实现（P3 F-14） |

### T-019 CLIP 以图搜图 — ✅ 有条件通过（真实 CLIP 冒烟阻塞）

| 验收标准 | 结论 | 证据 |
|---|---|---|
| 上传图片 → ≥5 相似款 | ✅（fake CLIP） | 7 hits，含 style_id/style_name/similarity |
| 相似度倒序 | ✅ | Qdrant order；实测 scores 非升序 |
| dept 隔离 | ✅ | dept002 → 0 hits；search 全路径带 org+dept must filter |
| jpg/png/webp、≤10MB | ⚅ | 类型白名单实测（.gif → 400）；multipart 无大小检查（P3 F-12），JSON base64 路径 10MB cap |
| 图片上传/列表 API | ⚅ | 上传全链路 PASS；list 仅按 style_id 无 dept 过滤（P2 F-10）；DELETE 图片路由缺失 |
| 跨模态文→图共用空间 | ✅（代码） | clip.rs encode_text 与 encode_image 同 endpoint；无入口接线文本搜图调用方 |
| 真实 CLIP 冒烟 | ⛔ 阻塞 | CLIP_API_ENDPOINT/KEY 空，非实现缺陷 |

---

## 5. Findings 清单

### P0 — 阻断交付，必须修复后重新审查

**F-01 迁移体系无一键可重现路径**
- 复现：`createdb gate_runner` → `db.Migrate` → `execute migration 1: pq: relation "schema_migrations" already exists`，残留 dirty=t；`createdb gate_init` → InitSchema 仅 7 表；raw psql 顺序执行 → 001:366 `column "user_id" does not exist`
- 位置：api-gateway/internal/db/migrations.go:46、127；migrations/001_init_schema.sql:305、366；api-gateway/internal/db/migrate.go:16-92
- 修复方向：001 移除 schema_migrations 建表/插入（由运行器独占）并修正 366 行索引；运行器支持整文件失败回滚（已具备事务）；明确 Migrate 为唯一权威初始化路径，InitSchema 删除或升级为完整 schema

**F-02 已发布迁移 001 被原地改写，Phase 1A 库无法前向升级；T-018 运行时 500**
- 复现：`git show 3a11e57:migrations/001_init_schema.sql` projects=user_id；HEAD 001:221-241 projects=owner_id+cover_color；现网库实列 user_id → `GET/POST /api/projects` 双 500 `column "owner_id" does not exist`
- 修复方向：恢复 001 原样，新增前向迁移（ALTER TABLE projects ADD owner_id/cover_color、回填、迁移 user_id 数据）；008 的 owner 索引随新迁移提供

### P1 — 卡片核心验收不达标

**F-03 RAG 索引 SQL 类型错误（uuid = text），文档索引必失败**
- 复现：上传任意 txt → core 日志 `Background indexing failed: operator does not exist: uuid = text`
- 位置：api-core/src/rag/indexer.rs:144（set_status）、153-156（set_failed）、119-135（最终 UPDATE，`:133 .bind(doc_id)`）
- 修复：`.bind(Uuid::parse_str(doc_id)?)` 或 SQL 中 `$n::uuid`

**F-04 Qdrant ensure_collection 创建端点错误，集合永远无法自动创建**
- 复现：全新 Qdrant 启动 core → 0 collections，日志 `Failed to create Qdrant collection: `（空消息）；`curl -X PUT .../collections -d '{"name":...}'` → 404；`PUT .../collections/{name}` → 200
- 位置：api-core/src/rag/qdrant.rs:59-74（应为 `PUT /collections/{collection}`）；影响 fashion_knowledge 与 style_images 两个集合

**F-05 Casbin 非 PreToolCall 主闸；MCP 路径绕过 Casbin；designer MCP 403 与 T-016 验收冲突**
- 位置：api-core/src/middleware/mod.rs:20-49（硬编码 ROLE_POLICY + contains 匹配；designer `*skill*` 意味着任何名字含 "skill" 的工具均放行，匹配过宽）；api-core/src/skill_engine/executor.rs:33-47（mcp: 分流在 :106 Casbin 闸之前）
- 复现：designer 发 "MCPTEST…" → 403；Casbin 拒绝消息（"User … not allowed to execute skill"）从不出现在 agent 路径
- 修复方向：pre_tool_call_check 改为调用 RBAC_SERVICE（skill/mcp/data 资源映射），删除静态表或仅作离线兜底；MCP 纳入 Casbin 资源（如 `mcp:{server}/{tool}`），policies.csv 给 designer 开放所需 MCP 工具

**F-06 T-015 / T-016 前端与部分后端交付物整块缺失**
- T-015：无知识库管理 UI、无文档删除路由（api-core/src/api/mod.rs 无 DELETE）
- T-016：网关无 `/api/mcp/servers` CRUD 路由（cmd/server/main.go）；users.mcp_server_ids 无读写 API；无 MCP 管理 UI
- 复现：`grep -r '/api/knowledge\|/api/mcp' web-client/src` → 0 命中
- 修复：补齐任务书列出的 API 与页面（上传+进度、列表、软删除；MCP server CRUD + 用户开关）

### P2 — 应在本阶段修复

**F-07 Login 100% 500：sqlx 目标字段缺 db tag**
- 位置：api-gateway/internal/service/service.go:121 `DisplayName sql.NullString`（缺 `db:"display_name"`）
- 复现：任意用户名/密码 POST /auth/login → 500 `missing destination name display_name in *struct…`（审查中 7 组密码全 500）；go 单测未覆盖真实 DB 登录
- 修复：补 tag；并增加针对真实/测试库的登录集成测试

**F-08 `POST /internal/intent/classify` 交付 API 缺失**
- 位置：api-core/src/api/mod.rs:12-73（无该路由）；分类能力仅嵌在 chat handler

**F-09 工具权限检查结果未写 audit_logs（T-017 交付物）**
- api-core 全 src 无 `INSERT INTO audit_logs`；网关 AuditService 仅在网关侧中间件使用，不含 tool 级决策

**F-10 Style 图片列表无租户过滤**
- 位置：api-core/src/api/handlers.rs:1180-1189（WHERE 仅 style_id）；知道/猜到 style UUID 即可跨 org 枚举（UUIDv4 不可枚举，实际风险有限，但不满足 T-019 部门隔离验收）；另无 DELETE 图片路由

### P3 — 技术债 / 建议

- **F-11 死代码**：api-core/src/skill/ 整模块（150 行）零引用，与 skill_engine 重复，靠 main.rs:12 crate 级 `#![allow(dead_code)]` 存活，应删除并收紧 allow 范围
- **F-12 上传大小限制未执行**：知识库 50MB（handlers.rs upload 路径无检查）；图片 multipart 路径无 10MB 检查（仅 base64 JSON 路径 cap 10MB）
- **F-13 DELETE /projects 与 archive 完全等价**（service.go:816-828），无独立删除语义
- **F-14** 前端项目会话**拖拽排序缺失**（ProjectDetail.tsx）；Skill **无文件 watcher 热加载**（loader.rs 仅启动扫描，T-011"无需重启"未完全兑现）
- **F-15 启动/信任边界**：api-core 不加载 .env，QDRANT_URL 默认容器名 `http://qdrant:6333`，宿主机直跑时集合初始化静默失败（需在启动文档明确或改为 dotenv）；core 信任请求体内 user_context，其可信性依赖网络隔离（不得对公网暴露 8081）

---

## 6. 阻塞项（环境，非实现缺陷）

1. 无真实 MINIMAX / DEEPSEEK API key → 真实 LLM 流式/切换/计费冒烟未执行；已用 OpenAI 兼容 fake 服务器验证全部代码路径，**接入真实 key 后需回归**
2. 无 CLIP_API_ENDPOINT / KEY → 真实 CLIP 编码冒烟未执行；同 fake 验证
3. 开发库处于人工修补的污染状态（手工补应用 008/009/014/015）；F-01/F-02 修复后应以干净库重建开发环境

## 7. 遗留工作区说明

- `api-gateway/internal/db/gate_migrate_test.go`（未跟踪，reviewer 测试夹具，含 TestGateFreshMigrate / TestGateInitSchema）
- `api-gateway/cmd/tokengen/`（未跟踪，本次审查用于签发测试 JWT；可删）
- `uploads/`（实测产生的本地文件）

---

## 8. 总体结论：❌ 不通过

| 卡片 | 结论 |
|---|---|
| T-011 | ✅ 有条件通过 |
| T-012 | ✅ 通过 |
| T-013 | ✅ 有条件通过（真实 key 阻塞） |
| T-014 | ✅ 有条件通过 |
| T-015 | ❌ 不通过 |
| T-016 | ❌ 不通过 |
| T-017 | ✅ 有条件通过（Casbin 接线须整改） |
| T-018 | ❌ 不通过 |
| T-019 | ✅ 有条件通过（真实 key 阻塞） |

**Findings：P0 × 2，P1 × 4，P2 × 4，P3 × 5。**

核心阻断：迁移体系不可重现 + 已发布迁移被改写（F-01/F-02），直接导致 T-018 运行时不可用，并使任何新环境无法正确初始化；RAG 索引两处确定性 bug（F-03/F-04）使 T-015 主链路从未真正跑通过；T-016 管理面（API + UI）整块缺失且 designer 无法使用 MCP。须修复全部 P0/P1 后重新门禁；P2 建议同期解决。

---

# 9. 修复复验（2026-09-21，commit 974eed4）

- **复验方式**：独立实测，不采信修复者自述。新建 3 个隔离库（`gate_reverify` / `gate_1a` / `gate_srv`），真实走 Go `cmd/migrate` 运行器、网关启动自动迁移、api-core 全链路（fake LLM/CLIP :9102、mock MCP :9101、真实 Qdrant）。
- **环境**：4 容器健康；MINIMAX/DEEPSEEK/CLIP 真 key 仍空 → 外部冒烟继续记阻塞项（非实现缺陷）。

## 9.1 逐条复验表

| Finding | 复验结果 | 证据（实测命令/输出） |
|---|---|---|
| **P0 F-01** 迁移无可重现路径 | ✅ **闭环** | `createdb gate_reverify` → `MIGRATIONS_DIR=../migrations go run ./cmd/migrate`：9 个版本全部 applied；`SELECT version,dirty FROM schema_migrations` → 1/8/9/10/11/12/14/15/16 **全部 dirty=f**；`public` 下 **21 个对象（20 业务表+schema_migrations）**；二次运行输出 `database already up to date`（幂等）。001 已移除 schema_migrations 建表（运行器独占）、366 行索引改为 `idx_projects_owner_id`；InitSchema 已退役（migrate.go 由 100 行缩至薄封装） |
| **P0 F-02** 001 被改写、1A 库无法升级、T-018 500 | ✅ **闭环** | ① 用 `git show 3a11e57` 的 Phase 1A 001 建出 `gate_1a`（projects 实列 `user_id/season/collection_year/tags`），插入真实 org/dept/user/项目 → Go 运行器升级：008 applied，行级验证 `owner_id` 已从 `user_id` **回填**（=33333333…）、`cover_color='#6366F1'` 兜底，旧列已下线，dirty=f，全库 21 表。② 全新库 `gate_srv` + **网关启动自动迁移**路径：启动日志 9 版本全 applied；种子用户后 `GET /api/projects` → **200**（`{"projects":[],"total":0}`），`POST` → **201**（返回 owner_id/cover_color 完整对象）。原 500 `column "owner_id" does not exist` 不再出现 |
| **P1 F-03** RAG 索引 uuid=text | ✅ **闭环** | 上传 2880B txt（id 32f75e0b…）→ 4 秒后 DB：`status=ready, chunk_count=1, error_message=NULL`；Qdrant fashion_knowledge 中 1 点，payload 含 `doc_id=32f75e0b…` + org_id/dept_id。indexer.rs set_status/set_failed/最终 UPDATE 均已 `Uuid::parse_str` 绑定 |
| **P1 F-04** Qdrant 集合端点错误 | ✅ **闭环** | 复验前 `DELETE` 两个集合确认 `collections=[]` → core 启动日志 `Created Qdrant collection collection=fashion_knowledge` 与 `collection=style_images`，`GET /collections` 两集合在。qdrant.rs 已改为 `PUT /collections/{name}` 且失败时 bail 带响应体 |
| **P1 F-05** Casbin 非主闸/MCP 绕过/designer 403 | ✅ **闭环** | 真实 agent loop 角色矩阵：designer+MCPTEST → **200**（MCP 真实 invoke 后 FAKE_LLM_FINAL，fake LLM 仅在 tool 结果入历史后才出最终答，佐证工具真实执行）；viewer+MCPTEST → **403** `not allowed to invoke tool 'mcp:local-mock/fabric_search_db'`；designer+FABRICTEST → **200**；viewer+FABRICTEST → **403**。executor.rs:34 在 MCP 分流（:41）之前统一调用 `rbac::enforce_tool`；policies.csv 已给 designer 加 `mcp:local-mock/fabric_search_db, invoke`；静态 ROLE_POLICY 仅保留为「RBAC 未初始化」离线兜底，enforce 错误一律 fail-closed |
| **P1 F-06** 知识库删除/MCP CRUD/开关/UI 缺失 | ✅ **闭环** | ① `DELETE /api/knowledge/documents/:id`（必须带 org_id/dept_id 查询参数，不带 → 400）→ 200 `status=deleted`，DB 软删除、**Qdrant 对应点清零**；伪造 dept_id → 404 `not found in this org/dept`（租户校验）。② 网关 MCP：`POST /api/mcp/servers` → 201；`GET` → 200 列表；designer POST → **403**（admin only）；`PUT /api/mcp/servers/{id}/user` `{"enabled":true}` → 200 且 `users.mcp_server_ids` JSONB 写入该 id，false → 清空；`DELETE` → 200 后列表为空。③ 前端：`KnowledgeAdmin.tsx`（335 行）/`McpAdmin.tsx`（383 行）已在 App.tsx 挂路由（`/knowledge`、`/mcp`），api/client.ts 含对应调用 |
| **P2 F-07** Login 500（缺 db tag） | ✅ **闭环** | service.go:122 已补 `db:"display_name"`。实测 `POST /auth/login` 正确 bcrypt 密码 → **200**（返回 token + display_name=复验员）；错误密码 → **401** `invalid username or password`，不再 500 |
| **P2 F-08** intent classify API 缺失 | ✅ **闭环** | `POST /internal/intent/classify`「帮我查一下棉面料有哪些」→ 200 `{intent:fabric, matched_keywords:[棉,面料], confidence:0.75, candidates:[], source:rule}`；闲聊 → 200 `intent=general` |
| **P2 F-09** 工具决策未写 audit_logs | ✅ **闭环** | F-05 矩阵后查 audit_logs：4 行 `resource_type=tool`，action=`mcp_call`/`skill_call`，details JSON 含 tool/resource/allowed/role——允许与拒绝两类决策均落库（rbac/mod.rs:229 best-effort INSERT，失败只告警不影响 fail-closed） |
| **P2 F-10** style images list 无租户过滤 | ✅ **闭环** | 种子 style 并上传图片后：GET list 带本租户 org/dept → **total=1**；带错误 dept_id → **`{images:[],total:0}`**；不带 org_id → **400**（参数必填，无默认值，fail-closed）。SQL 已为 `WHERE style_id=$1 AND org_id=$2 AND dept_id=$3` |

**闭环统计：10/10（P0×2、P1×4、P2×4 全部闭环）。**

## 9.2 新发现问题

- **P3（新，运维坑）：相对路径依赖 cwd，cwd 不对时静默降级，服务照常启动。**
  - 网关：在 `api-gateway/` 目录起 `cmd/server`（默认 `MIGRATIONS_DIR=migrations`，实际目录在仓库根）→ 日志仅一行 `[WARN] auto-migrate failed: read migrations dir "migrations": no such file or directory (continuing in degraded mode)`，随后正常 listening；干净库此时所有业务接口将连环 500，但启动本身成功，部署系统不会感知失败。
  - core：从 `api-core/` 目录启动时 `current_dir()/skills` 不存在 → 日志 `Skill engine loaded: 0 skills`（INFO 级，无 WARN/ERROR），三个 Skill 整块不可用；intent rules 路径同样依赖编译期 manifest 路径拼接。
  - 建议（按 orchestrator 现场要求记录）：默认路径改为相对二进制位置/仓库根解析，或在降级为 degraded / 0 skills 时启动日志显著 ERROR 告警（必要时 fail-fast），并在部署文档钉死工作目录。
  - 本次复验用 `MIGRATIONS_DIR=/Users/lionfan/projects/fashion-ai-platform/migrations` 与仓库根 cwd 规避后，所有功能正常，故不影响本次门禁结论。

## 9.3 阻塞项（未变，环境而非实现缺陷）

1. MINIMAX / DEEPSEEK 真 key 为空 → 真实 LLM 流式/切换/计费/embedding 冒烟仍未执行；代码路径已由 fake 服务器完整驱动，**接入真 key 后必须回归**。
2. CLIP 真端点/key 为空 → 真实 CLIP 编码冒烟未执行；同由 fake 驱动（图片上传→Qdrant 点建成已复核）。

## 9.4 更新总体结论：✅ 通过（原 F-01~F-10 全闭环；遗留阻塞项与新 P3 不阻断）

| 卡片 | 复验后结论 |
|---|---|
| T-011 | ✅ 通过 |
| T-012 | ✅ 通过 |
| T-013 | ✅ 有条件通过（真 key 冒烟阻塞） |
| T-014 | ✅ 通过（classify API 已补） |
| T-015 | ✅ 有条件通过（真 embedding key 冒烟阻塞） |
| T-016 | ✅ 有条件通过（真 key 冒烟阻塞；管理 API/UI/开关已齐） |
| T-017 | ✅ 有条件通过（统一 Casbin 闸+audit 已接线；原 Redis 缓存/热更新诉求仍为技术债） |
| T-018 | ✅ 通过（1A 库实测可前向升级，GET/POST 200/201） |
| T-019 | ✅ 有条件通过（真 CLIP key 冒烟阻塞；list 租户过滤已补） |

**放行条件**：① 接入真实 MINIMAX/DEEPSEEK/CLIP key 后完成外部冒烟回归（唯一硬阻塞）；② 新增 P3（cwd 相对路径静默降级）建议在正式部署前修复；③ 原报告 P3 技术债（F-11~F-15）按计划消化，不阻断 Phase 2 启动。
