#!/usr/bin/env python3
"""T-026 本地 CLIP 部署评估 — POC 第一阶段最小可运行服务。

加载本地 HuggingFace ViT-B/32，暴露两个编码端点，返回 512 维 L2-normalized
embedding。请求/响应形状刻意对齐 api-core/src/images/clip.rs 的 Generic 模式，
便于将来接线（T-027），但本文件本身不改任何生产代码。

契约
----
POST /encode/image
  请求（二选一）:
    a) Content-Type: application/json
       {"model": "clip-vit-base-patch32", "image": "<base64 或 data:image/...;base64,...>"}
       （与 ClipClient Generic 模式的请求体一致）
    b) Content-Type: image/jpeg | image/png | image/webp | application/octet-stream
       原始图片字节作为 request body
  响应:
    {"embedding": [..512..], "dim": 512, "model": "...", "device": "mps|cpu",
     "norm": 1.000000, "elapsed_ms": 12.34}
    响应体也兼容 ClipClient::parse_embedding 的 `{"embedding": [...]}` 形状。

POST /encode/text
  请求: Content-Type: application/json, {"model": "...", "text": "一条中文商品描述"}
  响应: 同上（embedding 为文本向量）

GET /health
  {"status":"ok","model":"...","device":"mps|cpu","dim":512}

用法（在项目根目录）:
    HTTPS_PROXY=http://127.0.0.1:7897 ./.venv/bin/python scripts/clip_poc_server.py \
        --host 127.0.0.1 --port 8399
"""

from __future__ import annotations

import argparse
import base64
import io
import os
import threading
import time
from typing import Any

for _k in ("HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"):
    os.environ.setdefault(_k, "http://127.0.0.1:7897")
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")

import torch
from fastapi import FastAPI, HTTPException, Request
from PIL import Image
from pydantic import BaseModel, Field
from transformers import CLIPModel, CLIPProcessor

DEFAULT_MODEL = "openai/clip-vit-base-patch32"
MAX_IMAGE_BYTES = 16 * 1024 * 1024  # 16MB，POC 级上限，防误传大文件撑爆内存
MAX_TEXT_CHARS = 2000


# ── 请求模型 ────────────────────────────────────────────────────────────────

class ImageRequest(BaseModel):
    model: str | None = None
    image: str = Field(..., description="base64 编码的图片字节，允许 data: URL 前缀")


class TextRequest(BaseModel):
    model: str | None = None
    text: str = Field(..., min_length=1, max_length=MAX_TEXT_CHARS)


# ── 应用状态 ────────────────────────────────────────────────────────────────

class ClipService:
    def __init__(self, model_id: str, prefer_device: str):
        self.model_id = model_id
        self.prefer_device = prefer_device
        self._lock = threading.Lock()
        mps_avail = torch.backends.mps.is_available()
        self.device = prefer_device if prefer_device != "auto" else ("mps" if mps_avail else "cpu")
        if self.device == "mps" and not mps_avail:
            self.device = "cpu"
            self.degraded = True
            self.degradation_reason = "请求 MPS 但 torch.backends.mps.is_available() == False，已降级 CPU"
        else:
            self.degraded = False
            self.degradation_reason = None

        self.model = CLIPModel.from_pretrained(model_id)
        self.processor = CLIPProcessor.from_pretrained(model_id)
        self.model.eval()
        self.model.to(self.device)

        # 探针：确定输出维度 + 预热（MPS 首次 kernel 编译开销大，不能算进请求延迟）
        self.dim = self._probe_dim()
        self._warmup()

    # transformers >= 5.0 的 get_image_features 可能返回 BaseModelOutputWithPooling
    @staticmethod
    def _pooled(out: Any) -> torch.Tensor:
        if hasattr(out, "pooler_output") and out.pooler_output is not None:
            return out.pooler_output
        return out

    def _probe_dim(self) -> int:
        with torch.inference_mode():
            px = self.processor(images=Image.new("RGB", (224, 224), "white"),
                                return_tensors="pt").pixel_values.to(self.device)
            return int(self._pooled(self.model.get_image_features(pixel_values=px)).shape[-1])

    def _warmup(self) -> None:
        with torch.inference_mode():
            px = self.processor(images=Image.new("RGB", (224, 224), "white"),
                                return_tensors="pt").pixel_values.to(self.device)
            self._pooled(self.model.get_image_features(pixel_values=px))
            te = self.processor(text="warmup", return_tensors="pt",
                                padding=True, truncation=True)
            te = {k: v.to(self.device) for k, v in te.items()}
            self._pooled(self.model.get_text_features(**te))
        if self.device == "mps":
            torch.mps.synchronize()

    def _sync(self) -> None:
        if self.device == "mps":
            torch.mps.synchronize()

    @staticmethod
    def _decode_b64(payload: str) -> bytes:
        raw = payload.split(",", 1)[1] if payload.startswith("data:") else payload
        try:
            return base64.b64decode(raw, validate=False)
        except Exception as exc:  # noqa: BLE001 — 转成 400 而不是 500
            raise HTTPException(status_code=400, detail=f"base64 解码失败: {exc}") from exc

    def encode_image_bytes(self, data: bytes) -> dict:
        if not data:
            raise HTTPException(status_code=400, detail="图片字节为空")
        if len(data) > MAX_IMAGE_BYTES:
            raise HTTPException(status_code=413,
                                detail=f"图片超过 {MAX_IMAGE_BYTES // 1024 // 1024}MB 上限")
        try:
            with Image.open(io.BytesIO(data)) as im:
                image = im.convert("RGB")
        except Exception as exc:  # noqa: BLE001
            raise HTTPException(status_code=400,
                                detail=f"图片解码失败（请确认是 JPEG/PNG/WebP）: {exc}") from exc

        with self._lock:
            self._sync()
            t0 = time.perf_counter()
            with torch.inference_mode():
                px = self.processor(images=image, return_tensors="pt").pixel_values.to(self.device)
                feats = self._pooled(self.model.get_image_features(pixel_values=px))
            self._sync()
            elapsed = (time.perf_counter() - t0) * 1000.0
            vec = feats[0].float().cpu()
            norm = float(vec.norm())

        if vec.shape[0] != self.dim:
            raise HTTPException(status_code=500,
                                detail=f"维度异常: 期望 {self.dim}，实际 {vec.shape[0]}")
        emb = (vec / norm).tolist()
        return {"embedding": emb, "dim": self.dim, "model": self.model_id,
                "device": self.device, "norm": round(norm, 6),
                "elapsed_ms": round(elapsed, 3)}

    def encode_text(self, text: str) -> dict:
        with self._lock:
            self._sync()
            t0 = time.perf_counter()
            with torch.inference_mode():
                te = self.processor(text=text, return_tensors="pt",
                                    padding=True, truncation=True)
                te = {k: v.to(self.device) for k, v in te.items()}
                feats = self._pooled(self.model.get_text_features(**te))
            self._sync()
            elapsed = (time.perf_counter() - t0) * 1000.0
            vec = feats[0].float().cpu()
            norm = float(vec.norm())

        if vec.shape[0] != self.dim:
            raise HTTPException(status_code=500,
                                detail=f"维度异常: 期望 {self.dim}，实际 {vec.shape[0]}")
        emb = (vec / norm).tolist()
        return {"embedding": emb, "dim": self.dim, "model": self.model_id,
                "device": self.device, "norm": round(norm, 6),
                "elapsed_ms": round(elapsed, 3)}


# ── FastAPI 应用 ────────────────────────────────────────────────────────────

def build_app(service: ClipService) -> FastAPI:
    app = FastAPI(title="T-026 CLIP POC Server", version="0.1.0",
                  description="本地 ViT-B/32 编码服务 POC（非生产）")

    @app.get("/health")
    def health() -> dict:
        return {"status": "ok", "model": service.model_id, "device": service.device,
                "dim": service.dim, "degraded": service.degraded,
                "degradation_reason": service.degradation_reason}

    @app.post("/encode/image")
    async def encode_image(request: Request) -> dict:
        ctype = (request.headers.get("content-type") or "").split(";")[0].strip().lower()
        if ctype in ("application/json", ""):
            try:
                body = await request.json()
            except Exception as exc:  # noqa: BLE001
                raise HTTPException(status_code=400,
                                    detail=f"请求体不是合法 JSON: {exc}") from exc
            if not isinstance(body, dict) or "image" not in body:
                raise HTTPException(status_code=400,
                                    detail="JSON 请求体缺少 `image` 字段（base64）")
            data = service._decode_b64(str(body["image"]))
        elif ctype.startswith("image/") or ctype == "application/octet-stream":
            data = await request.body()
        else:
            raise HTTPException(status_code=415,
                                detail=f"不支持的 Content-Type: {ctype!r}")
        return service.encode_image_bytes(data)

    @app.post("/encode/text")
    async def encode_text(request: TextRequest) -> dict:
        return service.encode_text(request.text)

    return app


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="T-026 本地 CLIP POC 服务")
    p.add_argument("--model", default=DEFAULT_MODEL)
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8399)
    p.add_argument("--device", default="auto", choices=["auto", "mps", "cpu"])
    return p.parse_args()


def main() -> None:
    args = parse_args()
    print(f"[INFO] 加载模型 {args.model} (device={args.device}) ...")
    service = ClipService(args.model, args.device)
    print(f"[INFO] 就绪: dim={service.dim} device={service.device} "
          f"degraded={service.degraded}")
    app = build_app(service)

    import uvicorn
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")


if __name__ == "__main__":
    main()
