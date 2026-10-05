# T-026 本地 CLIP 部署评估 — POC 第一阶段报告

日期：2026-10-05 · 负责人：backend（实施中断后由 orchestrator 补齐基准与报告）· 状态：POC 完成，DashScope 对照外部阻塞

## 1. 环境

| 项 | 值 |
|---|---|
| 机器 | MacBook Air, Apple M4, 16GB 统一内存 |
| OS | macOS 27.0.1（Darwin 27.0.0 arm64） |
| Python | 3.14.7（项目根 `.venv`，.gitignore 覆盖） |
| torch | 2.14.1（MPS built=true, available=true） |
| transformers | 5.18.0 |
| 其他 | torchvision 0.29.1, pillow 12.3.0, fastapi 0.142.2, uvicorn 0.54.0 |
| 模型 | `openai/clip-vit-base-patch32`（HF 缓存离线加载，load 0.373s） |
| 数据集 | 82 图 + 82 annotations（`data/` gitignored，本地在） |
| 基准参数 | 每设备 50 次延迟采样，预热 5 次，batch=16 吞吐 |

环境重建：backend agent 断连后 `.venv` 依赖缺失，由 orchestrator 补装（命令见 §7）。`.venv` 与 `data/poc_clip_bench.json` 均未入库。

## 2. 延迟与吞吐基准

| 指标 (ms) | CPU | MPS | MPS 相对 |
|---|---|---|---|
| 单张 图·模型 P50 | 46.33 | 39.21 | **1.18x 快** |
| 单张 图·模型 P95 | 67.08 | 70.72 | ≈持平 |
| 单张 图·模型 P99 | 125.83 | 79.57 | 1.58x 快 |
| 单张 图·端到端 P50 | 54.15 | 43.52 | 1.24x 快 |
| 单张 图·端到端 P95 | 62.34 | 64.59 | ≈持平 |
| 单张 图·端到端 P99 | 63.80 | 70.91 | 略慢 |
| 文本 P50 | 19.03 | 19.11 | ≈持平 |
| 文本 P95 | 23.04 | 39.50 | 略慢 |
| 文本 P99 | 24.17 | **111.83** | **劣化 4.6x** |
| 整批吞吐 (82 图, batch16) | **42.99 img/s** | 32.16 img/s | **0.75x 慢** |

一致性校验：CPU/MPS 双路 image L2 norm 9.618821 vs 9.618805（差 1.6e-5），dim=512 双路确认，数值一致。

**关键发现**
1. M4 MPS 对单张 image 推理仅 ~18% 提升，P95 起持平——kernel dispatch + 同步开销吃掉收益。
2. 批量吞吐 CPU 反而快 25%（43 vs 32 img/s）——batch=16 规模下 MPS 不划算。
3. 文本编码 MPS 尾延迟显著劣化（P99 111.8 vs 24.2ms）。
4. 当前规模（82 图全量重嵌入 CPU ~1.9s）设备选择不构成瓶颈；检索单查询端到端 43~54ms 可接受。

## 3. 可运行服务 POC

`scripts/clip_poc_server.py`（FastAPI，端口 8399），请求/响应契约对齐 `api-core/src/images/clip.rs` Generic 模式，为 T-027 接线预留。

curl 实测（真实证据）：

```
GET  /health → {"status":"ok","model":"openai/clip-vit-base-patch32",
                "device":"mps","dim":512,"degraded":false}
POST /encode/image (octet-stream, 真实 jpg)
     → dim=512 device=mps norm≈9.90 elapsed_ms=23.3   len(embedding)=512
POST /encode/text {"text":"米白色羊绒针织衫，圆领，柔软亲肤"}
     → dim=512 device=mps norm≈11.19 elapsed_ms=403.1  len(embedding)=512
       （首请求，含 MPS 同步开销；稳态文本 ~19ms，见基准表）
```

## 4. 质量基线

- T-024 本地评测：ViT-B/32 CPU 在同一 82 图集 Top-1 **40.24%**（`docs/clip-eval-report.md`）。
- **DashScope recall 对比：外部阻塞**——`.env` 无 `DASHSCOPE_API_KEY`。key 到位后用同一评测脚本跑对照即可，无额外工作。

## 5. 维度对齐（本地 512 vs DashScope 1024）

| 方案 | 成本/风险 | 判断 |
|---|---|---|
| A. 重索引 | provider 切换时全部 embedding 重算；本机 82 图 ≈2s，千级图分钟级；生产需停机窗口 | 可接受但只该发生一次 |
| B. 双集合共存 | 各 collection 元数据记录 dim+model，查询按当前 provider 选集合；调用方多一层选择逻辑 | **推荐** |
| C. 统一维度（PCA/随机投影） | 质量损失不可控，无评测数据验证 recall 影响 | 不推荐 |

推荐 **B**：迁移成本只在 provider 切换时发生一次，且规模小到运维复杂度可忽略。

## 6. ADR 草案（T-027 输入）

1. **本机 M4/16GB 本地 CLIP 可行**：单查询端到端 43~54ms，服务可跑，契约与 Generic 模式兼容。
2. **本地服务用 CPU device**：MPS 无吞吐收益、文本尾延迟劣化，不值得为 MPS 写特殊路径（`--device auto` 保留降级逻辑即可）。
3. **混合 fallback 值得做**：DashScope ↔ 本地互备，本地兜底零外部依赖、故障隔离好；维度对齐按 §5 方案 B（双集合）。
4. **GPU 判定（决策点 1 结论）**：本机无独立 GPU。ViT-B/32 当前规模无需独立 GPU；若未来上 ViT-L/14 或生产级批量重索引，fp16 权重 ~1.7GB + 推理峰值显存 ~4-6GB → 最低建议 **8GB VRAM GPU**（T4/A10 级）。

## 7. 复现命令

```bash
# 环境（已完成，重建时执行）
export HTTPS_PROXY=http://127.0.0.1:7897
python3 -m venv .venv && .venv/bin/python -m pip install \
  torch torchvision transformers pillow fastapi uvicorn

# 基准 → data/poc_clip_bench.json + stdout 摘要表
HF_HUB_OFFLINE=1 ./.venv/bin/python scripts/clip_poc_bench.py

# 本地服务
HF_HUB_OFFLINE=1 ./.venv/bin/python scripts/clip_poc_server.py \
  --host 127.0.0.1 --port 8399 --device auto
```

原始数据：`data/poc_clip_bench.json`（gitignored，本机保留）。

## 8. 未闭合项

- DashScope recall 对照——等 `DASHSCOPE_API_KEY`（外部阻塞）。
- 本地服务生产化接线（api-core ClipClient Generic ↔ localhost）——留 T-027。
- T-026 卡片其余项（决策记录正式化、kanban 更新）随本报告落 docs。
