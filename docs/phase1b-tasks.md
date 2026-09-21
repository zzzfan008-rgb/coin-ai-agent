# Phase 1B Kanban Cards — Fashion AI Platform

> Phase 1B：「核心价值交付」—— 将 Phase 1A 的骨架变成真正可用的产品。全部 8 个核心功能纳入 Phase 1B。

## Board
- **Slug**: fashion-ai-platform
- **Repo**: ~/projects/fashion-ai-platform
- **Phase**: 1B（Week 5-16）
- **目标工期**: 8~12 周

---

## 功能全景

| # | 功能 | 说明 |
|---|------|------|
| F1 | SKILL 引擎落地 | 加载 SKILL.md → 注册工具 → Agent Loop 调用 |
| F2 | 3 个 Skill 后端实现 | fabric-query / color-matching / style-inspiration 真正可用 |
| F3 | 真实 LLM 接入 | MiniMax / DeepSeek 替换 mock |
| F4 | Jiev 意图分类层 | 用户消息 → 路由到合适 Skill |
| F5 | RAG 知识库 | 管理员上传 → 向量检索 → AI 引用 |
| F6 | MCP 客户端 | 连接企业 MCP 服务器，扩展工具能力 |
| F7 | 权限细化 | Casbin PreToolCall Hook 执行层真实验证 |
| F8 | CLIP 以图搜图 | 上传图片找相似款式 |
| F9 | 项目归档 | 设计师按项目组织会话 |

---

## Card Definitions

### T-011: SKILL 引擎核心实现

```
标题: [T-011] SKILL 引擎核心实现
负责人: backend
依赖: T-005 (Rust 核心服务), T-008 (SKILL.md 定义)
前置条件:
  - api-core 已可运行
  - skills/ 目录下 3 个 SKILL.md 已就绪
交付物:
  - SKILL 引擎：Rust 模块，运行时加载 skills/ 目录下的所有 SKILL.md
  - 工具注册表：解析 SKILL.md frontmatter，动态注册 tools 数组中的所有工具
  - Skill 执行器：接收 skill_id + tool_name + parameters，执行对应工具逻辑，返回 JSON 结果
  - 权限注册：解析 permissions 数组，挂载到 Skill 元数据
  - Skill 加载 API：GET /internal/skills/list — 返回所有 Skill 的 id/name/version/description/tools
  - Skill 执行 API：POST /internal/skills/execute — 接收 skill_id/tool_name/parameters/user_context，返回执行结果
  - Skill 元信息解析：从 YAML frontmatter 提取 version/permissions/tools/prompt_template
验收标准:
  - GET /internal/skills/list 返回 3 个 Skill（fabric-query, color-matching, style-inspiration）
  - POST /internal/skills/execute 对每个 Skill 的每个工具都能执行并返回结构化结果
  - Skill 权限检查生效（无 skill:{id} 权限的用户调用返回 403）
  - Agent Loop 中 LLM 输出 tool_calls 时，引擎能拦截并路由到正确的工具
  - 引擎启动时从 skills/ 目录自动加载，无需重启服务
```

---

### T-012: 3 个 Skill 后端实现（fabric-query / color-matching / style-inspiration）

```
标题: [T-012] 3 个 Skill 后端实现
负责人: backend
依赖: T-011 (SKILL 引擎), T-015 (真实 LLM)
前置条件:
  - SKILL 引擎的工具注册接口已就绪
  - 面料/色彩/款式数据库 migrations 已执行
交付物:

【面料数据库】
  - migrations/002_fabric.sql：面料表（id/name/composition/weight/season/applicable_styles/care_instructions/features）
  - scripts/seed_fabric.sql：初始数据 ≥100 条（覆盖棉/麻/丝/毛/化纤）
  - search_fabric 工具：按名称/成分/季节关键词搜索，返回面料��表
  - filter_by_season 工具：按季节过滤（spring/summer/autumn/winter/all-season）
  - get_applicable_styles 工具：按面料 ID 返回适用款式列表

【色彩数据库】
  - migrations/003_color.sql：色彩表（id/hex/name/rgb/hsl/category/season）
  - scripts/seed_color.sql：初始数据 ≥200 条
  - suggest_palette 工具：主色 HEX + 方案类型（6种）→ 返回配色方案（含 HEX 色值）
  - color_harmony 工具：分析一组颜色和谐度（0-100 分）+ 调整建议
  - trend_colors 工具：返回当季流行色列表 ≥10 色

【款式数据库】
  - migrations/004_style.sql：款式表（id/name/description/silhouette/garment_type/key_features/suitable_seasons/target_audience）
  - scripts/seed_style.sql：初始数据 ≥50 条（覆盖主要服装品类）
  - generate_style_ideas 工具：关键词列表 + 服装类型 → 返回款式灵感（含描述和设计要点）
  - style_variations 工具：基础款式 + 变体类型（5种）→ 返回变体方案
  - trend_analysis 工具：返回趋势分析报告（轮廓/细节/面料/色彩）

验收标准:
  - 3 个数据库 migrations 可独立执行，无报错
  - 每个工具的 API 测试覆盖：输入有效值返回正确结果，输入无效值返回错误提示
  - 对话中触发任意 Skill 时，AI 正确调用对应工具并在其回复中引用返回结果
  - 面料/色彩数据支持按 org_id + dept_id 隔离查询
```

---

### T-013: 真实 LLM 接入（MiniMax + DeepSeek）

```
标题: [T-013] 真实 LLM 接入（MiniMax + DeepSeek）
负责人: backend
依赖: T-004 (Go API 网关), T-005 (Rust 核心服务)
前置条件:
  - MiniMax API Key 已配置（MINIMAX_API_KEY 环境变量）
  - DeepSeek API Key 已配置（DEEPSEEK_API_KEY 环境变量）
  - Rust 核心服务网络可达 LLM API
交付物:
  - MiniMax LLM 适配器（Rust）：调用 MiniMax Chat Completions API，支持 stream/non-stream
  - DeepSeek LLM 适配器（Rust）：调用 DeepSeek Chat Completions API，支持 stream/non-stream
  - 模型路由配置：config/models.toml — 模型名 ↔ API 端点/密钥映射
  - 适配器接口抽象：trait LLMClient，统一 call(messages, stream) → Response/Stream
  - Token 计费：usage 字段正确返回 prompt_tokens/completion_tokens/total_tokens
  - 模型切换 API：前端可选择使用 MiniMax 或 DeepSeek（通过 extra_body.model 传递）
  - LLM 健康检查：定时探测两个 LLM 服务的可用性
验收标准:
  - 前端发送消息 → 真实 LLM 流式回复（response 不是 mock 内容）
  - MiniMax 和 DeepSeek 均能正常响应，切换后对话连贯
  - Token 消耗记录持久化到 messages 表（token_count 字段）
  - MiniMax 或 DeepSeek 服务异常时，返回 502 + 友好错误提示，不崩溃
  - 无 LLM Key 时启动失败（fail-fast，不静默降级）
```

---

### T-014: Jiev 意图分类层（消息 → Skill 路由）

```
标题: [T-014] Jiev 意图分类层（消息 → Skill 路由）
负责人: backend
依赖: T-011 (SKILL 引擎), T-012 (3个 Skill 后端), T-013 (真实 LLM)
前置条件:
  - 3 个 Skill 的工具均已注册且可执行
  - LLM 可正常调用
交付物:
  - 意图分类器（Jiev）：
    - 输入：用户消息文本
    - 输出：意图标签（fabric / color / style / general）
    - 候选：多候选时返回多个可能的 Skill（用于模糊匹配场景）
  - 路由引擎：
    - 意图标签 → 激活对应的 Skill
    - Skill system prompt 注入：将激活 Skill 的 prompt_template 注入到 LLM messages 上下文
    - 多 Skill 同时激活支持（高级场景）
  - 意图分类 API：POST /internal/intent/classify — 内部调用
  - 默认 Skill 配置：当意图为 general 时激活的默认 Skill（可配置）
  - 意图分类日志：记录每次分类的输入/输出/路由决策（用于优化）
  - 分类规则可配置：支持 YAML 配置关键词 ↔ 意图标签映射，无需改代码
意图分类示例:
  - "这件衬衫用什么面料好？" → intent=fabric → 激活 fabric-query
  - "蓝色配什么颜色好看？" → intent=color → 激活 color-matching
  - "给我一些休闲款式的灵感" → intent=style → 激活 style-inspiration
  - "帮我看看这个设计怎么样" → intent=general → 通用对话
验收标准:
  - 面料/色彩/款式关键词问题 → 正确路由到对应 Skill，tool_calls 包含对应工具
  - 无法明确分类时降级为 general 对话，不报错
  - 意图分类 P99 延迟 < 200ms（不阻塞首 token 输出）
  - 分类规则变更（改 YAML）后无需重启服务
```

---

### T-015: RAG 知识库（管理员上传 + 检索）

```
标题: [T-015] RAG 知识库（管理员上传 + 检索）
负责人: backend
依赖: T-003 (DB schema), T-013 (真实 LLM)
前置条件:
  - Qdrant 向量数据库已就绪（Phase 1A docker-compose 已配置）
  - Embedding 模型可用（MiniMax/DeepSeek embed API 或本地部署）
交付物:
  - 知识库文档表：migrations/005_knowledge.sql
    - id / org_id / dept_id / title / file_type / file_path / chunk_count / status / uploaded_by / uploaded_at
  - 文档上传 API：POST /api/knowledge/documents
    - 支持格式：.pdf / .docx / .txt / .md
    - 限制：单个文件 ≤50MB，支持批量上传
  - 文档解析服务：
    - PDF 解析（pdf-extract 或 similar）
    - DOCX 解析（python-docx）
    - 文本直接读取
  - 向量化服务：
    - chunk 策略：固定 chunk_size=500 tokens，overlap=50 tokens
    - embedding 调用（MiniMax/DeepSeek embed API 或本地模型）
    - 批量向量化（>10 页文档异步处理）
  - Qdrant 存储：
    - collection：fashion_knowledge（按 org_id 分区）
    - payload：chunk_text / doc_id / dept_id / title
  - 检索 API：POST /internal/knowledge/search
    - 输入：query_text + org_id + dept_id（自动过滤其他部门）
    - 输出：Top-K 相关 chunk 片段 + 相似度分数
  - RAG 注入：对话时自动将检索结果注入 LLM context（附引用来源）
  - 知识库管理 UI（前端）：
    - 管理员上传文档（进度条展示）
    - 文档列表（按部门筛选）
    - 删除文档（软删除，更新 status=deleted）
  - 部门完全隔离：用户只能检索自己 dept_id 下的文档
验收标准:
  - 管理员上传一份 10 页 PDF → 解析成功 → chunks 入 Qdrant
  - 用户发送涉及知识库内容的问题 → AI 回复中引用文档内容（含引用标记）
  - dept_id=A 的用户无法检索 dept_id=B 的文档（数据库层面强制隔离）
  - 检索延迟 < 500ms（10K chunks 规模）
  - 支持至少 1000 篇文档（10 万 chunks 规模）
```

---

### T-016: MCP 客户端（连接企业 MCP 服务器）

```
标题: [T-016] MCP 客户端（连接企业 MCP 服务器）
负责人: backend
依赖: T-011 (SKILL 引擎), T-013 (真实 LLM)
前置条件:
  - 至少一个企业 MCP 服务器端点可用（用于联调测试）
  - SKILL 引擎的工具注册接口已就绪
交付物:
  - MCP 客户端库集成（Rust MCP SDK）：
    - 支持 MCP over HTTP（SSE 或 WebSocket）
    - 自动重连（断线重连 + 退避策略）
  - MCP Server 注册表：
    - 表：mcp_servers（id/org_id/name/endpoint/auth_token/description/enabled）
    - 管理员可添加/编辑/禁用 MCP Server
  - MCP Server 管理 API：
    - GET /api/mcp/servers — 列表
    - POST /api/mcp/servers — 添加
    - PUT /api/mcp/servers/:id — 更新
    - DELETE /api/mcp/servers/:id — 删除（软删除）
  - MCP 工具发现：启动时从 MCP Server 获取工具列表（tools/list）
  - MCP 工具路由：MCP 工具在 SKILL 引擎中注册为工具，Agent Loop 可调用
  - MCP 工具执行：接收 tool_call → 转发到 MCP Server → 返回结果
  - 用户绑定：用户可选择启用哪些 MCP Server（存储在 users 表 mcp_server_ids 字段）
  - 前端 MCP 管理 UI：
    - MCP Server 列表（名称/端点/状态）
    - 添加/编辑/禁用 Server
    - 用户 MCP Server 启用开关
验收标准:
  - 管理员添加 MCP Server 端点 → 工具列表自动发现 → 工具出现在 Skill 面板
  - 用户开启 MCP Server → 对话中触发该 Server 工具 → 正确调用并返回结果
  - MCP Server 不可用时，优雅降级（不阻塞对话，返回工具不可用提示）
  - 跨 org 隔离：每个 org 只能看到和管理自己的 MCP Server
```

---

### T-017: 权限细化（Casbin PreToolCall Hook 执行层验证）

```
标题: [T-017] 权限细化（Casbin PreToolCall Hook 执行层验证）
负责人: backend
依赖: T-011 (SKILL 引擎), T-006 (用户系统 RBAC)
前置条件:
  - 用户/角色/部门表已就绪
  - SKILL 引擎可接收 tool_call 请求
交付物:
  - Casbin 权限模型（model.conf）：
    - 主体：user_id, role, dept_id
    - 资源：skill:{skill-id}, tool:{tool-name}, data:{resource}:{action}
    - 操作：read, write, execute, admin
  - 权限策略表（migrations/006_permissions.sql）：
    - roles 表新增 permissions JSONB 字段
    - 支持角色级别的权限授予（admin > designer > viewer 继承链）
  - PreToolCall Hook 实现（Rust）：
    - 在 Agent Loop 每次执行 tool_call 前拦截
    - 提取 user_context（user_id/role/dept_id）
    - 查询用户角色权限（Redis 缓存，加速）
    - 调用 Casbin Enforcer.enforce(sub, obj, act)
    - 允许：继续执行 tool
    - 拒绝：返回 403 + 权限不足消息（不调用 LLM）
  - 权限管理 API：
    - GET /api/roles/:id/permissions — 获取角色权限
    - PUT /api/roles/:id/permissions — 更新角色权限（仅 admin）
  - 权限日志：每次 tool_call 的权限检查结果写入 audit_logs（user_id/action/resource/result）
  - 前端权限展示：
    - Skill 面板：根据用户权限显示 Skill 启用/禁用状态
    - 禁用 Skill 触发时显示"无权使用此技能"提示
验收标准:
  - admin 角色：可使用所有 Skill 和工具
  - designer 角色：可使用 fabric-query / color-matching / style-inspiration，无权管理 MCP Server
  - viewer 角色：只能对话，无权调用任何 Skill 工具
  - 权限检查在 tool_call 执行前完成，不泄漏任何无权访问的数据
  - 权限检查 P99 延迟 < 10ms（Redis 缓存生效）
  - 权限变更（更新角色）后 5 秒内生效（缓存 TTL ≤5s）
```

---

### T-018: 项目归档（设计师按项目组织会话）

```
标题: [T-018] 项目归档（设计师按项目组织会话）
负责人: backend + frontend
依赖: T-003 (DB schema), T-006 (用户系统), T-007 (前端最小闭环)
前置条件:
  - sessions 表已就绪（Phase 1A T-003）
交付物:
  - 项目表（migrations/007_projects.sql）：
    - id / org_id / dept_id / name / description / cover_color / owner_id / created_at / updated_at / is_archived
  - 会话-项目关联表（session_project）：
    - session_id / project_id / added_at
    - 一个会话可属于多个项目（多对多）
  - 项目管理 API：
    - POST /api/projects — 创建项目
    - GET /api/projects — 列表（按 org_id/dept_id 过滤）
    - GET /api/projects/:id — 项目详情（含会话列表）
    - PUT /api/projects/:id — 更新项目（名称/描述/封面色）
    - DELETE /api/projects/:id — 删除项目（软删除，不删除会话）
    - POST /api/projects/:id/sessions — 将会话加入项目
    - DELETE /api/projects/:id/sessions/:session_id — 从项目移除会话
    - POST /api/projects/:id/archive — 归档项目
    - POST /api/projects/:id/unarchive — 取消归档
  - 前端项目 UI：
    - 侧边栏项目列表（当前项目高亮）
    - 项目创建/编辑/归档
    - 项目详情页：会话卡片列表（可拖拽排序）
    - 会话详情页：可选择"添加到项目"（多选）
    - 归档项目独立分组（折叠/展开）
  - 部门隔离：用户只能查看/操作自己 dept_id 下的项目
验收标准:
  - 设计师创建项目 → 将相关会话加入 → 项目详情页展示所有会话
  - 项目归档后出现在"已归档"分组，不出现在主列表
  - 会话删除后自动从所有项目中移除（级联处理）
  - 多对多关联：一个会话可出现在多个项目中
  - 项目列表按 updated_at 倒序排列（最新活动优先）
```

---

### T-019: CLIP 以图搜图（上传图片找相似款式）

```
标题: [T-019] CLIP 以图搜图（上传图片找相似款式）
负责人: backend
依赖: T-003 (DB schema), T-013 (真实 LLM), T-015 (RAG 向量存储)
前置条件:
  - Qdrant 向量数据库已就绪（Phase 1A + T-015）
  - CLIP 模型可就绪（本地部署或第三方 API）
交付物:
  - CLIP 模型服务：
    - 方案 A：本地部署 CLIP（Rust + candle 或 ONNX Runtime）
    - 方案 B：调用第三方 CLIP API（如 Replicate / OpenCLIP）
    - 环境变量 CLIPE API_ENDPOINT 切换两种方案
  - 款式图片数据库：
    - migrations/008_style_images.sql：
      - id / style_id (FK) / image_url / clip_vector (pgvector) / uploaded_by / created_at
    - 款式图片上传到 MinIO
  - 图片向量化 pipeline：
    - 上传图片 → CLIP encoder → 提取特征向量（768-dim）
    - 存入 Qdrant（collection: style_images）或 PostgreSQL pgvector 列
  - 以图搜图 API：
    - POST /internal/images/similar — 接收图片文件或 Base64，返回相似款式列表
    - 流程：图片 → CLIP encode → 向量检索（Qdrant）→ 返回 Top-K 相似款式（含图片 URL 和相似度分数）
  - 款式图库管理 API：
    - POST /api/styles/:id/images — 上传款式图片
    - GET /api/styles/:id/images — 获取款式图片列表
    - DELETE /api/styles/:id/images/:image_id — 删除图片
  - 前端图片搜索 UI：
    - 对话输入框：支持上传图片（拖拽/点击）
    - 图片预览 + 发送 → 显示相似款式结果（图片网格 + 款式名称 + 相似度）
    - 款式详情页：点击图片可触发"找相似"功能
  - 跨模态检索：图片→款式（以图搜图），文本→款式（以文搜图）共用同一向量空间
验收标准:
  - 上传一张服装图片 → 返回 ≥5 个相似款式（图片 + 款式名称 + 相似度分数）
  - 相似款式按相似度倒序排列
  - 款式图片仅返回当前用户 dept_id 下的款式
  - 以图搜图延迟 < 2s（不含图片上传）
  - 支持常见图片格式：jpg / png / webp，图片大小 ≤10MB
```

---

## 依赖关系图

```
Phase 1A
  T-005 ─────────────────┐
  T-003 ───────────────┤
  T-008 ───────────────┤
                        │
Phase 1B ──────────────┴──────────────────────────────────────────

  SKILL引擎基础设施 ───────────────────────────────────────────────
  T-011 ──┬───────────────────────────────────────────────────────┐

  Skill后端 ───────────────┐                                       │
  T-012 ───┘              │                                       │
                           │                                       │
  真实LLM ─────────────────┼── T-014 ──┐                          │
  T-013 ───────────────────┤           │                          │
                           │           │                          │
  SKILL引擎 ───────────────┘           │                          │
  T-011 ───────────────────────────────┼── T-015 ──┐             │
                                         │           │             │
  真实LLM ──────────────────────────────┘           │             │
  T-013 ────────────────────────────────────────────┼─────────────┤
                                                       │             │
  RAG向量 ────────────────────────────────────────────┘             │
  T-015 ────────────────────────────────────────────────────────────┤

  MCP ─────────────────────────────────────────────────────────────
  T-016 ───┬───────────────────────────────────────────────────────┘

  权限 ────────────────────────────────────────────────────────────
  T-017 ───┘

  项目 ─────────────────────────────────────────────────────────────
  T-018 ───┐

  CLIP ─────────────────────────────────────────────────────────────
  T-019 ───┘
```

---

## 派发顺序（4 批次，8~12 周）

### 第 1 批（Week 5-6，可并行）
- **T-011** SKILL 引擎核心 — 所有 Skill 的基础设施，必须最早完成
- **T-013** 真实 LLM 接入 — 对话能力的必要条件
- **T-018** 项目归档 — 独立功能，与 Skill 引擎无依赖

### 第 2 批（Week 7-8）
- **T-012** 3 个 Skill 后端实现 — 依赖 T-011 的引擎接口
- **T-017** Casbin 权限细化 — 依赖 T-011 的工具调用 Hook

### 第 3 批（Week 9-10）
- **T-014** Jiev 意图分类 — 依赖 T-011 + T-012 + T-013
- **T-015** RAG 知识库 — 依赖 T-003 + T-013
- **T-016** MCP 客户端 — 依赖 T-011 + T-013

### 第 4 批（Week 11-12）
- **T-019** CLIP 以图搜图 — 依赖 T-013 + T-015（向量存储）

---

## 验收总览

| T卡 | 功能 | 验收方式 |
|-----|------|----------|
| T-011 | SKILL 引擎 | Agent Loop 调用 Skill 工具，集成测试通过 |
| T-012 | 3 个 Skill 后端 | 对话触发各 Skill → 工具调用 → 回复引用结果 |
| T-013 | 真实 LLM | 对话回复非 mock，含 token 消耗记录 |
| T-014 | Jiev 意图分类 | 面料/色彩/款式问题 → 正确 Skill 路由 |
| T-015 | RAG 知识库 | 上传文档 → 检索 → AI 引用回复 |
| T-016 | MCP 客户端 | MCP Server 注册 → 工具发现 → 正确调用 |
| T-017 | Casbin 权限 | admin/designer/viewer 各角色权限隔离生效 |
| T-018 | 项目归档 | 项目创建 → 会话分组 → 归档/取消归档 |
| T-019 | CLIP 以图搜图 | 上传图片 → 返回相似款式（≥5个，含相似度） |

---

## 需要确认的设计决策

1. **意图分类实现**：基于规则关键词匹配（快、准、可解释）vs 小模型分类（更准但慢）？建议 Phase 1B 先用规则，T-014 中预留 LLM 降级路径。

2. **CLIP 模型部署**：本地部署（GPU 成本）vs 第三方 API（延迟/费用）？建议 Phase 1B 用第三方 API（如 Replicate）快速验收，Phase 2 评估本地部署。

3. **向量模型（embedding）**：MiniMax/DeepSeek embed API vs 本地 sentence-transformers？建议 Phase 1B 用 DeepSeek embed API（成本低、延迟低）。

4. **MCP 服务器联调**：Phase 1B 需要至少一个真实 MCP Server 端点用于测试。请确认是否有现成的企业 MCP Server，或需要自建 mock MCP Server 供开发使用。

5. **面料/色彩/款式初始数据来源**：建议 Phase 1B 用程序生成的 mock 数据（100~200 条）快速验收，Phase 2 引入真实数据集。
