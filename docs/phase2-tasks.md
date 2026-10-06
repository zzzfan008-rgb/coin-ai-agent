# Phase 2 Kanban Cards — Coin-AI (Fashion AI Platform)

> Phase 2：「能力深化与本地化」—— 在 Phase 1B 已跑通的真实全链路之上，引入真实数据集、评估/落地本地 GPU CLIP、落地企业 MCP 规范、接入 ComfyUI 图片生成，并偿还 Phase 1B 门禁遗留的 P2 安全/工程债。
>
> 承接 Phase 1B 已确认的决策：D2（本地 CLIP 评估）、D4（真实数据集）、D5（企业 MCP 规范共同定义，企业负责实现）；架构已预留 GPU（CLIP + ComfyUI）。

## Board
- **Slug**: fashion-ai-platform
- **Repo**: ~/projects/fashion-ai-platform
- **Phase**: 2（承接 Phase 1B，commit 16f896d）
- **目标工期**: 8~12 周（含 GPU/数据授权/企业联调等外部等待）
- **绝对约束（SPEC，Phase 2 持续遵守）**：
  - 本地部署，仅 LLM API 出公网（本地 CLIP/ComfyUI 落地后，以图搜图与图片生成不再出公网）
  - 管理员控制 Skill/MCP 安装权限
  - 权限深入 tool 调用前（Casbin PreToolCall Hook）
  - RBAC 三层隔离 org_id + dept_id
  - 会话永久保留 + 项目归档
  - 20 人团队，无系统集成，不考虑完全离线

---

## 功能全景

| # | 功能 | 说明 | 类型 |
|---|------|------|------|
| T-020 | 多租户用户名唯一性 | (org_id, username) 唯一约束 | 遗留 P2 修复 |
| T-021 | JWT → httpOnly Cookie | 消除 XSS 风险，防 XSS | 遗留 P2 修复 |
| T-022 | SSE 超时配置 | 流式健壮性 | 遗留 P2 修复 |
| T-023 | 前端 test/lint 脚本 | 工程化门禁 | 遗留 P2 修复 |
| T-024 | 真实数据集引入 | 面料/色彩/款式替换 mock | 能力深化 |
| T-025 | 企业 MCP 规范 + 参考实现 | D5 落地，共同定义规范 | 能力深化 |
| T-026 | 本地 CLIP 部署评估 | D2 POC + 基准 + 决策 | 能力深化 |
| T-027 | 混合 CLIP 落地 | 本地模型 + API fallback | 能力深化 |
| T-028 | ComfyUI 服务 + 生成 Skill | 本地 GPU 文生图 | 新能力 |
| T-029 | 图片生成 UI + 生成闭环 | 生成→入库→以图搜图 | 新能力 |

---

## Card Definitions

> 每张卡在 Phase 1B 字段（目标/交付物/验收标准/依赖）基础上，新增 **预估 / 优先级 / 能否并行 / 依赖的已完成模块**。

---

### T-020: 多租户用户名唯一性约束

```
标题: [T-020] 多租户用户名唯一性约束（(org_id, username) 唯一）
负责人: backend
优先级: P0（多租户数据完整性）
预估: 1 人周
能否并行: 是（无对外依赖）
依赖的已完成模块: migrations/001 (users 表)、T-006 用户系统、T-004 网关
依赖: 无 Phase 2 前序卡
前置条件:
  - users 表存在 username / org_id 列
  - register 逻辑在网关侧（T-006）
交付物:
  - migrations/017_user_org_username_unique.sql：users(org_id, username) 建唯一约束；
    迁移前对同 org 重复 username 去重/回填（保留最早或最新，策略写入迁移注释），幂等可回滚
  - 网关 register handler：捕获唯一约束冲突 → 返回 409（"用户名已存在"）而非 500
  - 前端注册页错误提示（409 → 展示友好文案）
  - 集成测试：同 org 重复用户名 → 409；不同 org 相同用户名 → 201 成功
验收标准:
  - DB 层面 (org_id, username) 唯一约束生效（直接 INSERT 重复 → 报错）
  - 同 org 重复用户名注册 → 409 友好提示，不再 500
  - 不同 org 注册相同用户名 → 201 成功
  - 既有用户数据迁移后无丢失/损坏（幂等迁移，重复跑行数不变）
```

#### T-020 实施记录（2026-10-05，orchestrator 勘察）

- **DB 唯一约束：已存在，无需新建迁移**。`migrations/001_init_schema.sql:65`
  已有 `CONSTRAINT uq_user_username_org UNIQUE (org_id, username)`（附
  `migrations/001_init_schema.sql:324` 复合索引）。卡片原交付物
  `migrations/017_user_org_username_unique.sql` 不再需要——017 号迁移不存在
  （migrations 目录 001/008-012/014-016/018-020），任何新库建表即含约束。
- **产品决策：保持 username 全局唯一，不采纳「不同 org 相同用户名 → 201」**。
  原因：`api-gateway/internal/service/service.go:147-159` Login 按
  `username` 单字段查找、无 org 参数；若放开为 (org_id, username) 唯一，
  登录将产生跨租户歧义（需先选 org 才能验密）。当前 register
  （`service.go:71-`）每次注册新建 org，同 org 重复注册在注册路径不可达；
  全局查重（`service.go:79`）是对该语义的实现。
- **409 路径：已实现**。`api-gateway/internal/handler/handler.go:57-58`
  捕获 `service.ErrUserExists`（`service.go:22`）→ 409 "username already exists"。
- **进行中**：backend 补「二次注册 → 409」测试覆盖（此前 0 覆盖，handler_test
  仅含参数校验测试）；前端 409 友好文案随后由 frontend 处理。
- **race 边界已收口（2026-10-05）**：新增 `migrations/021_user_username_global_unique.sql`
  （跨 org 重复检测 DO block + `uq_users_username_global` 全局唯一索引，001 组合
  约束保持不变），`service.go` 的 users INSERT 捕获 23505 并按约束名过滤映射
  `ErrUserExists`（仅 `uq_users_username_global` / `uq_user_username_org`），并发
  同名注册不再各自建 org 后双插成功。handler 409 路径无需改动。

---

### T-021: JWT 迁移 httpOnly Cookie（防 XSS）

```
标题: [T-021] JWT 迁移 httpOnly Cookie（防 XSS）
负责人: backend + frontend
优先级: P1（XSS 安全债；涉及认证契约，需拍板）
预估: 2 人周
能否并行: 是（但需先拍板"双轨"方案）
依赖的已完成模块: T-004 网关 JWT 中间件、T-007 前端登录态（localStorage）
依赖: 无 Phase 2 前序卡
前置条件:
  - 登录/注册签发 JWT 的现有链路已通（T-006/T-004）
交付物:
  - 网关：登录/注册响应 Set-Cookie（HttpOnly + SameSite=Lax/Strict + Secure 可配置）
  - 鉴权中间件双轨：同时接受 Cookie 与 Authorization Bearer
  - CSRF 防护：SameSite + 自定义头校验（或 double-submit token），作用于非幂等写请求
  - 前端：登录后不再读写 localStorage token，fetch 走 cookie（withCredentials）
  - 登出：清除 cookie
  - OpenAI 兼容端点 /v1/chat/completions 继续接受 Bearer（API 消费者不受影响）
验收标准:
  - 登录响应带 HttpOnly Cookie，前端 JS 读不到 document.cookie（XSS 无法窃取 token）
  - 前端刷新/重开会话保持登录（cookie 生效）
  - 伪造跨站请求（无 CSRF 头 / SameSite 不匹配）被拒
  - 无 Authorization 头、仅凭 cookie 可调用受保护 API
  - Bearer token 路径回归通过（OpenAI 兼容端点不受影响）
```

---

### T-022: SSE 超时配置 + 流式健壮性

```
标题: [T-022] SSE 超时配置 + 流式健壮性
负责人: backend + frontend
优先级: P1（长流式回复可靠性）
预估: 1 人周
能否并行: 是
依赖的已完成模块: T-005 api-core SSE 输出、T-004 网关代理、T-007 前端流式展示
依赖: 无 Phase 2 前序卡
前置条件:
  - SSE 流式对话链路已通（T-013/qwen）
交付物:
  - 配置：SSE_IDLE_TIMEOUT / SSE_WRITE_TIMEOUT 环境变量 + config 字段（core 与网关）
  - core SSE handler 按配置设置空闲超时，长回复（含慢生成/长思考）不被误断
  - 网关代理对 SSE 禁用响应缓冲、透传超时配置
  - 前端：流式中断时友好提示 + 手动重试，不卡死
验收标准:
  - 长流式回复（如单次生成 >60s 且中间无 token 的空闲间隔）不被中途断开
  - 修改超时配置无需改代码（读 env 重启生效），部署文档记录默认值
  - 网络中断时前端显示友好提示而非无限转圈
```

---

### T-023: 前端 test/lint 脚本 + 工程化门禁

```
标题: [T-023] 前端 test/lint 脚本 + 工程化门禁
负责人: frontend
优先级: P2（工程化债）
预估: 1.5 人周
能否并行: 是
依赖的已完成模块: T-007 web-client（Vite + React + TS）
依赖: 无 Phase 2 前序卡
前置条件:
  - web-client 可构建（tsc --noEmit + vite 通过）
交付物:
  - package.json 脚本：lint（eslint）、test（vitest + @testing-library/react）、typecheck
  - 最小覆盖：登录页、对话输入、消息渲染的组件/单元测试
  - 复用 T-009 tests/e2e（可选）补关键路径 e2e
  - 贡献文档记录命令（README/贡献指南）
验收标准:
  - npm run lint 通过（0 error）
  - npm run test 通过（覆盖核心组件，无空断言/假通过）
  - npm run typecheck 通过
  - （如有 CI）CI 挂载 lint + test + typecheck 三道门禁
```

---

### T-024: 真实数据集引入（面料/色彩/款式替换 mock）

```
标题: [T-024] 真实数据集引入（面料/色彩/款式替换 mock）
负责人: backend + designer
优先级: P1（高产品价值）
预估: 3 人周（含数据清洗；不含授权谈判等待）
能否并行: 是（独立于 GPU/MCP）
依赖的已完成模块: T-012（010/011/012 表结构 + seed 结构）、fabric-query/color-matching/style-inspiration 三个 Skill
依赖: 无 Phase 2 前序卡（但建议在 T-020 迁移稳定后跑导入）
前置条件:
  - 面料/色彩/款式三张表结构已定型（T-012）
  - 数据源与授权已确认（见决策点 4）
交付物:
  - 数据源选型 + 授权确认，写成 docs/datasets.md（来源/许可/字段映射/清洗规则）
  - 清洗/规范化脚本（scripts/import_fabric.py / import_color.py / import_style.py 或等价 SQL）
  - 新 seed migration（或独立导入 pipeline），幂等、可回滚，与既有 mock seed 可共存或替换
  - 质量校验：字段完整性、季节/成分/品类分布报告
验收标准:
  - 导入后真实数据量达标（目标以数据源为准，建议面料 ≥500、色彩 ≥300、款式 ≥100）
  - 真实数据可被 search_fabric / suggest_palette / style_variations 等工具正常查询
  - 跨部门隔离在真实数据上仍生效（dept002 查 dept001 数据 → 0）
  - 导入可重复执行（幂等），不产生重复行
  - 三个 Skill 对话引用真实数据（非 mock 内容）
```

---

### T-025: 企业 MCP 规范定义 + 参考实现 + conformance

```
标题: [T-025] 企业 MCP 规范定义 + 参考实现 + conformance
负责人: backend + architect
优先级: P1（外部协作周期长，尽早启动）
预估: 3 人周（平台侧；企业开发周期另计）
能否并行: 是（仅依赖 T-016）
依赖的已完成模块: T-016（MCP 客户端 + mcp_servers 表 + McpAdmin 管理 UI）、T-017 Casbin
依赖: 无 Phase 2 前序卡
前置条件:
  - 至少一名企业技术联系人/联调端点（决策点 5）
  - mock MCP server（:9101）可供开发对照
交付物:
  - 规范文档 docs/enterprise-mcp-spec.md：
    传输（Streamable HTTP / SSE）、工具发现 tools/list、认证（token 与 org 租户绑定）、
    错误码、版本协商、限流、审计字段、工具命名规范
  - 参考实现：一个企业 MCP Server 脚手架/模板（含 fabric 数据示例），企业 clone 即开发
  - 平台 conformance 测试套件：验证一个 MCP Server 是否符合规范（发现/调用/认证/错误）
  - 平台管理面补强：MCP Server 健康检查、版本/状态展示、认证 token 管理
验收标准:
  - 企业按规范开发的最小 MCP Server 能通过平台 conformance 测试
  - 平台能连接企业 MCP Server、自动发现工具、在对话中真实调用
  - 认证 + 租户绑定生效（非本 org 用户不能调用该 Server 工具）
  - 规范文档经企业技术方评审确认（有记录）
```

---

### T-026: 本地 CLIP 部署评估（POC + 基准 + 决策）

```
标题: [T-026] 本地 CLIP 部署评估（POC + 基准 + 决策）
负责人: backend
优先级: P1（解锁成本/数据主权；决策前置）
预估: 2 人周（不含 GPU 采购等待）
能否并行: 是（独立 POC；与 T-024/T-025 并行）
依赖的已完成模块: T-019（ClipClient 抽象 + ClipMode 枚举）、架构预留 GPU
依赖: 无 Phase 2 前序卡（T-027 依赖本卡结论）
前置条件:
  - GPU 硬件到位（型号/显存，决策点 1）
  - 现有 ClipClient 的 ClipMode::Generic / DashScope 两模式已通
交付物:
  - POC：本地 CLIP 服务（ONNX Runtime / candle / 独立 FastAPI CLIP 容器，三选一验证），
    加载 ViT-B/32 或 ViT-L/14
  - 基准报告：编码延迟（CPU vs GPU）、检索质量（与 DashScope 的 recall 对比）、显存/资源占用
  - 维度对齐方案：本地 512/768 维 vs DashScope 1024 维的集合迁移策略（重索引 vs 固定统一维度）
  - ADR 决策记录：本地 / 混合 / 纯 API 的取舍 + 推荐方案
验收标准:
  - 本地 CLIP 服务可运行并返回正确维度向量
  - 基准数据量化（P99 延迟、GPU 显存、检索 recall）
  - 出具评估报告 + 明确推荐方案（供 T-027 执行）
  - 明确 GPU 硬件是否满足要求（不足则给出最低规格建议）
```

**T-026 实施记录（2026-10-05）**
- POC 第一阶段完成，报告：`docs/t026-clip-poc-phase1.md`；脚本：`scripts/clip_poc_server.py`（FastAPI :8399，契约对齐 ClipClient Generic 模式）、`scripts/clip_poc_bench.py`。
- 决策点 1 结论：本机 MacBook Air M4 / 16GB **无独立 GPU**；MPS 单张仅快 18%、批量吞吐反慢 25%（43 vs 32 img/s）、文本 P99 劣化 4.6x → 本地服务用 **CPU device**。
- 基准：CPU 端到端 P50 54.2ms / MPS 43.5ms；吞吐 CPU 42.99 img/s；dim=512 双路一致。数据：`data/poc_clip_bench.json`（gitignored）。
- 验收「服务可运行 + 正确维度」已 curl 实测（image/text 均 512 维）；质量基线沿用 T-024（40.24%）。
- **外部阻塞**：DashScope recall 对比缺 `DASHSCOPE_API_KEY`（.env 无）；key 到位后同脚本补跑即可。
- ADR 草案：混合 fallback 值得做；维度对齐推荐双集合共存（collection 元数据标 dim/model）；未来 ViT-L/14 生产化最低建议 8GB VRAM GPU。供 T-027 执行。
- 备注：backend agent 实施中断（provider 超时），环境补装/基准/报告由 orchestrator 收尾；T-027 接线（api-core ↔ localhost）未动。

---

### T-027: 混合 CLIP 落地（本地模型 + API fallback）

```
标题: [T-027] 混合 CLIP 落地（本地模型 + API fallback）
负责人: backend
优先级: P2（依赖 T-026 决策）
预估: 2 人周
能否并行: 否（依赖 T-026 结论）
依赖的已完成模块: T-019（ClipClient）、style_images 集合（Qdrant）、T-026 评估结论
依赖: T-026（评估/决策）
前置条件:
  - T-026 已出具推荐方案（本地或混合）
交付物:
  - ClipClient 新增 Local 模式（本地 CLIP 服务端点）+ 双 provider fallback（本地失败 → DashScope API）
  - 维度对齐：CLIP_VECTOR_DIM 按 provider 动态解析；统一存储维度；
    重索引 pipeline（一键把既有 style_images 迁到本地维度）
  - 健康检查/降级监控：本地 CLIP 不可用时的告警与自动切换
  - 配置：CLIP_PROVIDER=local+fallback、本地端点、fallback 阈值
  - 修正 CLIP_VECTOR_DIM 默认值（现默认 512 与 DashScope 实际 1024 不一致，属潜在配置坑）
验收标准:
  - 本地 CLIP 可用时以图搜图走本地（不出公网）；本地故障时自动 fallback 到 DashScope，检索仍可用
  - 重索引后 style_images 与本地维度一致，检索正常
  - fallback 切换有日志/监控可观测
  - 延迟目标维持（<2s）
```

#### T-027 增量 1 实施记录（2026-10-05）

范围裁剪：重索引 pipeline、Qdrant 集合维度元数据、健康监控集成 → 留增量 2。本增量只做 ClipClient 本地模式 + fallback + 配置面 + CLIP_VECTOR_DIM 修正。

**配置面（均遵循现有 CLIP_* 环境变量模式，未改 config.rs）**

| 变量 | 缺省 | 说明 |
|---|---|---|
| `CLIP_PROVIDER` | 未设置=Generic | `local` 仅本地；`dashscope`/`qwen` 仅远端；`hybrid` 本地优先 + fallback；未设置/未知值 = Generic（与 T-019 既有行为一致，不破坏任何现有部署） |
| `CLIP_LOCAL_BASE_URL` | `http://127.0.0.1:8399` | 本地 CLIP 服务基址（local/hybrid），客户端拼接 `/encode/image`、`/encode/text` |
| `CLIP_LOCAL_TIMEOUT_SECS` | 5 | 本地请求超时（仅作用于本地端点，远端仍 30s） |
| `CLIP_API_ENDPOINT` | 未设置=Generic 模式 | Generic=完整端点 URL；DashScope/hybrid fallback=DashScope 基址，客户端追加 `/api/v1/services/embeddings/multimodal-embedding/multimodal-embedding` |
| `CLIP_VECTOR_DIM` | 按 provider 解析 | 显式配置最高优先；未设置时 dashscope/qwen→1024，其余（local/hybrid/generic/未知）→512（修正了旧默认 512 与 DashScope 实际 1024 不一致的配置坑） |

**fallback 语义（hybrid 模式）**
- fallbackable（自动降级 DashScope）：连接错误（含 connection refused）、超时、HTTP 5xx、响应形状不符（JSON 解析失败 / 无可识别 embedding 字段）。
- 不 fallback、fail-closed 直接报错：HTTP 4xx（本地服务可达但拒绝请求=配置/鉴权问题，不该被远端掩盖）、本地 URL 配置错误（reqwest builder error）。
- 可观测性：每次 fallback 打 `warn!` 日志，含失败原因与本地耗时（`elapsed_ms`）；成功路径 `embed`（indexer 日志 `provider` 字段）与 `query`（`/internal/images/search` 响应 `provider_used` 字段）两条路径都带 provider 元数据。
- hybrid 模式下本地 URL 为空 → 直接走 DashScope 并打 warn（不报错，保持可用性）。

**维度解析规则**：`images/mod.rs::resolve_vector_dim(explicit, provider)` —— 显式 `CLIP_VECTOR_DIM` > provider 默认（dashscope/qwen=1024，其余=512）。维度元数据/双集合/重索引在增量 2。

**关键文件与符号**（只列符号不列行号，行号易漂移，以符号名为准）
- `api-core/src/images/clip.rs`：`ClipMode`（Generic/DashScope/Local/Hybrid）、`ClipEmbedding`+`ClipProviderId`（provider_used 元数据载体）、`ClipConfig`（env 读取 + mode() 映射）、`ClipClient`、hybrid dispatch（私有 `encode()`）、`try_local` fallback 分类（`LocalFailure`）、响应形状解析 `parse_embedding`（OpenAI/proxy/Replicate/bare-array 四形状）、DashScope 解析 `parse_dashscope_embedding`。
- `api-core/src/images/mod.rs`：`resolve_vector_dim`、`SimilarSearchOutcome`（search_similar 返回值带 `provider_used`）、新维度解析测试。
- `api-core/src/images/indexer.rs`：embed 日志带 `provider`/`vector_dim`。
- `api-core/src/api/handlers/mod.rs`：`/internal/images/search` 响应增加 `provider_used`。
- `api-core/examples/clip_smoke.rs`：实机冒烟工具（不进 cargo test；`cargo run --example clip_smoke -- <jpg>`）。

**测试（api-core，wiremock 进程内 mock，零真实网络；本地失败路径用死端口 18399，DashScope 端用 wiremock 随机端口，仿 15432/16333 惯例）**
- clip.rs 新增 12 个：hybrid 本地成功且 DashScope 零调用（断言向量内容+512 维+provider=local）、本地不可达 fallback、本地 5xx fallback、本地形状不符 fallback、本地超时 fallback（300ms 超时 vs 3s 延迟 mock）、本地 4xx 不 fallback（dashscope 调用数=0）、dashscope 模式不碰本地（回归保护）、local 模式无 DashScope 也能跑、hybrid 空本地 URL 直接远端、Generic 模式行为不变（请求体仍 `{model,image}` 无 `input` 字段）、env→mode 映射、默认 model 随 mode。
- mod.rs 新增 2 个：`resolve_vector_dim` provider 默认 + 显式最高优先。
- 全量 cargo test：lib 71 passed（59 既有 + 12 新 clip + 2 新 dim）+ handlers_chat 4 + handlers_mcp 2 + handlers_skills 3 = 80 绿 0 红。
- cargo fmt：干净。cargo clippy：本机未安装该组件（`cargo-clippy is not installed`），留 CI 验证。

**实机冒烟证据（2026-10-05，data/images 任取一 jpg，非 cargo test）**
- 本地成功：`CLIP_PROVIDER=hybrid`（未设 endpoint）→ `encode OK: provider_used=local dim=512 elapsed_ms=138`，真实 8399 服务返回 512 维向量。
- fallback：`CLIP_PROVIDER=hybrid CLIP_LOCAL_BASE_URL=http://127.0.0.1:18399 CLIP_API_ENDPOINT=http://127.0.0.1:18400`（18400 为进程内 DashScope shape mock）→ `WARN ... Local CLIP request failed — falling back to DashScope provider="dashscope" elapsed_ms=3 error=local CLIP request failed: error sending request for url (http://127.0.0.1:18399/encode/image)`，随后 `encode OK: provider_used=dashscope dim=1024`。
    - 注：fallback 冒烟的 dashscope 路径输出来自进程内 mock（未入库）；等价行为已由 hermetic 测试 `clip::tests::hybrid_falls_back_when_local_unreachable` 等覆盖（wiremock 随机端口），复现以测试为准。

**增量 2 待办（T-027 剩余交付物）**
- 重索引 pipeline：一键把既有 style_images（512 维）迁到与本地维度一致的集合（ADR 推荐双集合共存：collection 元数据标 dim/model，新旧并存不删旧）。
- Qdrant 集合维度元数据：`style_images` 等集合创建/读取时携带 dim/model 元数据，查询前校验维度匹配，防错配静默失败。
- 健康监控集成：本地 CLIP `/health` 探活、不可用时告警 + 模式切换可观测（本增量只有 per-request warn 日志，无主动健康检查）。

#### T-027 增量 2 实施记录（2026-10-06）

范围：集合维度元数据 + 重索引 pipeline + CLIP 健康监控。未改 `CLIP_VECTOR_DIM` 语义、未动迁移文件与前端；存量集合只新增不改动。

**1. 元数据方案与取舍**

勘察了三种载体后采用「命名约定 + Qdrant 原生 collection metadata」双重方案：

| 方案 | 判断 |
|---|---|
| 元数据点（集合内固定 UUID + 零向量 payload） | 否决：Cosine 空间零向量退化，且污染搜索结果，需永久 filter 排除 |
| 仅命名约定 | 可用但元数据不经 API 可读、改名即丢失 |
| Qdrant 1.19 原生 collection metadata | **采用**：`PUT /collections/{name}` body 顶层 `metadata`（任意 JSON），collection-info 返回在 `result.config.metadata`；权威依据 Qdrant 官方 Collections 文档与 Create Collection API 参考（"application-specific information such as creation time, migration data, inference model"）。本机 Qdrant 1.19.1 实测落地（见冒烟证据） |
| 命名约定 | **同时保留**：作为第二道防线，不查 API 也能肉眼分辨维度，且路由按名称绑定 |

集合 ensure 语义（`QdrantStore::ensure_collection(dim, metadata)`）：已存在时读 `result.config.params.vectors.size` 与期望维度比对，**漂移即报错**（不再静默返回）；新增 `verify_collection_dim`（GET-only，缺失返回 false，绝不创建）供只读路由使用。

**2. 集合命名与理由**

命名函数 `images::style_collection_name(provider, model, dim)` →
`style_images_{provider}_{model-tag}_{dim}`：
- 本地 ViT-B/32：`style_images_local_clipvitb32_512`
- 未来 DashScope 专属集（本增量不创建）：`style_images_dashscope_mmembedv1_1024`
- 维度放末尾 token，列表/日志按嵌入空间排序，512 集合视觉上不可能与存量混淆；模型短标对已知模型显式映射（clip-vit-base-patch32→clipvitb32、multimodal-embedding-v1→mmembedv1），未知模型退化为 ASCII 字母数字小写，保证集合名合法。

各模式路由表（`ImageSearchService::from_env`）：
- Local：512 → 新集合（可写）
- Hybrid：512 → 新集合（可写）＋ 1024 → 存量 `style_images`（**只读**，fallback embedding 检索用，绝不创建/修改）
- DashScope/Generic：保持存量 `style_images`（Generic 行为不变，既有部署零影响）

写保护为代码级硬守卫：`QdrantStore::new_read_only` 绑定的 store 在任何 upsert/delete 前即报错、不发 HTTP；`PROTECTED_LEGACY_COLLECTIONS = [fashion_knowledge, style_images]` 白名单 + `is_protected_legacy()` 供 pipeline 入口断言。检索按**实际向量维度**路由（`route_for_dim`），512 向量在结构上不可能打到 1024 集合；无匹配维度 → fail-closed 报错。

**3. 重索引 pipeline**

- 库逻辑 `images::reindex.rs`（可 hermetic 测试）＋薄封装 `examples/reindex_clip.rs`（仿 clip_smoke，不进 cargo test）。
- 用法（api-core/ 下）：
  - `cargo run --example reindex_clip`（默认目录 ../data/images、batch=16）
  - `cargo run --example reindex_clip -- ../data/images 16`（位置参数：目录、批大小）
- 工具安全：CLIP_PROVIDER 未设置时强制 local；dashscope/generic 模式直接拒绝（exit 2）；目标集合名由 model+dim 派生，命中保护名单即中止；只实例化绑定新集合的一个 QdrantStore。
- 幂等：点 ID = `stable_point_id(filename)` = UUID v5（固定命名空间 `REINDEX_NAMESPACE`，改动命名空间即破坏幂等契约，注释已标注），同文件名任何机器/次数都解析到同一 ID，upsert 替换而非追加。
- provider 契约：每个 embedding 必须是 provider=local，否则整批中止（防止经未验证 provider 落数）。
- 输出统计：total/succeeded/failed、dim、批次数、耗时、唯一 ID 数、跑前后 points_count。

**4. 健康监控字段（GET /health，services.clip）**

零新依赖（reqwest + chrono + once_cell 均既有）：
- `mode`：当前生效模式（generic/dashscope/local/hybrid）
- `model`：客户端模型名
- `local`：`configured`（模式是否含本地端）、`reachable`（GET {base}/health 是否 2xx，超时 = CLIP_LOCAL_TIMEOUT_SECS）、`endpoint`、服务回报的 `model`/`device`/`dim`、`error`
- `last_fallback`：最近一次 fallback 事件 `{at RFC3339, reason, elapsed_ms}`，无则 null。hybrid dispatch 在既有 warn! 处同时写入进程级记录（`record_fallback`/`last_fallback`，OnceCell<RwLock>）；「hybrid 无本地 URL 直走 DashScope」也记录。

**5. 测试（全 hermetic：wiremock 进程内 mock；失败路径死端口 18399/16333，不碰真 :6333/:8399）**

新增 22 个 lib 单测：qdrant.rs 6（元数据创建请求体断言、同 dim 无 PUT、漂移报错、verify 缺失不创建、只读 upsert 零 HTTP、保护名单）、clip.rs 4（探活可达/死端口/非本地模式跳过/记录器）、images/mod.rs 6（命名 2、512/1024 路由、只读写拒绝、未知维度 fail-closed）、reindex.rs 6（稳定 ID、3 文件全 local、复跑同 ID、非 local 拒绝、保护目标拒绝、失败收集不丢批）。集成测试 handlers_chat +2（clip 段服务在/不在两态）。

全量结果：lib 93 passed（增量 1 的 71 + 22）、handlers_chat 6、handlers_mcp 2、handlers_skills 3，合计 **104 绿 0 红**。cargo fmt --check 干净。cargo build --example（clip_smoke + reindex_clip）通过。clippy 本机仍未安装，留 CI。

**6. 真机冒烟证据（2026-10-06，非 cargo test）**

- 重索引首跑（真 :8399 CPU + 真 :6333）：`Created Qdrant collection collection=style_images_local_clipvitb32_512 vector_dim=512`；`stats: total=82 succeeded=82 failed=0 dim=Some(512) batches=6 elapsed_ms=3767`；`point_ids: 82 unique of 82`；`points_count: before=Some(0) after=Some(82)`。
- 元数据实测（GET collection）：`config.metadata = {created_by:"api-core reindex_clip", model:"clip-vit-base-patch32", provider:"local", task:"T-027", vector_dim:512}`，`vectors: {size:512, distance:Cosine}`。
- 幂等复跑：`points_count: before=Some(82) after=Some(82)`，统计不变（82/82/0，6 批），无重复点。
- 健康两态（CORE_PORT=8091 真实服务）：
  - 服务在（8399 活）：`mode:"hybrid" local:{configured:true, reachable:true, device:"cpu", dim:512, error:null} last_fallback:null`
  - 服务不在（CLIP_LOCAL_BASE_URL=18399；POST 触发一次 encode）：`local:{reachable:false, error:"error sending request for url (http://127.0.0.1:18399/health)"}`，`last_fallback:{at:"2026-10-06T07:50:58.894123+00:00", elapsed_ms:0, reason:"local CLIP request failed: error sending request for url (http://127.0.0.1:18399/encode/image)"}`
- 存量对账（冒烟后）：`style_images` points_count=2、`fashion_knowledge` points_count=7，均未变；新集合 82。

**遗留项**
- Hybrid fallback 到 DashScope 时检索的是只读存量 `style_images`（1024）；全新环境若无该集合，fallback 检索降级为空结果（不报错），待 DashScope 侧专用集合决策后再补（属 T-027 最终形态决策范围）。
- 健康探活为按需触发（GET /health 时），无后台周期探活/告警推送——当前规模足够，需要时再加。
- DashScope recall 对照（T-026 遗留）仍等 API key。

---

### T-028: ComfyUI 服务部署 + 图片生成 Skill

```
标题: [T-028] ComfyUI 服务部署 + 图片生成 Skill（文生图）
负责人: backend
优先级: P2（新能力，价值高但依赖 GPU）
预估: 4 人周（含 GPU/模型准备）
能否并行: 是（独立于 CLIP，但共享 GPU 资源需协调）
依赖的已完成模块: T-002 docker-compose、T-011 Skill 引擎、T-017 Casbin、T-002 MinIO
依赖: 无 Phase 2 前序卡（T-029 依赖本卡）
前置条件:
  - GPU 硬件到位 + 模型选型确认（决策点 6）
交付物:
  - ComfyUI 服务容器（GPU），docker-compose 新增 comfyui 服务（可选 profile，无 GPU 环境可禁用）
  - 基础 workflow：文生图 text-to-image（SDXL / SD1.5 / 企业 LoRA）
  - 图片生成 Skill（style-image-gen）：新 SKILL.md + 工具 generate_image(prompt, params)
  - 生成 API：POST /internal/images/generate → ComfyUI → 结果存 MinIO → 返回 URL
  - 权限：生成工具纳入 Casbin（admin/designer 可用，viewer 403）
  - 异步任务队列（长生成任务不阻塞 SSE 对话），完成/失败通知
验收标准:
  - 本地 ComfyUI 可生成图片，结果存 MinIO 且可访问
  - 对话中触发生成 Skill → 返回生成图片（URL/预览）
  - viewer 触发生成 → 403；admin/designer 可生成
  - 生成过程不阻塞对话流（异步 + 进度/完成通知）
```

---

### T-029: 图片生成前端 UI + 生成闭环

```
标题: [T-029] 图片生成前端 UI + 生成闭环（生成→入库→以图搜图）
负责人: frontend + backend
优先级: P2（依赖 T-028）
预估: 2 人周
能否并行: 否（依赖 T-028 生成 API）
依赖的已完成模块: T-007 前端、T-018 项目归档、T-019 以图搜图、T-028 生成 API
依赖: T-028
前置条件:
  - T-028 生成 API 可用
交付物:
  - 前端生成 UI：输入 prompt → 生成 → 预览 → 保存
  - 生成图加入会话消息（可点开大图）
  - 生成图可选入库（作为 style image，纳入以图搜图图库，形成"生成→入库→搜图"闭环）
  - 生成图可归入项目、随会话归档
验收标准:
  - 用户在对话中/独立入口生成图片并预览
  - 生成图入库后可通过以图搜图检索到
  - 生成图可加入项目、随会话归档
  - 权限隔离生效（跨部门不可见）
```

---

## 依赖关系图

```
Phase 1B（已完成：T-004/005/006/007/011/012/013/014/015/016/017/018/019 + 迁移 001~016）

遗留修复（互不依赖，可并行）────────────────────────────
  T-020 ──── (org_id,username 唯一)
  T-021 ──── (JWT → Cookie)
  T-022 ──── (SSE 超时)
  T-023 ──── (前端 lint/test)

能力深化 ─────────────────────────────────────────────
  T-024 ──── (真实数据集)          ← T-012
  T-025 ──── (企业 MCP 规范)       ← T-016
  T-026 ──── (本地 CLIP 评估)      ← T-019 + GPU
      └──> T-027 (混合 CLIP 落地)  ← T-026 结论

新能力 ───────────────────────────────────────────────
  T-028 ──── (ComfyUI + 生成 Skill) ← T-002/T-011/T-017 + GPU
      └──> T-029 (生成 UI + 闭环)   ← T-028 + T-018/T-019
```

---

## 派发顺序（4 批次，8~12 周）

> 优先级原则：**先交付价值高 + 先还影响正确性/安全的债 + 尽早启动外部协作周期长的项**；GPU/数据授权/企业联调是三条外部等待线，越早排队越好。

### 第 1 批（Week 1-2，可全并行）— 遗留修复，快赢
- **T-020** 用户名唯一性 — P0 数据完整性债，最小工作量，最先做
- **T-021** JWT → Cookie — 安全债，但先拍板"双轨"方案再动工
- **T-022** SSE 超时 — 健壮性，独立无依赖
- **T-023** 前端 lint/test — 工程化债，可并行

### 第 2 批（Week 1-3，可并行）— 尽早排队外部等待线
- **T-024** 真实数据集 — 需数据授权谈判，尽早启动（授权等待不占开发人周）
- **T-025** 企业 MCP 规范 — 需企业技术方协作，尽早启动
- **T-026** 本地 CLIP 评估 — 需 GPU 到位，尽早排队 POC

### 第 3 批（Week 3-6）— 依决策落地的深化项
- **T-027** 混合 CLIP 落地 — 依赖 T-026 结论

### 第 4 批（Week 4-10）— 新能力
- **T-028** ComfyUI + 生成 Skill — 依赖 GPU 到位，可与 T-027 并行（共享 GPU 需协调）
- **T-029** 生成 UI + 闭环 — 依赖 T-028

---

## 验收总览

| T卡 | 功能 | 验收方式 | 优先级 |
|-----|------|----------|--------|
| T-020 | 用户名唯一性 | 同 org 重复 → 409，跨 org 同名 → 201 | P0 |
| T-021 | JWT → Cookie | 登录带 HttpOnly Cookie，JS 读不到，Bearer 回归通过 | P1 |
| T-022 | SSE 超时 | 长流式回复不断连，超时可配置 | P1 |
| T-023 | 前端 lint/test | npm run lint/test/typecheck 全绿 | P2 |
| T-024 | 真实数据集 | 真实数据可查询、隔离生效、幂等导入 | P1 |
| T-025 | 企业 MCP 规范 | 企业 MCP Server 过 conformance + 真实联调 | P1 |
| T-026 | 本地 CLIP 评估 | 本地服务出向量 + 基准报告 + 决策 ADR | P1 |
| T-027 | 混合 CLIP | 本地优先 + API fallback，重索引后检索正常 | P2 |
| T-028 | ComfyUI 生成 | 本地生成图存 MinIO，viewer 403，异步不阻塞 | P2 |
| T-029 | 生成闭环 | 生成图可入库、可搜图、可归档 | P2 |

---

## 优先级排序理由

1. **T-020（P0）**：多租户下用户名唯一性是数据完整性问题，跨 org 用户名碰撞会导致登录/归属歧义，且迁移工作量极小，是"先堵漏"的最高性价比项。
2. **T-021 / T-022 / T-023（P1/P2）**：三项都是 Phase 1B 门禁明示的遗留债，风险低、互不依赖，可与 Phase 2 主线并行消化，不占关键路径。
3. **T-024 / T-025 / T-026（P1，尽早启动）**：这三项各自绑定一条**外部等待线**——数据授权、企业技术协作、GPU 硬件到位。它们的开发量都不大，但等待周期长，越早启动越能避免成为关键路径瓶颈。
4. **T-027（P2）**：是 T-026 的落地，必须等评估结论，不宜提前投入。
5. **T-028 / T-029（P2，最后）**：图片生成是全新的高价值能力，但依赖 GPU 到位 + 模型选型，且工作量大，放最后；T-029 又依赖 T-028，串行收尾。

---

## 需要确认的设计决策（待确认项）

1. **GPU 硬件（T-026/T-028 共同前置）**：本地 GPU 是否已到位？型号/显存？——决定本地 CLIP 用 CUDA 还是 CPU/ONNX 兜底，以及 ComfyUI 模型规模（SDXL 约需 10GB 显存，SD1.5 约 4GB）。
2. **CLIP 维度迁移（T-026/T-027）**：DashScope `multimodal-embedding-v1` 输出 **1024 维**，本地 CLIP ViT-B/32 为 **512 维**、ViT-L/14 为 768 维。是否接受**重索引既有 style_images**，还是固定统一维度？另注意：现代码 `CLIP_VECTOR_DIM` 默认 512 与 DashScope 实际 1024 不一致，生产需在 .env 显式配置，属潜在配置坑（T-027 一并修正）。
3. **JWT 迁移契约（T-021）**：是否**双轨并存**（Web httpOnly Cookie + API 消费者 Bearer token）？生产是否 HTTPS（Secure cookie 前提）？
4. **真实数据集来源与授权（T-024）**：开源数据集 / 商业图库 / 企业自有？是否保留 mock 数据作为 fallback/演示？
5. **企业 MCP 协作（T-025）**：企业技术联系人/联调端点是否已定？参考实现用什么语言？认证方式（token + 租户绑定）？
6. **ComfyUI 模型选型与闭环（T-028/T-029）**：SDXL vs SD1.5 vs 企业 LoRA？生成图是否纳入以图搜图图库（形成"生成→入库→搜图"闭环）？
7. **以图搜图最终形态（T-027）**：是否最终**下线 DashScope 以图搜图（纯本地，数据不出公网）**，还是长期保持"本地 + API fallback"混合？
