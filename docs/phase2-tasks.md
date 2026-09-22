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
