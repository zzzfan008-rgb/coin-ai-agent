# Coin-AI 平台 — 企业 MCP 接口规范

> **规范版本**: v1.0.0
> **生效日期**: 2026-09-23
> **适用范围**: 企业开发团队，为 Coin-AI 平台开发 MCP Server 以接入企业专有数据与工具
> **规范负责人**: Coin-AI 平台架构组
>
> 企业按此规范开发 MCP Server，平台提供 MCP Gateway 负责连接、认证与调用。

---

## 1. 概述

### 1.1 什么是企业 MCP

企业 MCP（Model Context Protocol Server）是企业自行开发的 HTTP/gRPC 服务，实现了本规范定义的 MCP 接口。接入平台后，Coin-AI Agent 在对话中可调用企业 MCP Server 暴露的工具，访问企业的面料数据库、款式图库、设计知识库等专有数据。

### 1.2 为什么需要企业 MCP

Coin-AI 平台部署在企业内网，企业拥有大量未公开的结构化数据（面料成分、款式档案、供应商资料、设计规范等）。企业 MCP 提供了安全、规范的接入通道，无需暴露数据接口，平台 Agent 按需调用，企业掌控工具实现。

### 1.3 平台 MCP 架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        Coin-AI 平台                              │
│                                                                 │
│   ┌─────────────┐      ┌─────────────────┐                     │
│   │  Agent Loop │─────▶│   MCP Gateway   │                     │
│   │  (Rust Core)│      │   (平台侧组件)   │                     │
│   └─────────────┘      └────────┬────────┘                     │
│                                  │ stdio / HTTP（SSE）           │
└──────────────────────────────────┼───────────────────────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │    Enterprise MCP Server     │
                    │    (企业自行开发托管)         │
                    │                              │
                    │  · fashion.search_images     │
                    │  · fashion.get_product_info  │
                    │  · design.get_style_reference │
                    │  · ...                       │
                    └─────────────────────────────┘
```

- **平台侧**：MCP Gateway — 负责连接管理、认证校验、工具发现、调用路由、审计记录
- **企业侧**：Enterprise MCP Server — 企业开发，部署在企业内网，暴露企业专有工具
- **通信方式**：stdio（推荐，默认）或 Streamable HTTP（含 SSE），见 §2.2

### 1.4 接入流程概览

```
1. 企业技术团队阅读本规范（docs/mcp/enterprise-mcp-spec.md）
2. 按规范开发 MCP Server，完成本地调试（§6）
3. 提交 Server 信息给平台管理员（Server 路径/命令/认证 token）
4. 平台管理员在 MCP 管理页注册 Server（§5.2）
5. 平台执行 conformance 测试验证（§5.3）
6. 测试通过后上线，Agent 对话中自动发现并调用工具
```

---

## 2. 接口规范（MCP Protocol）

本规范基于 [Model Context Protocol](https://modelcontextprotocol.io/specification) 2025-06-18 版本，定义了企业 MCP Server 必须遵循的协议行为。

### 2.1 JSON-RPC 2.0 基础

所有消息为 [JSON-RPC 2.0](https://www.jsonrpc.org/specification) 格式，UTF-8 编码。

#### 请求格式（Request）

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "fashion.search_images",
    "arguments": {
      "query": "2026 春夏新款连衣裙",
      "filters": { "category": "dress" }
    }
  }
}
```

#### 响应格式（Response）

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"images\": [{\"id\": \"IMG-001\", \"url\": \"...\", \"title\": \"...\"}]}"
      }
    ]
  }
}
```

#### 错误格式（Error）

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32602,
    "message": "Invalid params: missing required field 'query'"
  }
}
```

#### 错误码定义

| 错误码 | 含义 | 说明 |
|--------|------|------|
| -32700 | Parse error | JSON 解析失败 |
| -32600 | Invalid Request | 请求格式非法 |
| -32601 | Method not found | 方法未实现 |
| -32602 | Invalid params | 参数校验失败 |
| -32603 | Internal error | 服务器内部错误 |
| -32000 | Auth failed | 认证失败（平台自定义） |
| -32001 | Permission denied | 无权调用该工具（平台自定义） |
| -32002 | Tool execution failed | 工具执行异常（见 §3.3 返回格式说明） |

### 2.2 Transport 传输层

#### 推荐：stdio 模式（默认）

Server 以子进程模式启动，平台通过 stdin/stdout 进行 JSON-RPC 通信。

- 无需开放额外网络端口
- 平台进程生命周期管理简单
- 适合内部部署场景

**启动命令示例**：

```bash
# 平台侧通过 MCP Gateway 启动
node server/index.js
# 或
python -m enterprise_mcp_server
```

#### 可选：Streamable HTTP（SSE）

Server 以 HTTP 服务模式运行，平台通过 SSE（Server-Sent Events）接收通知，通过 HTTP POST 发送请求。

适用于：
- Server 需要保持持久连接状态
- Server 需要主动推送通知到平台

**端点约定**：

```
POST /mcp  — 发送 JSON-RPC 请求
GET  /mcp  — 接收 SSE 通知流（text/event-stream）
```

> 注意：若选择 HTTP 模式，企业须在配置文件中声明 `transport: http`，并在注册时提供 Server URL。stdio 模式无需暴露网络端口，优先推荐。

### 2.3 协议版本与协商

平台通过 `initialize` 请求宣告自身支持的协议版本范围：

```json
{
  "jsonrpc": "2.0",
  "id": 0,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-06-18",
    "capabilities": {
      "tools": {}
    },
    "clientInfo": {
      "name": "coin-ai-mcp-gateway",
      "version": "1.0.0"
    }
  }
}
```

Server 须在响应中确认协商后的协议版本：

```json
{
  "jsonrpc": "2.0",
  "id": 0,
  "result": {
    "protocolVersion": "2025-06-18",
    "capabilities": {
      "tools": {
        "listChanged": true
      }
    },
    "serverInfo": {
      "name": "enterprise-fashion-server",
      "version": "1.0.0"
    }
  }
}
```

Server 须在 `initialize` 完成后、收到任何其他请求前，发送 `initialized` 通知：

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/initialized",
  "params": {}
}
```

### 2.4 生命周期

```
┌──────────────┐
│  启动 Server │
└──────┬───────┘
       │ stdio: 读取 stdin；HTTP: 监听端口
       ▼
┌──────────────────┐
│  initialize      │◄─── 平台发起，协商协议版本与能力
└────────┬─────────┘
       │ Server 回应 protocolVersion + capabilities
       ▼
┌──────────────────────┐
│  notifications/      │◄─── Server 主动发送（仅 HTTP 模式需处理）
│  initialized         │
└────────┬─────────────┘
       │ 双方进入就绪状态
       ▼
┌──────────────────────────────────────────────────┐
│  正常运行阶段                                      │
│                                                   │
│  · tools/list         工具发现                     │
│  · tools/call         工具调用                     │
│  · (optional) prompts/list  提示词（若实现）        │
│  · (optional) resources/*   资源（若实现）          │
└──────────────────────┬───────────────────────────┘
       │
       ▼
┌──────────────────┐
│  shutdown        │◄─── 平台发起，优雅关闭
└────────┬─────────┘
       │ Server 清理资源，退出进程
       ▼
┌──────────────┐
│  Server 退出  │
└──────────────┘
```

---

## 3. 工具接口（Tools）

### 3.1 工具发现 — `tools/list`

平台通过此请求获取 Server 当前暴露的所有工具列表。Server 应在工具集变更时支持 `listChanged` 通知（本规范不强制实现动态刷新）。

**请求**：

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list",
  "params": {}
}
```

**响应**：

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": [
      {
        "name": "fashion.search_images",
        "description": "在企业图库中检索服装设计图片，支持关键词和分类过滤",
        "inputSchema": {
          "type": "object",
          "properties": {
            "query": {
              "type": "string",
              "description": "搜索关键词，如款式名、面料、风格等"
            },
            "filters": {
              "type": "object",
              "description": "筛选条件",
              "properties": {
                "category": {
                  "type": "string",
                  "enum": ["dress", "outerwear", "tops", "bottoms", "accessories"],
                  "description": "服装品类"
                },
                "season": {
                  "type": "string",
                  "enum": ["spring", "summer", "autumn", "winter", "all-season"],
                  "description": "季节"
                },
                "style": {
                  "type": "string",
                  "description": "风格标签，如'简约'、'复古'、'运动'"
                }
              }
            },
            "limit": {
              "type": "integer",
              "description": "返回数量上限",
              "default": 10,
              "minimum": 1,
              "maximum": 50
            }
          },
          "required": ["query"]
        }
      },
      {
        "name": "fashion.get_product_info",
        "description": "根据款号查询服装产品的详细信息",
        "inputSchema": {
          "type": "object",
          "properties": {
            "product_id": {
              "type": "string",
              "description": "产品款号/唯一标识符"
            }
          },
          "required": ["product_id"]
        }
      },
      {
        "name": "fashion.list_collections",
        "description": "列出企业在 Coin-AI 平台注册的所有可用数据集/系列",
        "inputSchema": {
          "type": "object",
          "properties": {
            "category": {
              "type": "string",
              "enum": ["fabric", "color", "style", "product", "knowledge"],
              "description": "数据集分类"
            }
          }
        }
      },
      {
        "name": "design.get_style_reference",
        "description": "获取指定品类的风格参考图像与描述",
        "inputSchema": {
          "type": "object",
          "properties": {
            "category": {
              "type": "string",
              "description": "品类名称，如'dress'、'outerwear'"
            },
            "style_tags": {
              "type": "array",
              "items": { "type": "string" },
              "description": "风格标签列表"
            }
          },
          "required": ["category"]
        }
      },
      {
        "name": "design.generate_variation",
        "description": "基于参考图片和文字指令生成设计变体",
        "inputSchema": {
          "type": "object",
          "properties": {
            "image_url": {
              "type": "string",
              "description": "参考设计图的 URL（企业内网可访问）"
            },
            "instruction": {
              "type": "string",
              "description": "变体生成指令，如'将廓形改为A字形、色彩改为莫兰迪色系'"
            }
          },
          "required": ["image_url", "instruction"]
        }
      },
      {
        "name": "knowledge.query",
        "description": "在企业设计知识库中进行问答检索",
        "inputSchema": {
          "type": "object",
          "properties": {
            "q": {
              "type": "string",
              "description": "自然语言查询，如'2026年女装流行趋势'"
            },
            "top_k": {
              "type": "integer",
              "description": "返回相关条目数量",
              "default": 5,
              "minimum": 1,
              "maximum": 20
            }
          },
          "required": ["q"]
        }
      }
    ]
  }
}
```

### 3.2 工具调用 — `tools/call`

**请求**：

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "fashion.search_images",
    "arguments": {
      "query": "2026 春夏新款连衣裙",
      "filters": { "category": "dress", "season": "spring" },
      "limit": 10
    }
  }
}
```

**响应（成功）**：

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"images\": [{\"id\": \"IMG-001\", \"title\": \"2026SS 连衣裙 A\", \"url\": \"https://internal.corp.com/images/2026ss-dress-a.jpg\", \"tags\": [\"春季\", \"连衣裙\", \"简约\"]}], \"total\": 1, \"query\": \"2026 春夏新款连衣裙\"}"
      }
    ]
  }
}
```

**响应（失败 — 工具执行异常）**：

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "error": {
    "code": -32002,
    "message": "Tool execution failed: 数据库查询超时"
  }
}
```

> **说明**：所有工具返回数据均放在 `content[0].text` 字段中，格式为 JSON 字符串，由企业 Server 负责序列化。平台 Gateway 不解析工具返回值内容，直接透传给 Agent。

### 3.3 统一响应格式规范

企业 MCP Server 所有工具调用成功响应的 `content` 字段须遵循以下格式：

```typescript
interface ToolResponse {
  content: Array<{
    type: "text";
    text: string; // JSON 序列化后的结果字符串
  }>;
}
```

`text` 字段内的 JSON 结构由企业自行定义，但须包含以下元信息字段：

```json
{
  "success": true,
  "data": { ... },        // 业务数据
  "meta": {
    "server": "enterprise-fashion-server",
    "version": "1.0.0",
    "latency_ms": 42
  }
}
```

错误响应使用标准 JSON-RPC error 对象（见 §2.1）。

### 3.4 必需与可选工具

| 工具名 | 必需 | 说明 |
|--------|------|------|
| `fashion.search_images` | 建议 | 图库检索 |
| `fashion.get_product_info` | 建议 | 款号查详情 |
| `fashion.list_collections` | 建议 | 列出数据集 |
| `design.get_style_reference` | 可选 | 风格参考 |
| `design.generate_variation` | 可选 | 变体生成 |
| `knowledge.query` | 建议 | 知识库问答 |

企业可按需实现上述工具的任意子集。未实现的工具在 `tools/list` 中不出现即可。平台不强制要求全部实现。

---

## 4. 认证与安全

### 4.1 认证机制

平台与 Enterprise MCP Server 之间采用 **Bearer Token** 认证。

#### 令牌传递方式（stdio 模式）

平台 MCP Gateway 启动企业 Server 时，通过**环境变量**传递令牌：

```bash
# 平台侧启动企业 MCP Server 时注入
MCP_AUTH_TOKEN="eyJhbGciOiJIUzI1NiJ9..." \
MCP_ORG_ID="org-uuid-xxxx" \
  node server/index.js
```

企业 Server 启动时须验证 `MCP_AUTH_TOKEN`：

```typescript
// TypeScript 示例
const token = process.env.MCP_AUTH_TOKEN;
const orgId = process.env.MCP_ORG_ID;
if (!token) {
  console.error("[MCP Server] MCP_AUTH_TOKEN is required");
  process.exit(1);
}
```

#### 令牌传递方式（HTTP 模式）

平台在每个 HTTP 请求的 `Authorization` 头中携带 Bearer Token：

```
Authorization: Bearer eyJhbGciOiJIUzI1NiJ9...
```

企业 Server 须在 `initialize` 请求中验证该令牌，拒绝无效请求。

#### 令牌格式

平台颁发的令牌为 JWT（HS256），Payload 包含：

```json
{
  "sub": "server-id",
  "org_id": "org-uuid",
  "server_name": "enterprise-fashion-server",
  "exp": 1735689600
}
```

企业 Server 验证时检查：
1. 签名有效性（使用平台公开的 HMAC 密钥验签）
2. `exp` 未过期
3. `org_id` 与本 Server 对应（防跨租户调用）

> 平台提供验签密钥的获取方式（平台管理员告知或配置文件注入）。

### 4.2 数据隔离

- Enterprise MCP Server 只能返回本组织（`org_id`）授权的数据
- Server 在处理每个请求时须检查 `org_id` 上下文，拒绝跨组织数据访问
- 平台 MCP Gateway 已在路由层保证请求携带正确的 `org_id` JWT claims
- Server 若发现 `org_id` 缺失或异常，应返回错误码 `-32001`（Permission denied）

### 4.3 日志与审计

- Enterprise MCP Server 须记录每次 `tools/call` 的调用日志
- 日志字段：`timestamp`、`method`、`tool_name`、`arguments`（脱敏后）、`latency_ms`、`status`（success/error）
- 日志保留周期由企业运维政策决定，平台不统一要求
- 平台 MCP Gateway 侧独立记录调用审计，与 Server 日志互为补充

### 4.4 安全建议

| 项目 | 建议 |
|------|------|
| 网络隔离 | Server 部署在企业内网，不对外暴露端口（stdio 模式无需端口） |
| 令牌存储 | 验签密钥写入配置文件或 Secret Manager，不硬编码 |
| 输入校验 | 所有工具参数须严格校验类型与范围，防止注入 |
| 限流 | 建议对每个工具实现调用频率限制（防止 Agent 循环调用耗尽资源） |
| 超时 | 工具执行超时建议 ≤ 30s，超时返回错误 |

---

## 5. 配置与部署

### 5.1 企业 MCP Server 配置文件

企业 Server 根目录须包含 `config.yaml` 或 `config.json`：

**config.yaml 示例**：

```yaml
# enterprise-mcp-server/config.yaml

server:
  name: "enterprise-fashion-server"
  version: "1.0.0"
  transport: "stdio"   # stdio | http

auth:
  verify_token: true   # 是否验证平台传来的 MCP_AUTH_TOKEN

tools:
  enabled:
    - fashion.search_images
    - fashion.get_product_info
    - fashion.list_collections
    - knowledge.query
  # 可选：每个工具的独立超时（毫秒）
  timeouts:
    fashion.search_images: 15000
    knowledge.query: 10000

database:
  type: "postgresql"  # postgresql | mysql | sqlite
  host: "10.0.1.50"
  port: 5432
  name: "fashion_db"
  # credentials 从环境变量或 Secret Manager 获取，不写在配置文件里

logging:
  level: "info"        # debug | info | warn | error
  format: "json"
```

**config.json 示例**（等效）：

```json
{
  "server": {
    "name": "enterprise-fashion-server",
    "version": "1.0.0",
    "transport": "stdio"
  },
  "auth": {
    "verify_token": true
  },
  "tools": {
    "enabled": [
      "fashion.search_images",
      "fashion.get_product_info",
      "fashion.list_collections",
      "knowledge.query"
    ]
  },
  "database": {
    "type": "postgresql",
    "host": "10.0.1.50",
    "port": 5432,
    "name": "fashion_db"
  },
  "logging": {
    "level": "info",
    "format": "json"
  }
}
```

### 5.2 平台注册方式

平台管理员在 Coin-AI MCP 管理页面录入以下信息：

| 字段 | 说明 | 示例 |
|------|------|------|
| Server 名称 | 标识名称，供管理员使用 | 企业面料数据服务 |
| Server 命令 | 启动命令（stdio 模式）或 URL（HTTP 模式） | `node /opt/mcp-server/index.js` 或 `https://mcp.corp.com` |
| 传输方式 | stdio 或 http | stdio |
| 认证 Token | 平台生成的 Bearer Token（64字符随机字符串或 JWT） | `eyJhbGciOiJIUzI1NiJ9...` |
| 组织绑定 | org_id | `org-uuid-xxxx` |
| 描述 | 可选，说明该 Server 提供哪些功能 | 提供面料、款式图库检索 |

### 5.3 健康检查

平台 MCP Gateway 定期向 HTTP 模式的 Server 发送 `ping` 请求（每 60s，可配置）：

```json
{
  "jsonrpc": "2.0",
  "id": null,
  "method": "ping",
  "params": {}
}
```

Server 须响应：

```json
{
  "jsonrpc": "2.0",
  "id": null,
  "result": {}
}
```

若连续 3 次 ping 无响应，平台标记该 Server 为 `unhealthy`，暂停调用并告警。

> **stdio 模式**：平台通过进程退出码判断健康状态。Server 异常退出（退出码 ≠ 0）视为不健康。

### 5.4 部署环境要求

| 项目 | 要求 |
|------|------|
| Node.js | ≥18.0（若使用 Node.js SDK） |
| Python | ≥3.10（若使用 Python SDK） |
| 内存 | ≥512MB |
| 网络 | 企业内网可达（若 HTTP 模式） |
| 数据库 | Server 自行连接企业数据源 |

---

## 6. 开发指南

### 6.1 推荐 SDK

| 语言 | SDK | 文档 |
|------|-----|------|
| TypeScript / Node.js | `@modelcontextprotocol/sdk` | https://github.com/modelcontextprotocol/typescript-sdk |
| Python | `mcp`（mcp Python SDK） | https://github.com/modelcontextprotocol/python-sdk |

### 6.2 TypeScript 模板代码

```typescript
// enterprise-mcp-server/src/index.ts
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

// ── 工具定义 ──────────────────────────────────────────────────────────────

const TOOLS = [
  {
    name: "fashion.search_images",
    description: "在企业图库中检索服装设计图片",
    inputSchema: {
      type: "object",
      properties: {
        query: {
          type: "string",
          description: "搜索关键词",
        },
        filters: {
          type: "object",
          properties: {
            category: {
              type: "string",
              enum: ["dress", "outerwear", "tops", "bottoms", "accessories"],
            },
          },
        },
        limit: {
          type: "integer",
          default: 10,
          minimum: 1,
          maximum: 50,
        },
      },
      required: ["query"],
    },
  },
  {
    name: "fashion.get_product_info",
    description: "根据款号查询产品详情",
    inputSchema: {
      type: "object",
      properties: {
        product_id: {
          type: "string",
          description: "产品款号",
        },
      },
      required: ["product_id"],
    },
  },
  {
    name: "fashion.list_collections",
    description: "列出所有可用数据集",
    inputSchema: {
      type: "object",
      properties: {
        category: {
          type: "string",
          enum: ["fabric", "color", "style", "product", "knowledge"],
        },
      },
    },
  },
  {
    name: "knowledge.query",
    description: "企业知识库问答检索",
    inputSchema: {
      type: "object",
      properties: {
        q: {
          type: "string",
          description: "自然语言查询",
        },
        top_k: {
          type: "integer",
          default: 5,
          minimum: 1,
          maximum: 20,
        },
      },
      required: ["q"],
    },
  },
];

// ── 工具执行逻辑 ──────────────────────────────────────────────────────────

async function executeTool(
  name: string,
  arguments_: Record<string, unknown>
): Promise<{ content: Array<{ type: "text"; text: string }> }> {
  const start = Date.now();

  try {
    switch (name) {
      case "fashion.search_images": {
        const { query, filters, limit = 10 } = arguments_ as {
          query: string;
          filters?: { category?: string };
          limit?: number;
        };
        // TODO: 替换为企业实际图库检索逻辑
        const results = await searchImages(query, filters?.category, limit);
        return wrapResponse(true, { images: results }, start);
      }

      case "fashion.get_product_info": {
        const { product_id } = arguments_ as { product_id: string };
        // TODO: 替换为企业实际产品数据查询逻辑
        const product = await getProductById(product_id);
        return wrapResponse(true, product, start);
      }

      case "fashion.list_collections": {
        const { category } = arguments_ as { category?: string };
        // TODO: 替换为企业实际数据集列表逻辑
        const collections = await listCollections(category);
        return wrapResponse(true, { collections }, start);
      }

      case "knowledge.query": {
        const { q, top_k = 5 } = arguments_ as { q: string; top_k?: number };
        // TODO: 替换为企业实际知识库检索逻辑
        const answers = await queryKnowledgeBase(q, top_k);
        return wrapResponse(true, { answers }, start);
      }

      default:
        throw new Error(`Unknown tool: ${name}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    // 返回错误结构（平台按 JSON-RPC error 处理）
    throw { code: -32002, message: `Tool execution failed: ${message}` };
  }
}

// ── 响应包装 ──────────────────────────────────────────────────────────────

function wrapResponse(
  success: boolean,
  data: unknown,
  start: number
): { content: Array<{ type: "text"; text: string }> } {
  return {
    content: [
      {
        type: "text" as const,
        text: JSON.stringify({
          success,
          data,
          meta: {
            server: "enterprise-fashion-server",
            version: "1.0.0",
            latency_ms: Date.now() - start,
          },
        }),
      },
    ],
  };
}

// ── TODO: 替换为企业实际实现 ──────────────────────────────────────────────

async function searchImages(
  query: string,
  category?: string,
  limit = 10
): Promise<unknown[]> {
  // 示例返回值：替换为企业真实图库 API 调用
  return [
    {
      id: "IMG-001",
      title: `${query} 示例款`,
      url: "https://internal.corp.com/images/sample.jpg",
      tags: [category || "general"],
    },
  ];
}

async function getProductById(productId: string): Promise<unknown> {
  return {
    product_id: productId,
    name: "示例产品",
    category: "dress",
    season: "spring",
  };
}

async function listCollections(
  category?: string
): Promise<unknown[]> {
  const all = [
    { id: "fabrics-2026", name: "2026面料系列", type: "fabric" },
    { id: "colors-palette", name: "色彩企划", type: "color" },
    { id: "styles-main", name: "款式主库", type: "style" },
    { id: "products-ss", name: "春夏产品库", type: "product" },
    { id: "design-knowledge", name: "设计知识库", type: "knowledge" },
  ];
  return category ? all.filter((c) => c.type === category) : all;
}

async function queryKnowledgeBase(
  q: string,
  topK: number
): Promise<unknown[]> {
  return [
    { question: q, answer: "这是基于企业知识库的示例回答。", score: 0.95 },
  ];
}

// ── 认证（可选）────────────────────────────────────────────────────────────

function verifyAuthToken(): void {
  const token = process.env.MCP_AUTH_TOKEN;
  const orgId = process.env.MCP_ORG_ID;
  if (!token) {
    console.error("[MCP Server] MCP_AUTH_TOKEN is required");
    process.exit(1);
  }
  console.error(`[MCP Server] Starting for org: ${orgId}`);
  // TODO: 实现 JWT 验签逻辑（使用平台提供的密钥）
  // const payload = verifyJWT(token, process.env.MCP_SIGNING_KEY);
  // if (payload.org_id !== orgId) throw new Error("org_id mismatch");
}

// ── Server 初始化 ──────────────────────────────────────────────────────────

async function main() {
  verifyAuthToken();

  const transport = new StdioServerTransport();
  const server = new Server(
    {
      name: "enterprise-fashion-server",
      version: "1.0.0",
    },
    {
      capabilities: {
        tools: { listChanged: false },
      },
    }
  );

  server.setRequestHandler(ListToolsRequestSchema, async () => {
    return { tools: TOOLS };
  });

  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    try {
      const result = await executeTool(name, args);
      return result;
    } catch (err: unknown) {
      // 若 err 是我们的错误对象，抛出标准 JSON-RPC 错误
      if (typeof err === "object" && err !== null && "code" in err) {
        const { code, message } = err as { code: number; message: string };
        throw new Error(JSON.stringify({ code, message }));
      }
      throw err;
    }
  });

  await server.connect(transport);
  console.error("[MCP Server] Connected and ready.");
}

main().catch((err) => {
  console.error("[MCP Server] Fatal error:", err);
  process.exit(1);
});
```

**package.json 依赖**：

```json
{
  "name": "enterprise-mcp-server",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "start": "node dist/index.js",
    "dev": "tsx src/index.ts"
  },
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.0.0"
  },
  "devDependencies": {
    "tsx": "^4.0.0",
    "typescript": "^5.0.0"
  }
}
```

### 6.3 Python 模板代码

```python
# enterprise_mcp_server/main.py
import json
import os
import sys
import time
from typing import Any

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import Tool, CallToolResult, TextContent

# ── 工具定义 ───────────────────────────────────────────────────────────────

TOOLS: list[Tool] = [
    Tool(
        name="fashion.search_images",
        description="在企业图库中检索服装设计图片",
        inputSchema={
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "搜索关键词"},
                "filters": {
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "enum": ["dress", "outerwear", "tops", "bottoms", "accessories"],
                        },
                    },
                },
                "limit": {"type": "integer", "default": 10, "minimum": 1, "maximum": 50},
            },
            "required": ["query"],
        },
    ),
    Tool(
        name="fashion.get_product_info",
        description="根据款号查询产品详情",
        inputSchema={
            "type": "object",
            "properties": {
                "product_id": {"type": "string", "description": "产品款号"},
            },
            "required": ["product_id"],
        },
    ),
    Tool(
        name="fashion.list_collections",
        description="列出所有可用数据集",
        inputSchema={
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "enum": ["fabric", "color", "style", "product", "knowledge"],
                },
            },
        },
    ),
    Tool(
        name="knowledge.query",
        description="企业知识库问答检索",
        inputSchema={
            "type": "object",
            "properties": {
                "q": {"type": "string", "description": "自然语言查询"},
                "top_k": {"type": "integer", "default": 5, "minimum": 1, "maximum": 20},
            },
            "required": ["q"],
        },
    ),
]

# ── 工具执行逻辑 ────────────────────────────────────────────────────────────

async def execute_tool(name: str, arguments: dict[str, Any]) -> CallToolResult:
    start = time.time()
    try:
        if name == "fashion.search_images":
            query = arguments.get("query", "")
            category = arguments.get("filters", {}).get("category")
            limit = arguments.get("limit", 10)
            results = await search_images(query, category, limit)
            return wrap_response(True, {"images": results}, start)

        elif name == "fashion.get_product_info":
            product_id = arguments.get("product_id")
            product = await get_product_by_id(product_id)
            return wrap_response(True, product, start)

        elif name == "fashion.list_collections":
            category = arguments.get("category")
            collections = await list_collections(category)
            return wrap_response(True, {"collections": collections}, start)

        elif name == "knowledge.query":
            q = arguments.get("q", "")
            top_k = arguments.get("top_k", 5)
            answers = await query_knowledge_base(q, top_k)
            return wrap_response(True, {"answers": answers}, start)

        else:
            raise ValueError(f"Unknown tool: {name}")

    except Exception as e:
        return CallToolResult(
            content=[TextContent(type="text", text=json.dumps({
                "success": False,
                "error": str(e),
                "meta": {
                    "server": "enterprise-fashion-server",
                    "version": "1.0.0",
                    "latency_ms": int((time.time() - start) * 1000),
                }
            }))],
            isError=True,
        )


def wrap_response(success: bool, data: Any, start: float) -> CallToolResult:
    return CallToolResult(
        content=[TextContent(
            type="text",
            text=json.dumps({
                "success": success,
                "data": data,
                "meta": {
                    "server": "enterprise-fashion-server",
                    "version": "1.0.0",
                    "latency_ms": int((time.time() - start) * 1000),
                },
            }),
        )]
    )


# ── TODO: 替换为企业实际实现 ────────────────────────────────────────────────

async def search_images(query: str, category: str | None, limit: int) -> list[dict]:
    return [
        {
            "id": "IMG-001",
            "title": f"{query} 示例款",
            "url": "https://internal.corp.com/images/sample.jpg",
            "tags": [category or "general"],
        }
    ]


async def get_product_by_id(product_id: str) -> dict:
    return {
        "product_id": product_id,
        "name": "示例产品",
        "category": "dress",
        "season": "spring",
    }


async def list_collections(category: str | None) -> list[dict]:
    all_collections = [
        {"id": "fabrics-2026", "name": "2026面料系列", "type": "fabric"},
        {"id": "colors-palette", "name": "色彩企划", "type": "color"},
        {"id": "styles-main", "name": "款式主库", "type": "style"},
        {"id": "products-ss", "name": "春夏产品库", "type": "product"},
        {"id": "design-knowledge", "name": "设计知识库", "type": "knowledge"},
    ]
    if category:
        return [c for c in all_collections if c["type"] == category]
    return all_collections


async def query_knowledge_base(q: str, top_k: int) -> list[dict]:
    return [
        {"question": q, "answer": "这是基于企业知识库的示例回答。", "score": 0.95}
    ]


# ── 认证 ────────────────────────────────────────────────────────────────────

def verify_auth_token():
    token = os.environ.get("MCP_AUTH_TOKEN")
    org_id = os.environ.get("MCP_ORG_ID")
    if not token:
        print("MCP_AUTH_TOKEN is required", file=sys.stderr)
        sys.exit(1)
    print(f"[MCP Server] Starting for org: {org_id}", file=sys.stderr)
    # TODO: 实现 JWT 验签逻辑
    # import jwt
    # payload = jwt.decode(token, os.environ["MCP_SIGNING_KEY"], algorithms=["HS256"])


# ── Server 启动 ─────────────────────────────────────────────────────────────

async def main():
    verify_auth_token()

    server = Server("enterprise-fashion-server", "1.0.0")

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        return TOOLS

    @server.call_tool()
    async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
        result = await execute_tool(name, arguments)
        return result.content

    options = server.create_initialization_options()
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, options, raise_exceptions=True)


if __name__ == "__main__":
    import asyncio
    asyncio.run(main())
```

**requirements.txt**：

```
mcp>=1.0.0
```

### 6.4 本地调试方法

#### 6.4.1 Standalone 模式验证

Server 开发完成后，先以 standalone 模式独立运行，验证协议握手与工具发现：

**Step 1：验证 `initialize` + `initialized` 握手**

```bash
# 终端 1：启动 Server
MCP_AUTH_TOKEN="test-token" MCP_ORG_ID="test-org" node dist/index.js

# 终端 2：手动发送 JSON-RPC 请求到 stdin
echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"clientInfo":{"name":"test","version":"1.0.0"}}}' | node dist/index.js
```

**Step 2：验证 `tools/list`**

```bash
# 手动构造测试流
node -e "
const { StdioServerTransport } = require('@modelcontextprotocol/sdk/server/stdio.js');
const { Server } = require('@modelcontextprotocol/sdk/server/index.js');
// ... 注入测试请求 ...
"
```

#### 6.4.2 Mock 平台 Gateway（开发阶段）

开发阶段使用平台提供的 Mock Gateway 脚本模拟调用：

```bash
# 平台提供 mock-mcp-gateway 工具
./scripts/mock-mcp-gateway --server-cmd "node dist/index.js" --test-case search_images
```

#### 6.4.3 日志调试

Server 所有 `console.error` 输出在 stdio 模式下被平台捕获为调试日志：

```typescript
console.error("[MCP Server] tools/call: fashion.search_images, args:", args);
```

---

## 7. 版本与演进

### 7.1 规范版本号

遵循 Semantic Versioning：

| 版本 | 含义 |
|------|------|
| MAJOR（v2.0.0） | 破坏性变更 — 不兼容旧版本 Server |
| MINOR（v1.1.0） | 新增工具或可选能力 — 向后兼容 |
| PATCH（v1.0.1） | 文档修正、澄清 — 向后兼容 |

**当前版本：v1.0.0**

### 7.2 向后兼容策略

- 平台 MCP Gateway 支持**多个协议版本并存**
- 企业 Server 实现 v1.0.0，即兼容平台所有 v1.x Gateway
- 新增工具为**可选实现**（`tools/list` 中不出现不影响兼容性）
- 移除工具或修改工具参数签名属于 MAJOR 变更，须提前 3 个月通知

### 7.3 如何提交规范变更申请

1. 在 Coin-AI 平台 Git 仓库提交 Issue，标签 `mcp-spec-change`
2. 填写变更申请模板（Issue 内容包含：变更描述、理由、影响评估）
3. 平台架构组在 5 个工作日内评审并回复
4. 评审通过后进入规范修订流程，发布新版本

### 7.4 规范历史

| 版本 | 日期 | 变更 |
|------|------|------|
| v1.0.0 | 2026-09-23 | 初始版本，定义 stdio/HTTP 传输、6 个核心工具接口、认证机制与配置规范 |

---

## 附录 A：完整工具 inputSchema 参考

详见 §3.1 `tools/list` 响应中的 `inputSchema` 定义。所有 schema 遵循 [JSON Schema Draft-07](https://json-schema.org/draft-07/draft-handrews-json-schema-00)。

## 附录 B：错误码速查

| 错误码 | 含义 | 触发场景 |
|--------|------|----------|
| -32700 | Parse error | JSON 格式错误 |
| -32600 | Invalid Request | 请求结构非法 |
| -32601 | Method not found | 方法名错误 |
| -32602 | Invalid params | 参数缺失或类型错误 |
| -32603 | Internal error | Server 内部异常 |
| -32000 | Auth failed | Token 无效或缺失 |
| -32001 | Permission denied | org_id 不匹配 |
| -32002 | Tool execution failed | 工具执行异常 |

## 附录 C：配置字段完整参考

```yaml
# 完整配置字段说明
server:
  name: string              # Server 标识名
  version: string           # 语义化版本
  transport: "stdio" | "http"

auth:
  verify_token: boolean     # 是否验证平台令牌（生产环境建议 true）

tools:
  enabled: string[]         # 启用的工具列表
  timeouts:                 # 可选：各工具超时（ms）
    <tool_name>: integer

database:
  type: "postgresql" | "mysql" | "sqlite" | "other"
  host: string
  port: integer
  name: string
  # credentials 通过环境变量或 Secret Manager 注入，不写在配置文件

logging:
  level: "debug" | "info" | "warn" | "error"
  format: "json" | "text"
```

## 附录 D：术语表

| 术语 | 说明 |
|------|------|
| MCP Gateway | 平台侧组件，负责与企业 MCP Server 连接、认证、调用路由（平台提供） |
| Enterprise MCP Server | 企业自行开发的 MCP Server，接入企业专有数据（企业开发） |
| stdio 传输 | 通过子进程 stdin/stdout 通信，适合本地部署（推荐） |
| SSE 传输 | HTTP + Server-Sent Events 双向通信（可选） |
| Bearer Token | 平台颁发的 JWT 格式认证令牌 |
| tools/list | MCP 协议方法：查询 Server 暴露的工具列表 |
| tools/call | MCP 协议方法：调用指定工具 |
| Conformance 测试 | 平台提供的自动化测试，验证企业 Server 是否符合本规范 |
