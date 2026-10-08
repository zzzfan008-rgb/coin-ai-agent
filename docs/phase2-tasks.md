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

写保护的实际范围（F1 更正，2026-10-06 用户拍板 A）：**保证的是「重索引 pipeline 及其 fallback 路径不可写存量集合」**——`QdrantStore::new_read_only` 绑定的 store 在任何 upsert/delete 前即报错、不发 HTTP；`PROTECTED_LEGACY_COLLECTIONS = [fashion_knowledge, style_images]` 白名单在 pipeline 入口断言（`reindex.rs:90`、`reindex_clip.rs:87`）。**边界（如实记录）**：`QdrantStore` 的 `ensure_collection`/`upsert_raw_points`/`delete_by_document` 本身不查白名单；缺省 DashScope/Generic 模式的写路由按设计仍绑定存量 `style_images`（`mod.rs` 路由表；style 图索引即写入该集合，T-027 之前的既定行为，本轮真机对账存量 2 点无恙）。存储层硬锁与全 provider 迁移见「增量 3」卡片。检索按**实际向量维度**路由（`route_for_dim`），512 向量在结构上不可能打到 1024 集合；无匹配维度 → fail-closed 报错。

**3. 重索引 pipeline**

- 库逻辑 `images::reindex.rs`（可 hermetic 测试）＋薄封装 `examples/reindex_clip.rs`（仿 clip_smoke，不进 cargo test）。
- 用法（api-core/ 下）：
  - `cargo run --example reindex_clip`（默认目录 ../data/images、batch=16）
  - `cargo run --example reindex_clip -- ../data/images 16`（位置参数：目录、批大小）
- 工具安全：CLIP_PROVIDER 未设置时强制 local；dashscope/generic 模式直接拒绝（exit 2）；目标集合名由 model+dim 派生，命中保护名单即中止；只实例化绑定新集合的一个 QdrantStore。
- 幂等：点 ID = `stable_point_id(filename)` = UUID v5（固定命名空间 `REINDEX_NAMESPACE`，改动命名空间即破坏幂等契约，注释已标注），同文件名任何机器/次数都解析到同一 ID，upsert 替换而非追加。
- provider 契约：每个 embedding 必须是 provider=local，否则整批中止（防止经未验证 provider 落数）；中止前已 flush 的批次为合法本地向量，stable_point_id 幂等可重跑恢复（F4）。
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
- **F1 处置（2026-10-06，用户拍板 A→C→B 序列）**：A = 保护范围措辞更正（见上方「写保护的实际范围」，已完成）；C = 全 provider 迁 dim+model 命名集合 + 白名单收窄（增量 3 卡片）；B = `QdrantStore` 写入口硬锁，C 验证通过后执行。hybrid 生产启用仍暂缓（缺省 provider 未开 hybrid），C 落地后随「DashScope 侧专用集合」遗留项一并放开评估。

**reviewer 修复轮记录（2026-10-06，ce24233 门禁 finding 修复；F1 需用户裁决、F7 流程记录均不在本轮范围）**

- F2：ensure_collection 元数据改为从实际生效模式推导——新增 `ClipClient::collection_identity()`（api-core/src/images/clip.rs:332：DashScope→dashscope/multimodal-embedding-v1，Generic→generic/客户端实际 model，Local/Hybrid→local/客户端实际 model），ensure_collection 调用点 api-core/src/images/mod.rs:177 不再硬编码 `"provider":"local"`；新增 hermetic 测试 api-core/src/images/mod.rs:540（GET 404→PUT）断言 DashScope 模式写入请求体 metadata.provider="dashscope"、model="multimodal-embedding-v1"、vector_dim=1024。
- F3：探活走独立短超时——新增 `PROBE_TIMEOUT=1s`（api-core/src/images/clip.rs:175），probe_local_health 的 GET /health 改用该超时（api-core/src/images/clip.rs:376），encode 路径 local_timeout 语义不变；新增测试 api-core/src/images/clip.rs:1296（/health 延迟 3s，local_timeout 保持 5s，断言探活在 2.5s 内返回 reachable=false）。
- F5：健康面模型字段按来源改名（api-core/src/api/handlers/mod.rs）——顶层 clip.model → `collection_model`（handlers/mod.rs:153，客户端实际模型=集合命名来源），local.model → `local_model_reported`（handlers/mod.rs:158，本地服务 /health 自报值），零新依赖；集成测试 handlers_chat.rs 两态断言已适配并同时钉住两个字段。
- F6：消除 LAST_FALLBACK 跨测试竞态——新增 dev-dependency serial_test 3.5.0（Cargo.toml；cargo fetch 仅新增 serial_test + serial_test_derive 两包），clip.rs:794 `use serial_test::serial`；5 个 hybrid fallback 测试（clip.rs:1022/1037/1059/1081/1223）与记录器测试（clip.rs:1327）全部 `#[serial]` 同键串行化，record→read 不会再被并发测试覆写；`cargo test --lib images::clip` 连跑 3 次均 29 passed 0 failed。
- F4：reindex「整批中止」表述补精确——run_reindex 文档注释注明「中止前已 flush 的批次为合法本地向量，stable_point_id 幂等可重跑恢复」（api-core/src/images/reindex.rs:77-80），provider 契约内联注释同注（reindex.rs:121-125）；本记录「3. 重索引 pipeline」provider 契约行已同步补注。

修复轮验证：全量 cargo test = lib 95 + handlers_chat 6 + handlers_mcp 2 + handlers_skills 3，合计 **106 passed 0 failed**（增量 2 原 104，+F2/F3 两条新测试）；cargo fmt --check 干净；cargo build --examples（clip_smoke + reindex_clip）通过；存量 4 个编译 warning 与本轮改动无关。

**7. 增量 3（T-027 收尾）：全 provider 迁移专用集合（C）＋ 存量保护硬锁（B）**——2026-10-06 用户拍板 A→C→B 序列，本卡实施 C，B 在 C 验证通过后作为收尾步骤。

**C 阶段（本增量）**：
1. **路由表迁移**：DashScope/Generic 写路由从 `(dim, "style_images", writable=true)` 迁至 `(dim, style_collection_name(provider, model, dim), true)`——复用增量 2 的 `style_collection_name` + 修复轮的 `collection_identity()`（F2 已保证元数据真实来源）。迁移后四种 provider 的写集合全部为 dim+model 命名，元数据与向量真实来源一致。
2. **存量点位平移**：平移脚本（examples/ 风格，仿 `reindex_clip.rs`）把存量 `style_images` 的 1024 维点位**纯复制**到 DashScope 专用新集合（Qdrant scroll+upsert，**不需要 DashScope key**）；幂等（同点 ID 覆盖）；先建后验再停用。开发环境存量仅 2 点；真实部署同脚本执行。
3. **过渡期检索行为**：新集合存在且非空 → 用新集合；否则回退只读存量 `style_images`（fail-safe，保持 hybrid fallback 的 1024 路由语义不变）。存量集合在删除决策前保持只读引用。
4. **fashion_knowledge 迁移（用户拍板 2026-10-06 追加，豁免建议否决）**：1536 维文档 RAG 主链路一并迁移——RAG 写/检索路由迁到 dim+model 命名集合（provider/model 从 rag 实际 embedding 配置与代码推导，先核实 1536 向量的真实来源；已知 RagConfig embedding_dim 默认 1024 与实际 1536 不一致为既有问题，迁移时一并核实 .env/配置实际值并如实记录）；存量 7 点平移（同条 2 姿势，纯 Qdrant 复制，不需要 embedding key）；过渡期 fail-safe 同条 3（新集合存在且非空→用新，否则回退只读存量）；B 阶段硬锁对 RAG ingestion 同样生效，无豁免名单。
5. **白名单收窄**：C 完成后 `PROTECTED_LEGACY_COLLECTIONS` 语义更新——新命名集合不可能命中白名单（已有测试 `generated_collection_names_are_never_protected_legacy` 钉住），白名单只剩「只读存量 `style_images`/`fashion_knowledge`（删除决策前的过渡引用）」，无豁免项。

**B 阶段（C 验证通过后收尾，可同 PR 或独立小 PR）**：
6. `QdrantStore` 三个写入口（`ensure_collection`/`upsert_raw_points`/`delete_by_document`）加 `is_protected_legacy` 硬校验，命中即报错不发 HTTP；此时缺省模式写路由与 RAG ingestion 均已指向新集合，不再触发；fashion_knowledge 一并迁移（用户拍板，无豁免名单）。

**验收**：
- C：四 provider 各自 ensure 目标为 dim+model 命名集合；平移后新集合 points_count 对账（= 存量点数）；检索 hermetic 回归（新集合优先/存量回退两态）；真机冒烟：generic 模式 style 图索引写入新集合、存量 `style_images` 点数不变。
- B：hermetic 测试断言对保护名的三写入口全部报错且零 HTTP；缺省模式全链路测试绿。
- 全程除平移脚本显式目标外，存量集合零写入。

**依赖/遗留**：DashScope key（recall 对比校验用，不阻塞平移与 B）；旧 `style_images` 删除决策（另定，本卡不删）；F7 流程——backend 后续任务须先跑 impact 分析（AGENTS.md MUST；修复轮 2a20a11 已补跑）。

#### T-027 增量 3 · C 阶段实施记录（2026-10-06）

范围：卡上 C 阶段 1-5 条，四 provider 写集合全部迁到 dim+model 命名集合 + 存量点平移 + 过渡期 fail-safe 检索 + fashion_knowledge 豁免核实 + 白名单语义更新。**B 阶段（QdrantStore 写入口硬锁）未实施**，留 C 验收后另派。

**0. Impact 分析（F7，AGENTS.md MUST；动手前执行）**

先 `node .gitnexus/run.cjs analyze --index-only .` 把落后 2 commit 的索引更新（3,566 nodes / 7,560 edges / 306 flows）。如实记录 analyze 的 truncation：450 个候选入口中 250 个未入排名、44 个流程在 maxProcesses 丢弃——流程覆盖不完整，下述结论仅覆盖被索引符号。随后 upstream impact：

| 目标符号 | risk | 结果与处置 |
|---|---|---|
| `ImageSearchService::from_env`（mod.rs:168 起表构建） | UNKNOWN / 0 callers | 索引不可达；按规则文本搜索确认唯一生产调用 lib.rs:345。签名不变 |
| `search_similar`（mod.rs:349） | HIGH | 3 直接（mod.rs 测试）+ 1 dropped call site → 即 handler handlers/mod.rs:1522（find_similar_images）。保留方法签名与既有单路由行为，新增逻辑只在双路由 dim 生效 |
| `route_for_dim`（mod.rs:271） | CRITICAL | 2 直接（search_similar 已改走 resolve_read_route、publish_raw_point）+ 6 测试流程。写拒绝/未知 dim fail-closed 语义逐字保留，两态新测试钉住 |
| `style_collection_name`（mod.rs:73）、`ensure_collection`（mod.rs:215） | LOW | 复用，不改签名 |

HIGH/CRITICAL 不豁免；风险通过「签名不变 + 新增 17 个 hermetic 测试」收敛。

**1. 路由表迁移（卡条 1）**

新增纯函数 `route_specs(mode, model, dim) -> Vec<(dim, collection, writable)>`（mod.rs:138），`from_env` 在 mod.rs:184 调用；拆成纯函数是为了不碰进程 env 就能测表（env 在 cargo test 并行下有竞态）。迁移后各模式表：

| 模式 | 写集合（writable） | 只读 fallback |
|---|---|---|
| Local | style_images_local_clipvitb32_512 | — |
| DashScope | style_images_dashscope_mmembedv1_1024（mod.rs:146-150，dim 来自 resolve_vector_dim=1024） | 1024→style_images（mod.rs:151-153） |
| Generic（默认 dim=512） | style_images_generic_clipvitb32_512（mod.rs:156-157） | **不绑** legacy：512 向量在结构上无法检索 1024 集合 |
| Generic（显式 CLIP_VECTOR_DIM=1024） | style_images_generic_clipvitb32_1024 | 1024→style_images（mod.rs:158-160） |
| Hybrid | style_images_local_clipvitb32_512（不变） | 1024→style_images（mod.rs:141-144）——**Hybrid 的 1024 fallback 路由过渡期仍指存量 style_images** |

元数据来源沿用修复轮 F2 的 `ClipClient::collection_identity()`，ensure 路径（mod.rs:215-260）对新集合 stamped 的 provider/model 与实际模式一致，未硬编码。

**2. 存量点位平移（卡条 2）**

库逻辑 `migrate_points(source, target_store, target, page_size, batch_size)`（api-core/src/images/migrate.rs:55）+ CLI 封装 `api-core/examples/migrate_style_images.rs`（仿 reindex_clip 风格）。

- 纯 Qdrant-to-Qdrant：源 `scroll_points`（POST points/scroll，with_vector+with_payload）→ 目标 upsert，不调用任何 embedding provider，**不需要 DashScope key**。
- 源 store 以 `new_read_only` 绑定（example:78），对存量集合只有读；新增读方法 `QdrantStore::points_count`（qdrant.rs:206）、`scroll_points`（qdrant.rs:229）；复制写走新方法 `upsert_scrolled_points`（qdrant.rs:277），点结构 `ScrolledPoint`（qdrant.rs:56，id 保持原 JSON：UUID 或无符号整数都原样回写）。
- 双重配置保护：目标命中 `is_protected_legacy` 即 bail（migrate.rs:62-64）；目标 store 绑定集合与声明 target 不符即 bail（migrate.rs:65-70）——存量集合只能当源、不可能当写目标。
- 幂等：同 id upsert 为替换；真机复跑计数不变（见冒烟①）。先建后验：example 先 `ensure_collection(1024, metadata provider=dashscope/model=multimodal-embedding-v1)`（example:83-100）再复制，结束按 scrolled==copied 且目标点数 >= 源点数对账。

**3. 过渡期检索行为（卡条 3）**

实现放在路由解析层：`resolve_read_route(dim)`（mod.rs:306），`search_similar`（mod.rs:349-366）编码后调用它：

- dim 只有单条路由（Local 512、Generic 512、Hybrid 的 1024 legacy）→ 直接用，零额外 HTTP；
- dim 有双路由（可写命名集合 + 只读 legacy）→ GET 命名集合 points_count：`Some(>0)` 用命名集合；缺失（404→None）或 0 → 回退只读 style_images，fail-safe；
- 无任何路由 → fail-closed，报错文本与 route_for_dim 逐字一致（mod.rs:308-315）。
- 另加 `tracing::debug!` 记录最终检索集合（mod.rs:357-361）。写路径不受影响：publish_raw_point（mod.rs:336）仍走 route_for_dim 只选 writable。

**4. fashion_knowledge 豁免核实（卡条 4）——结论：豁免，不迁**

依据（grep 核实）：
- 集合名来自 knowledge config：`RagConfig::default` 的 `QDRANT_COLLECTION` 缺省 fashion_knowledge（rag/mod.rs:102-103）；`from_app_config` 直接硬绑 fashion_knowledge（rag/mod.rs:140）。
- 它是**文档 RAG 主链路**：上传→index_document（rag/indexer.rs:34）parse→chunk→文本 embedding（rag/embedding.rs，text-embedding 系）→`upsert_chunks`，状态回写 knowledge_documents；检索走 `RagRetriever::retrieve`（rag/mod.rs:237-260）。
- 与 style 图链路（ImageSearchService，图片 multimodal 向量，集合名由 CLIP provider/model/dim 派生）职责、载体、维度均不同。真机实测该集合为 **1536 维**（服务启动日志：`fashion_knowledge vector-dim drift: collection has 1536, expected 1024`——RagConfig 默认 1024 与真实集合不一致是既有问题，本轮不修、仅记录），points_count=7。
- 处置：豁免迁移，白名单保留其保护项；不删不改建。

**5. 白名单语义更新（卡条 5）**

只动注释/文档，不动任何写入口逻辑：
- `PROTECTED_LEGACY_COLLECTIONS`（qdrant.rs:38，值不变）的文档（qdrant.rs:19-37）改为明确：白名单只剩两类——(a) 过渡期被只读引用的 style_images（+ 平移脚本的源），(b) RAG 豁免项 fashion_knowledge；并注明 B 阶段硬锁尚未实施。
- 新命名集合结构上不可能命中白名单：`style_collection_name` 输出以 `style_images_` 前缀 + provider/model/dim token，白名单两项为无前缀裸名；既有测试 `generated_collection_names_are_never_protected_legacy`（mod.rs:444）已钉死。
- **未动 QdrantStore 三个写入口**：ensure_collection（qdrant.rs:135）、upsert_raw_points（qdrant.rs:364）、delete_by_document（qdrant.rs:466）均不加 is_protected_legacy 校验——那是 B 阶段。

**6. 测试（全 hermetic：wiremock 进程内 mock；失败路径死端口 18399/16333，零真实 :6333/:8399）**

新增 17 个：
- images/mod.rs +8：纯表 4（dashscope 命名+legacy、generic 512 仅命名、generic 1024 带 fallback、local/hybrid 表不变）＋过渡两态 4（非空优先、缺失回退、空集合回退、1024 写指向可写命名集合；见 mod.rs:686-928）。
- rag/qdrant.rs +4：scroll 请求体+pre-1.19 next_offset 解析、1.19 next_page_offset+命名字向量、points_count 两态、迁移 upsert 保 id+只读拒绝（qdrant.rs:725-854）。
- images/migrate.rs +5：单页全复制（断言 PUT 体 id/vector/payload 原样、源只见读请求）、双页分页+offset 透传、protected 目标拒绝（零 HTTP）、store 绑定不符拒绝、复跑幂等（migrate.rs:223-374）。

全量 `cargo test`：**lib 112 passed（增量 2 的 95 +17）、handlers_chat 6、handlers_mcp 2、handlers_skills 3，合计 123 passed 0 failed**。`cargo fmt --check` 干净。`cargo build --examples`（clip_smoke + reindex_clip + migrate_style_images）通过。clippy 本机未安装，留 CI。存量 4 个编译 warning 与本轮无关。

**7. 真机冒烟证据（2026-10-06，非 cargo test）**

冒烟后全量对账：

| 集合 | 冒烟前 | 冒烟后 |
|---|---|---|
| style_images（存量） | 2 | **2（全程未变）** |
| style_images_dashscope_mmembedv1_1024（新） | 不存在 | **2** |
| style_images_generic_clipvitb32_512（新） | 不存在 | **1** |
| style_images_local_clipvitb32_512 | 82 | 82 |
| fashion_knowledge | 7 | 7 |

① 平移脚本对真 :6333：`Created Qdrant collection collection=style_images_dashscope_mmembedv1_1024 vector_dim=1024`；`stats: scrolled=2 copied=2 batches=1 elapsed_ms=31`；`target points_count: before=Some(0) after=Some(2)`；源仍 2。独立对账：源/目标 id 集合一致（275348ec-…、cf8dc516-…），带向量 scroll 后 vector/payload 逐字节 diff 全等（VECTORS_IDENTICAL / PAYLOADS_IDENTICAL）；幂等复跑 2→2 无 Created、无重复。
② Generic 真实服务（CORE_PORT=8091，CLIP_API_ENDPOINT=本地 8399 Generic 方言）：启动即建 `Created Qdrant collection collection=style_images_generic_clipvitb32_512 vector_dim=512`；POST /api/styles/693f8879…/images 上传一张 jpg 后日志 `Image indexed in Qdrant image_id=94f02e4e-… provider="generic" vector_dim=512`；generic 集合 0→1、存量 style_images 仍 2（点 id 落位已 scroll 核实）。注：本机 .env 无 LLM key，启动以 MINIMAX_API_KEY=boot-dummy 通过 LlmClient fail-fast（LLM health FAILED，与图链路无关；聊天功能本轮不验）。
③ 检索优先路径：同图 POST /internal/images/similar → `provider_used="generic"`，命中点 image_path=uploads/style-images/…/94f02e4e-….jpg——该 id 只存在于 generic 命名集合（存量两 id 为 275348ec/cf8dc516），由数据本身证明检索走了新集合；存量仍 2。

**遗留 / 下一步**
- **B 阶段未实施**：三写入口硬锁（qdrant.rs:135/364/466）待本 C 阶段验收后另派。
- 旧 style_images 删除决策另定（本卡不删）；DashScope recall 对比仍等 key。
- 真机发现的既有不一致：RagConfig embedding_dim 默认 1024 vs fashion_knowledge 实际 1536（仅记录，未修）。

---

#### T-027 增量 3 · C 阶段追加轮：fashion_knowledge 迁移（2026-10-06）

用户拍板：**fashion_knowledge 豁免否决，一并迁移**（卡条 4 已据此改写）。本节为追加轮记录，上一轮「豁免，不迁」结论作废、保留为历史。范围仅 RAG 链路；style 链路已迁代码未动。**B 阶段仍未实施。**

**0. 事实核实：1536 向量的真实来源（2026-10-07 orchestrator 独立取证后重写；旧归因作废）**

- 存量集合 fashion_knowledge：7 点，向量逐点实测 1536 维；集合 config params size=1536，无 collection metadata。
- 现存 7 点的写入时点：点 payload 的 4 个 doc_id 与 DB knowledge_documents 中 **2026-10-02 16:14 批次**逐一对上（f371fb2e=washing-care、f041afce=fabric、d74654ac=color-palette、80e60320=tech-pack，共 7 chunks）；09-23 14:53 首批的 7 点已被该重灌覆盖，不在集合中。
- **配置证据**：api-core/.env 于同日 **16:09** 被切到 EMBEDDING_BASE_URL=https://api.apiyi.com/v1、EMBEDDING_MODEL=text-embedding-3-small、EMBEDDING_DIM=1536（文件 mtime 佐证，16:09 早于 16:14 摄入）。
- **结论（定案）**：现存 7 点最可能是 **text-embedding-3-small（原生 1536）** 向量，apiyi 端点产出。
- **旧归因作废（本节重写原因）**：2026-10-06 追加轮曾写「摄入时 RagConfig=qwen+text-embedding-v3+1024、qwen 兼容代理忽略 dimensions 强制输出 1536」——那是 09-23 首批的可能情况，但首批已被 10-02 重灌覆盖，归因对象不存在；同轮「无法排除摄入时 shell env 注入」的猜测无证据支持，一并撤下。
- pgvector 侧：knowledge_documents.chunk_embedding 列虽是 vector(1024)，实查 **0 行有值**——pgvector 路径从未真正嵌入过，1024 只是建表默认，不构成任何归因证据。
- 配置默认值演进（历史保留）：30545cb（2026-09-21 接入 DashScope）之前代码硬编码 model=deepseek-embedding、dim 1536、base=api.deepseek.com；30545cb 之后默认 dim 1024，与现存集合形成 drift（启动 ensure 报 drift 的历史现象）。
- 当前 .env 生效面：EMBEDDING_BASE_URL/MODEL/DIM 已设为 apiyi/text-embedding-3-small/1536（本节取证时实查）。

**1. 命名（同款 dim+model 约定）**

新增 `knowledge_collection_name(provider, model, dim) -> fashion_knowledge_{provider}_{model-tag}_{dim}`（rag/mod.rs:160）＋`knowledge_model_tag`（:140）：text-embedding-v1/v2/v3 → tev1/tev2/tev3，deepseek-embedding → dsembed，未知模型 ASCII sanitize。

provider/model 取真实来源：新增 `EmbeddingService::identity()`（rag/embedding.rs:114），provider 按配置端点 host 数据驱动推导——host 含 dashscope/qianwen/maas → dashscope；含 deepseek → deepseek；其余（含 localhost 代理、apiyi）→ generic；model 为实际发送的模型名。
本环境目标集合名：**fashion_knowledge_generic_textembedding3small_1536**（2026-10-07 P1-A 改道轮定案，apiyi=text-embedding-3-small 权威；下节 P1-A 记录含推导中间值证据）。首轮（2026-10-06）曾按当时误判的 qwen 归因产出 `fashion_knowledge_dashscope_tev3_1536` 与 `fashion_knowledge_generic_tev3_1536`——**均为误名孤儿集合，勿使用**，不删不写（删除决策另定）。

**2. RAG 写/检索路由迁移（rag/mod.rs）**

RagRetriever 改为双 store（结构见 :203-222）：
- `named_store`：writable，绑定 identity 派生的 dim+model 命名集合；
- `legacy_store`：`new_read_only` 绑定 config.qdrant_collection（fashion_knowledge），仅检索 fallback。
方法路由（方法签名全部不变）：
- `new`（:225）：按 embedding identity 构造命名集合；
- `ensure_collection`（:258）：named 集合带 metadata（vector_dim/model/provider/created_by/task）ensure；legacy 走 verify_collection_dim 只验证不创建（missing → debug 日志，不报错）；
- `read_store`（:288）：过渡期 fail-safe——named points_count `Some(>0)` 用 named；missing(None)/0 回退 legacy；
- 写入：`upsert_chunks`（:296）→ named；删除：`delete_document_vectors`（:317，及 delete_document_points）→ named；并注明：只存在于 legacy 的点不被删除路径覆盖，须先跑平移（与 style 链路一致，legacy 保持只读）；
- 检索：`search`（:302）→ read_store；`retrieve`（:325，agent/engine.rs:186 调用）同样自动获得新集合优先语义。pgvector 职责未动。

**3. 存量 7 点平移（复用 migrate.rs）**

上一轮 images/migrate.rs 的 `migrate_points`（库逻辑不绑定 style 语义：source/target stores + target 名校验）直接复用，无需泛化。新增 CLI 示例 `api-core/examples/migrate_knowledge.rs`（仿 migrate_style_images）：
- 源 store new_read_only（example:78 附近）；目标 writable 绑定命名集合；先 `ensure_collection(1536, metadata provider=dashscope/model=text-embedding-v3)` 建集合后复制（先建后验）；
- 目标 protected 拦截（is_protected_legacy → exit 2）、source==target 拦截；migrate_points 内部再做 protected bail 与 store 绑定一致性 bail；
- 同 id upsert 替换 → 幂等可恢复；结束按 scrolled==copied 且 target 点数 >= source 对账。

**4. 白名单语义同步（仅注释）**

`PROTECTED_LEGACY_COLLECTIONS`（qdrant.rs:38，值不变）文档更新：两个 legacy 名现在都是「过渡期只读 fallback ＋ 平移脚本源」，均永不为写目标；生成名不可能命中（knowledge 侧新增同名 never-protected 测试）。三写入口仍未加硬校验（B）。

**5. 测试（全 hermetic：wiremock 进程内 mock；零真实 :6333）**

rag/mod.rs 新增 10 个（:365 起）：
- 命名 2：knowledge_collection_name 三种 provider/model + never-protected；
- identity：host 三态（maas→dashscope、deepseek、localhost→generic）；
- 构造：new 绑定名 = identity 派生名；
- 过渡两态 3：named 非空优先（断言 GET+POST 均落 named、legacy 零接触）、missing 回退 legacy、empty(0) 回退 legacy；
- 写路由 2：upsert PUT 落 named、delete POST 落 named；
- ensure：missing 时只 PUT 创建 named 且 metadata 正确，legacy verify missing 被容忍。

全量 `cargo test`：**lib 122 passed（上一轮 112 +10）、handlers_chat 6、handlers_mcp 2、handlers_skills 3，合计 133 passed 0 failed**。`cargo fmt --check` 干净。`cargo build --examples`（clip_smoke/reindex_clip/migrate_style_images/migrate_knowledge）通过。clippy 本机未安装，留 CI。

**6. 真机冒烟证据（2026-10-06，非 cargo test）**

① 平移脚本对真 :6333：
- dashscope 命名集：`Created Qdrant collection collection=fashion_knowledge_dashscope_tev3_1536 vector_dim=1536`；`scrolled=7 copied=7`；target 0→7；源 fashion_knowledge 仍 7。
- 独立对账：源/目标 id 集合相等（7=7）；按 id 逐点比较 payload 全等、vector 全等（1536 分量差值全 0；整行 diff 的唯一差异是点顺序）。
- 幂等复跑：7→7，无 Created。
- 另按本地代理部署名（generic provider）建第二个目标集 fashion_knowledge_generic_tev3_1536：7→7，源仍 7。
② RAG 写+检索真实链路（CORE_PORT=8092；embedding 指向本地 OpenAI 兼容 mock：127.0.0.1:18401 返回常量 1536 向量；EMBEDDING_DIM=1536）：
- 上传 smoke_doc.txt（唯一标记 UNIQUE-MARKER-KNOWLEDGE-SMOKE-7Q）→ 后台索引完成日志（indexed=1，doc ready）；generic 命名集 7→8，存量 fashion_knowledge 仍 7（写路由落新集合）。
- POST /internal/knowledge/search（top_k=20）→ 结果包含 UNIQUE-MARKER 文本（grep 计数 1）：该 chunk 只存在于命名集合，由数据本身证明检索走新集合；存量仍 7。
- 注：MINIMAX_API_KEY=boot-dummy 仅为通过启动 fail-fast（LLM health 与本轮无关）；embedding mock 为 scratch 临时组件、非仓库交付物；冒烟新增的 1 个 smoke 文档 DB 行与 1 个标记点保留在环境中（与上一轮 generic style 冒烟数据同样处理），故 generic 命名集当前 8 点。

冒烟后全量对账：

| 集合 | 点数 |
|---|---|
| fashion_knowledge（存量） | 7（全程未变） |
| fashion_knowledge_dashscope_tev3_1536（新，生产命名） | 7 |
| fashion_knowledge_generic_tev3_1536（新，含 1 冒烟标记点） | 8 |
| style_images（存量） | 2 |
| style_images_dashscope_mmembedv1_1024 | 2 |
| style_images_generic_clipvitb32_512 | 1 |
| style_images_local_clipvitb32_512 | 82 |

**遗留 / 下一步**
- **B 阶段仍未实施**：三写入口硬锁（qdrant.rs ensure_collection/upsert_raw_points/delete_by_document）对 style 与 RAG ingestion 同样生效、无豁免名单——待验收后另派。
- 旧集合删除决策另定（本卡不删）；生产部署建议显式 EMBEDDING_DIM=1536（与 text-embedding-3-small 原生输出维度一致）。
- **2026-10-07 更新**：上述「生产 qwen 兼容端点」归因已被 orchestrator 取证推翻（见第 0 节重写）；本节 dashscope_tev3_1536/generic_tev3_1536 的平移与冒烟记录保留为历史，两个集合均为**误名孤儿集合，勿使用**。改道实施见下节 P1-A 记录。

---

#### T-027 增量 3 · P1-A 改道轮：fashion_knowledge → generic_textembedding3small_1536（2026-10-07）

用户拍板：**apiyi 是现行 embedding 权威**；目标集合名 = `fashion_knowledge_generic_textembedding3small_1536`。范围纪律：不动 style 链路已迁代码、不动 QdrantStore 写入口（B 未放行）、不删任何集合、不 commit 不 push。真机验证用当前 .env 实际配置，不注入覆盖任何 EMBEDDING_* 变量。

**1. 推导中间值证据（真机，api-core/.env 源入后 `cargo run --example migrate_knowledge` 无参）**

```
migrate_knowledge: embedding endpoint=https://api.apiyi.com/v1 provider=generic model=text-embedding-3-small dim=1536
migrate_knowledge: source=fashion_knowledge
migrate_knowledge: target=fashion_knowledge_generic_textembedding3small_1536 (derived from config)
migrate_knowledge: target matches service derivation (fashion_knowledge_generic_textembedding3small_1536)
```
推导链逐字：host=api.apiyi.com → identity() generic 分支 → provider=generic；model=text-embedding-3-small → knowledge_model_tag → textembedding3small；dim=1536；knowledge_collection_name 拼出目标名，与用户指定逐字一致。该链路同时被 hermetic 测试钉住（rag/mod.rs `binding_reports_production_identity_from_config`：注入 apiyi/text-embedding-3-small/1536 → binding() 四元组全断言）。

**2. 平移与幂等（READ-COPY：scroll+upsert、点 ID 不变（Uuid v5 of doc_id:i）、payload 原样、不调 embedding API、存量 fashion_knowledge 零写入）**

- 第一遍：`scrolled=7 copied=7 batches=1`；target 0→7；source 仍 7。
- 第二遍复跑：`scrolled=7 copied=7 batches=1`——无变化，幂等确认。

**3. 对账（带向量+payload scroll 逐点严格比对）**

```
legacy count: 7  new count: 7
id sets equal: True
points byte-identical (id+payload+vector): True | differing: 0
vector dims: {1536}
  00b8ee75… / 0134b5ca… -> doc_id=f371fb2e (washing-care)
  283b4db9…            -> doc_id=f041afce (fabric)
  42566db4…            -> doc_id=d74654ac (color-palette)
  58fcaa97… / b61c9572… -> doc_id=80e60320 (tech-pack)
  d2a3311a…            -> doc_id=d74654ac (color-palette)
```
存量 fashion_knowledge = 1536/7 全程零写入；新集合 = 1536/7 逐点 ID+payload+vector 与存量全等；doc_id 与第 0 节 10-02 16:14 批次逐一对上。

**4. 真服务冒烟（CORE_PORT=8092，当前 .env，无任何 EMBEDDING_* 注入）**

- P3-1 启动日志：`INFO api_core: RAG embedding binding resolved provider=generic model=text-embedding-3-small dim=1536 collection=fashion_knowledge_generic_textembedding3small_1536`
- POST /internal/knowledge/search（query=洗涤护理 cotton 晾晒，org/dept=00000000-…-0001）→ 命中「服装洗护保养指南…纯棉（Cotton）」。
- 路由证据（LOG_LEVEL=debug 复启服务）：`DEBUG api_core::rag: knowledge search route: named collection collection=fashion_knowledge_generic_textembedding3small_1536 count=7`——检索明确走新集合，未落存量。RAG fail-safe 两态语义未动（新集合非空→用新，缺/空→只读回退存量）。

**5. P2/P3 修复（与改道同轮）**

- **P2-1（rag/indexer.rs）**：upsert 批失败不再被吞——错误记入 upsert_errors + tracing::error，失败批 indexed 回退；文档最终状态经纯函数 `DocumentIndexer::final_doc_state(indexed, total_chunks, &upsert_errors)`：有 upsert 错误 → ("failed", 错误拼接)；indexed==0 → ("failed", "Some chunks failed to embed")；indexed<total_chunks（embed 部分失败但全部持久化）→ ("ready", 部分 embed 失败消息)；全成 → ("ready", None)。UPDATE bind(status, indexed, error_message)。hermetic 测试 3 个钉住三态（upsert 错败永不 ready / 全成 ready 无消息 / 部分 embed 失败 ready 带消息 / 全失败 failed）。
- **P2-2（examples 参数化）**：migrate_knowledge.rs 重写（`cargo run --example migrate_knowledge -- <SOURCE> [<TARGET>]`，TARGET 缺省=EMBEDDING_* 推导，显式 TARGET 与推导不一致时打 WARNING）；migrate_style_images.rs 同款重写（TARGET 缺省=CLIP_* 经 from_env/route_specs 推导首个 writable route）。两 example 均保留 READ-COPY 幂等语义、protected bail、source==target 拦截。**P3-2（examples panic!/位置参数）按拍板接受不改。**
- **P3-1（启动日志）**：lib.rs RagRetriever 构造后 `RAG embedding binding resolved provider=… model=… dim=… collection=…`（info）；CLIP 侧对称日志（configured → `CLIP embedding binding resolved provider=… model=… dim=… collection=…`；无 writable route → warn）。RagRetriever 新增 `binding() -> (provider, model, dim, named_collection)` 供日志/测试复用；read_store() 两分支补 tracing::debug（named collection 命中 / legacy fallback）。
- **.env.example（仓库根）**：Embedding 模板行改为 apiyi 身份（EMBEDDING_BASE_URL=https://api.apiyi.com/v1、EMBEDDING_MODEL=text-embedding-3-small、EMBEDDING_DIM=1536、集合名注释 fashion_knowledge_generic_textembedding3small_1536）；qwen maas（text-embedding-v3）转为注释备选；无任何真实 key。
- 测试钉住：rag/mod.rs `knowledge_collection_names` 断言目标名逐字；identity cases 增 ("https://api.apiyi.com/v1","generic") 与生产链路断言；never-protected 测试增 (generic,text-embedding-3-small,1536) case；`binding_reports_production_identity_from_config` 注入生产身份断言四元组。

**6. 验证汇总**

- `cargo check --examples --tests` OK；`cargo fmt --check` 干净。
- `cargo test`：**lib 127（上轮 122 + P2-1 3 个 + binding 1 个）、handlers_chat 6、handlers_mcp 2、handlers_skills 3，合计 138 passed 0 failed**（hermetic，零真实 :6333/:8399）。
- 真机：平移两遍幂等（见 §2）、对账全等（见 §3）、冒烟走新集合（见 §4）。

**7. 环境现状（冒烟后）**

| 集合 | 点数 | 备注 |
|---|---|---|
| fashion_knowledge（存量） | 7 | 全程零写入，只读 fallback |
| fashion_knowledge_generic_textembedding3small_1536 | 7 | **现行目标**，1536 |
| fashion_knowledge_dashscope_tev3_1536 | 7 | 误名孤儿，勿使用（不删不写） |
| fashion_knowledge_generic_tev3_1536 | 8 | 含 1 冒烟标记点，误名孤儿，勿使用 |
| style_images* 四集合 | 2/2/1/82 | 未动（本轮回退不动 style 链路） |

**遗留 / 下一步**
- **B 阶段仍未实施**：三写入口硬锁待验收后另派。
- 三个旧 fashion_knowledge* 集合（存量 + 两个孤儿）删除决策另定，本卡不删。
- 无未提交 commit（orchestrator 收口）。

#### T-027 增量 3 · 复审修复轮（2026-10-07，PASS-WITH-NOTES 后 4 点小轮）

复审报告：/Users/lionfan/.hermes/cache/scratch/t027_recheck_report.md（PASS-WITH-NOTES）。范围严格限 4 点，未碰 qdrant.rs / route_specs / search_similar / B 阶段 / Qdrant / DB。

1. **P2-1 恢复 source==target 拦截（修法 a，reviewer 推荐）**：migrate_knowledge.rs:87-89 与 migrate_style_images.rs:96-98 在 protected bail 之后各恢复 3 行守卫 `if source == target { anyhow::bail!(...) }`。实证：`cargo run --example migrate_knowledge -- fashion_probe fashion_probe` → `Error: refusing to migrate: source and target are the same collection 'fashion_probe'`，bail 发生在任何 Qdrant 调用之前（零网络零写入）。docs 增量 3 记录（"两 example 均保留…source==target 拦截"）随守卫恢复自动重新属实。
2. **P2-2 两 example 加 .env 加载**：migrate_knowledge.rs:38-40 与 migrate_style_images.rs:34-36 各加 `let _ = dotenvy::dotenv();`（config.rs:64 同款，dotenvy 已是依赖）。实证：未 source .env 直跑 example 打印 `effective EMBEDDING_BASE_URL=https://api.apiyi.com/v1`（migrate_knowledge.rs:70-74 新增行），说明 dotenvy 已加载 api-core/.env，推导与服务读到同一配置。
3. **P3-1 final_doc_state 调用点接线测试覆盖（修法 a）**：indexer.rs 抽纯函数 `DocumentIndexer::apply_upsert_result(indexed, batch_len, upsert_result, upsert_errors)`（:179-199）——upsert 失败时 error 入列 + indexed 按 batch_len 回退，batch 循环（:110-117）改为经该函数接线。hermetic 测试 2 个钉住接线（:303-332）：`upsert_failure_wiring_rolls_back_indexed_and_records_error`（5-3=2 回退、error 入列、与 final_doc_state 组合后 failed 永不 ready）与 `upsert_success_wiring_keeps_indexed_untouched`。未采用 b。
4. **P3-2 docs:622 历史行改事实措辞**：`生产部署建议显式 EMBEDDING_DIM=1536（与 text-embedding-3-small 原生输出维度一致）`，删除「与代理实际输出对齐」因果措辞。

验证：cargo check --examples --tests OK；cargo fmt --check 干净；cargo test → **140 passed 0 failed（lib 129 = 上轮 127 + 接线 2；chat 6；mcp 2；skills 3）**。守卫/ dotenvy 均真机实证（见上）。

#### T-027 增量 3 · B 阶段：QdrantStore 写入口 is_protected_legacy 硬锁（2026-10-08）

用户放行 B 阶段。前置（C + P1-A + 复审修复轮）收口于 b7a114f。**只改 qdrant.rs 生产代码 + 一处既有 images 测试对齐**；rag/mod.rs / images/mod.rs 生产代码未动；route_specs / search_similar 未动；Qdrant/DB 零写入。

**1. 实现**

- 新增 `QdrantStore::assert_not_protected_legacy()`（api-core/src/rag/qdrant.rs:125-138）：绑定集合名命中 `PROTECTED_LEGACY_COLLECTIONS`（fashion_knowledge / style_images）时 bail，文案 `"write refused: Qdrant collection '{name}' is a PROTECTED LEGACY collection (read-only fallback / migration source) — writes are forbidden"`，与 read-only 文案（"bound read-only"）可区分。
- 5 个变异入口在**任何 HTTP 请求之前**调用它，且 **fail-closed 顺序 = protected 检查排在空批次早退之前**（空批次写 protected 也响亮报错，静默 Ok 即门禁失效）：
  1. `ensure_collection` qdrant.rs:163（在 GET 之前——missing 时 PUT 会重建禁用目标）
  2. `upsert_chunks` qdrant.rs:349
  3. `upsert_raw_points` qdrant.rs:388
  4. `upsert_scrolled_points` qdrant.rs:305
  5. `delete_by_document` qdrant.rs:492
- 读入口保持可用（迁移源 + 过渡回退依赖）：`scroll_points` / `search` / `points_count` / `verify_collection_dim` 未加锁。无豁免名单、无 feature flag、无环境变量开关。
- scope note 更新（qdrant.rs:34-42）：删「NOT implemented yet」，改为已实施并列出 5 个入口 + 读入口保持开放的说明。

**2. hermetic 测试（qdrant.rs:893-1109，wiremock，绝不触真 :6333）**

- `protected_legacy_refuses_all_five_write_entries_before_http`：两个 protected 名 × 5 个变异入口各断言 bail 且 mock 端收到 0 个请求。
- `protected_legacy_empty_batch_still_fails`：空批次 × 3 个 upsert 入口仍 bail（钉住 fail-closed 顺序）。
- `protected_legacy_read_paths_stay_open`：protected 集合上 scroll/search/points_count/verify_collection_dim 对 mock 均成功。
- `non_protected_empty_batch_is_silent_ok`：非 protected 空批次静默 Ok 行为不变。
- 既有测试调整（唯一非 qdrant.rs 改动）：images/mod.rs `ensure_collection_metadata_provider_follows_dashscope_mode`（F2）原直连 `style_images` 路由，B 阶段硬锁下改为 `style_images_dashscope_mmembedv1_1024`（F2 的 provider/model/dim metadata 语义不变，测试意图保留）。
- 现有 `creates_collection_with_metadata_when_missing` 等非 protected 测试全部继续绿；examples 侧 refuses_protected_legacy_target 测试继续绿。

**3. 验证**

- cargo check --examples --tests OK；cargo fmt --check 干净。
- cargo test：**144 passed 0 failed（lib 133 = 129 + B 阶段 4；chat 6；mcp 2；skills 3）**。

**4. 真机冒烟（2026-10-08，当前 .env，无任何 EMBEDDING_* 注入）**

- 启动日志：`INFO api_core: RAG embedding binding resolved provider=generic model=text-embedding-3-small dim=1536 collection=fashion_knowledge_generic_textembedding3small_1536`——走 dim+model 命名集合，ensure_collection 无 protected bail（日志零 "PROTECTED LEGACY"、零 ERROR）。
- CLIP 侧 generic 模式无可写路由（warn 路径，未触发 bail）；health 探针 `clip.mode=generic`。
- POST /internal/knowledge/search（洗涤护理 cotton 晾晒）→ 命中「服装洗护保养指南…纯棉（Cotton）」。
- 结论：**存量两集合（fashion_knowledge / style_images）仍可读不可写**——读入口（scroll/search/points_count/verify_collection_dim）开放、5 个写入口在任何 HTTP 前硬拒。

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
