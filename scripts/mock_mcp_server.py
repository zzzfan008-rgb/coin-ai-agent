#!/usr/bin/env python3
"""Mock MCP Server — for local development and integration testing.

Exposes fashion-domain tools over plain HTTP that match the endpoints
the Rust McpClient calls:
  GET  /health        → health check
  GET  /tools         → tool discovery
  POST /tools/{name}/invoke → tool invocation

Run:
  pip install fastapi uvicorn
  python scripts/mock_mcp_server.py
  # or: uvicorn scripts.mock_mcp_server:app --port 9101 --reload

Default port is 9101 (override with MCP_MOCK_PORT). Port 9001 is avoided
because the platform's MinIO Console already binds it.

Server registers itself against the default base URL http://localhost:9101.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI(title="Fashion AI — Mock MCP Server", version="0.1.0")

# ── Tool schema definitions ────────────────────────────────────────────────────

TOOLS: list[dict[str, Any]] = [
    {
        "name": "fabric_search_db",
        "description": "从面料数据库中搜索面料（按成分、克重、适用季节等条件）",
        "input_schema": {
            "type": "object",
            "properties": {
                "composition": {
                    "type": "string",
                    "description": "面料成分关键词，如 cotton / silk / polyester",
                },
                "weight_min": {"type": "number", "description": "最小克重 (gsm)"},
                "weight_max": {"type": "number", "description": "最大克重 (gsm)"},
                "season": {"type": "string", "description": "适用季节"},
                "limit": {"type": "integer", "default": 10},
            },
        },
    },
    {
        "name": "fabric_get_detail",
        "description": "获取单个面料的详细信息（含护理说明、适用款式等）",
        "input_schema": {
            "type": "object",
            "properties": {"fabric_id": {"type": "string", "format": "uuid"}},
            "required": ["fabric_id"],
        },
    },
    {
        "name": "color_harmony_check",
        "description": "分析一组颜色的和谐度，返回配色建议",
        "input_schema": {
            "type": "object",
            "properties": {
                "colors": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "HEX 颜色值列表",
                }
            },
            "required": ["colors"],
        },
    },
    {
        "name": "trend_colors_get",
        "description": "获取当季流行色趋势",
        "input_schema": {
            "type": "object",
            "properties": {
                "season": {"type": "string", "default": "spring"},
                "category": {"type": "string", "default": "apparel"},
            },
        },
    },
    {
        "name": "style_search",
        "description": "按关键词搜索款式库（名称、描述、品类、特征）",
        "input_schema": {
            "type": "object",
            "properties": {
                "keywords": {"type": "array", "items": {"type": "string"}},
                "garment_type": {"type": "string"},
                "limit": {"type": "integer", "default": 10},
            },
            "required": ["keywords"],
        },
    },
]


# ── Request / response models ──────────────────────────────────────────────────


class InvokeRequest(BaseModel):
    parameters: dict[str, Any] = {}


# ── Endpoints ──────────────────────────────────────────────────────────────────


@app.get("/health")
def health() -> dict[str, str]:
    return {"status": "ok", "server": "mock_mcp_server", "time": datetime.utcnow().isoformat()}


@app.get("/tools")
def list_tools() -> dict[str, list[dict[str, Any]]]:
    return {"tools": TOOLS}


@app.post("/tools/{tool_name}/invoke")
def invoke_tool(tool_name: str, body: InvokeRequest) -> dict[str, Any]:
    params = body.parameters

    if tool_name == "fabric_search_db":
        return {
            "fabrics": [
                {
                    "id": "f-001",
                    "name": "高支精梳棉府绸",
                    "composition": "100% Cotton",
                    "weight_gsm": 128,
                    "season": params.get("season", "spring"),
                    "applicable_styles": ["衬衫", "连衣裙"],
                },
                {
                    "id": "f-002",
                    "name": "真丝双绉",
                    "composition": "100% Silk",
                    "weight_gsm": 16,
                    "season": "spring/summer",
                    "applicable_styles": ["衬衫", "礼服"],
                },
            ],
            "count": 2,
            "note": "Mock MCP result",
        }

    if tool_name == "fabric_get_detail":
        fabric_id = params.get("fabric_id", "unknown")
        return {
            "id": fabric_id,
            "name": "高支精梳棉府绸",
            "composition": "100% Cotton",
            "weight_gsm": 128,
            "width_cm": 147,
            "care_instructions": ["机洗冷水", "不可漂白", "低温熨烫"],
            "features": ["透气", "亲肤", "抗皱性中等"],
        }

    if tool_name == "color_harmony_check":
        colors = params.get("colors", [])
        return {
            "colors": colors,
            "harmony_score": 78,
            "analysis": {
                "hue_balance": "良好 — 色相环分布均匀",
                "saturation_contrast": "适中",
                "lightness_range": "对比度 45% — 层次丰富",
            },
            "suggestions": ["可适当提高明度以增加透气感", "建议加入中性色作为过渡"],
        }

    if tool_name == "trend_colors_get":
        return {
            "season": params.get("season", "spring"),
            "trends": [
                {"hex": "#8B5CF6", "name": "极光紫", "hot": "high"},
                {"hex": "#10B981", "name": "鼠尾草绿", "hot": "high"},
                {"hex": "#F59E0B", "name": "琥珀金", "hot": "medium"},
            ],
        }

    if tool_name == "style_search":
        keywords = params.get("keywords", [])
        return {
            "keywords": keywords,
            "styles": [
                {
                    "id": "s-001",
                    "name": "都市轻通勤西装外套",
                    "garment_type": "outerwear",
                    "silhouette": "H型",
                    "key_features": ["平驳领", "单排两粒扣", "贴袋"],
                }
            ],
            "count": 1,
        }

    return {"error": f"Unknown tool: {tool_name}", "available": [t["name"] for t in TOOLS]}


if __name__ == "__main__":
    import os

    import uvicorn

    port = int(os.environ.get("MCP_MOCK_PORT", "9101"))
    uvicorn.run(app, host="0.0.0.0", port=port)
