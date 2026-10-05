#!/usr/bin/env python3
"""T-026 本地 CLIP 部署评估 — POC 第一阶段基准脚本。

对 data/images/ 下的服装图批量编码 ViT-B/32 image embedding，并测量：
  - CPU / MPS 单张编码延迟（P50 / P95 / P99，含预热后的 N 次采样）
  - CPU / MPS 整批吞吐（wall time）
  - 文本编码延迟（从 data/annotations.json 抽样）
  - MPS 不可用时优雅降级到 CPU，并在输出里显式标注

用法（在项目根目录）:
    HTTPS_PROXY=http://127.0.0.1:7897 ./.venv/bin/python scripts/clip_poc_bench.py

输出:
    data/poc_clip_bench.json   (data/ 已被 .gitignore 忽略，不入库)
    stdout 摘要表
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

# ── 下载代理（本机 HF 直连不通，必须走本地代理）───────────────────────────
for _k in ("HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"):
    os.environ.setdefault(_k, "http://127.0.0.1:7897")
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")

import torch  # noqa: E402
from PIL import Image  # noqa: E402
from transformers import CLIPModel, CLIPProcessor  # noqa: E402

DEFAULT_MODEL = "openai/clip-vit-base-patch32"
SINGLE_LATENCY_SAMPLES = 50  # 单张延迟采样次数（>= 20，够 P95/P99 有区分度）
WARMUP_ITERATIONS = 5
TEXT_SAMPLE_SIZE = 20


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="T-026 本地 CLIP POC 基准")
    p.add_argument("--model", default=DEFAULT_MODEL)
    p.add_argument("--data", default="data/annotations.json")
    p.add_argument("--images-dir", default="data/images")
    p.add_argument("--output", default="data/poc_clip_bench.json")
    p.add_argument("--device", default="auto", choices=["auto", "mps", "cpu"],
                   help="auto = MPS 可用则用 MPS，否则降级 CPU（默认）")
    p.add_argument("--batch-size", type=int, default=16)
    p.add_argument("--samples", type=int, default=SINGLE_LATENCY_SAMPLES)
    p.add_argument("--text-samples", type=int, default=TEXT_SAMPLE_SIZE)
    p.add_argument("--skip-cpu", action="store_true", help="只跑 MPS 路径")
    p.add_argument("--skip-mps", action="store_true", help="只跑 CPU 路径")
    return p.parse_args()


# ── 设备选择 ────────────────────────────────────────────────────────────────

def resolve_devices(requested: str, skip_cpu: bool, skip_mps: bool) -> tuple[list[str], dict]:
    """返回要跑的设备列表 + 降级说明。"""
    mps_built = torch.backends.mps.is_built()
    mps_avail = torch.backends.mps.is_available()
    mps_reason = None

    if mps_built and not mps_avail:
        mps_reason = ("torch.backends.mps.is_built() == True 但 is_available() == False"
                      "（torch 构建含 MPS，当前硬件/驱动不可用）")
    elif not mps_built:
        mps_reason = "当前 torch 构建不含 MPS 后端"

    degraded = False
    if requested == "mps":
        if not mps_avail:
            degraded = True
            mps_reason = (mps_reason or "MPS 不可用") + "；--device=mps 请求被降级为 CPU"
            devices = ["cpu"]
        else:
            devices = ["mps"]
    elif requested == "cpu":
        devices = ["cpu"]
    else:  # auto
        devices = ["mps", "cpu"] if mps_avail else ["cpu"]
        if not mps_avail:
            degraded = True

    if skip_cpu:
        devices = [d for d in devices if d != "cpu"]
    if skip_mps:
        devices = [d for d in devices if d != "mps"]
    if not devices:
        devices = ["cpu"]

    if devices == ["cpu"] and requested == "auto" and not mps_avail:
        degraded = True

    info = {
        "requested": requested,
        "devices_run": devices,
        "mps_built": mps_built,
        "mps_available": mps_avail,
        "degraded_to_cpu": degraded,
        "degradation_reason": mps_reason,
    }
    return devices, info


def device_sync(device: str) -> None:
    if device == "mps":
        torch.mps.synchronize()
    elif device == "cuda":
        torch.cuda.synchronize()


def percentile(sorted_vals: list[float], q: float) -> float:
    """线性插值百分位（不依赖 numpy）。q in [0, 100]。"""
    if not sorted_vals:
        return float("nan")
    if len(sorted_vals) == 1:
        return sorted_vals[0]
    rank = (q / 100.0) * (len(sorted_vals) - 1)
    lo = int(rank)
    hi = min(lo + 1, len(sorted_vals) - 1)
    frac = rank - lo
    return sorted_vals[lo] * (1.0 - frac) + sorted_vals[hi] * frac


def summarize(samples_ms: list[float]) -> dict:
    s = sorted(samples_ms)
    return {
        "n": len(s),
        "mean_ms": round(statistics.fmean(s), 3),
        "p50_ms": round(percentile(s, 50), 3),
        "p95_ms": round(percentile(s, 95), 3),
        "p99_ms": round(percentile(s, 99), 3),
        "min_ms": round(s[0], 3),
        "max_ms": round(s[-1], 3),
    }


def load_images(images_dir: Path, names: list[str]) -> list[Image.Image]:
    out = []
    for n in names:
        with Image.open(images_dir / n) as im:
            out.append(im.convert("RGB"))
    return out


def encode_image_preprocessed(model, processor, device: str, images, n: int) -> list[float]:
    """预处理已缓存的 pixel_values -> 单张 forward 延迟（纯后端算力对比）。"""
    tensors = []
    for im in images:
        px = processor(images=im, return_tensors="pt").pixel_values.to(device)
        tensors.append(px)

    for _ in range(WARMUP_ITERATIONS):
        with torch.inference_mode():
            model.get_image_features(pixel_values=tensors[0])
    device_sync(device)

    lat = []
    for i in range(n):
        px = tensors[i % len(tensors)]
        device_sync(device)
        t0 = time.perf_counter()
        with torch.inference_mode():
            model.get_image_features(pixel_values=px)
        device_sync(device)
        lat.append((time.perf_counter() - t0) * 1000.0)
    return lat


def encode_image_end_to_end(model, processor, device: str, raw_paths: list[Path],
                            n: int) -> list[float]:
    """原始字节 -> decode -> preprocess -> forward（服务端真实每请求成本）。"""
    payloads = []
    for p in raw_paths:
        payloads.append(p.read_bytes())

    def one(data: bytes) -> None:
        with Image.open(__import__("io").BytesIO(data)) as im:
            px = processor(images=im.convert("RGB"), return_tensors="pt").pixel_values.to(device)
        with torch.inference_mode():
            model.get_image_features(pixel_values=px)

    for _ in range(WARMUP_ITERATIONS):
        one(payloads[0])
    device_sync(device)

    lat = []
    for i in range(n):
        data = payloads[i % len(payloads)]
        device_sync(device)
        t0 = time.perf_counter()
        one(data)
        device_sync(device)
        lat.append((time.perf_counter() - t0) * 1000.0)
    return lat


def encode_text_latency(model, processor, device: str, texts: list[str], n: int) -> list[float]:
    encs = [processor(text=t, return_tensors="pt", padding=True, truncation=True)
            for t in texts]
    encs = [{k: v.to(device) for k, v in e.items()} for e in encs]

    for _ in range(WARMUP_ITERATIONS):
        with torch.inference_mode():
            model.get_text_features(**encs[0])
    device_sync(device)

    lat = []
    for i in range(n):
        e = encs[i % len(encs)]
        device_sync(device)
        t0 = time.perf_counter()
        with torch.inference_mode():
            model.get_text_features(**e)
        device_sync(device)
        lat.append((time.perf_counter() - t0) * 1000.0)
    return lat


def encode_image_batch(model, processor, device: str, images, batch_size: int) -> dict:
    """整批 wall time + embedding 维度/L2 校验。"""
    all_embeds = []
    for _ in range(max(1, batch_size // 2)):  # 预热一个批次
        pass
    with torch.inference_mode():
        px = processor(images=images[:batch_size], return_tensors="pt",
                       padding=True).pixel_values.to(device)
        model.get_image_features(pixel_values=px)  # 预热
    device_sync(device)

    t0 = time.perf_counter()
    with torch.inference_mode():
        for i in range(0, len(images), batch_size):
            batch = images[i:i + batch_size]
            px = processor(images=batch, return_tensors="pt", padding=True).pixel_values.to(device)
            feats = model.get_image_features(pixel_values=px)
            if hasattr(feats, "pooler_output") and feats.pooler_output is not None:
                feats = feats.pooler_output
            all_embeds.append(feats.cpu())
    device_sync(device)
    wall = time.perf_counter() - t0

    emb = torch.cat(all_embeds, dim=0)
    norms = emb.norm(dim=-1)
    return {
        "n_images": len(images),
        "batch_size": batch_size,
        "wall_time_s": round(wall, 3),
        "images_per_s": round(len(images) / wall, 3),
        "embedding_dim": int(emb.shape[-1]),
        "l2_norm_mean": round(float(norms.mean()), 6),
        "l2_norm_min": round(float(norms.min()), 6),
        "l2_norm_max": round(float(norms.max()), 6),
    }


def check_embedding_dim(model, processor, device: str, image, text: str) -> dict:
    with torch.inference_mode():
        px = processor(images=image, return_tensors="pt").pixel_values.to(device)
        out = model.get_image_features(pixel_values=px)
        if hasattr(out, "pooler_output") and out.pooler_output is not None:
            out = out.pooler_output
        img_dim = int(out.shape[-1])
        img_norm = float(out.norm(dim=-1).mean())

        te = processor(text=text, return_tensors="pt", padding=True, truncation=True)
        te = {k: v.to(device) for k, v in te.items()}
        tout = model.get_text_features(**te)
        if hasattr(tout, "pooler_output") and tout.pooler_output is not None:
            tout = tout.pooler_output
        txt_dim = int(tout.shape[-1])
        txt_norm = float(tout.norm(dim=-1).mean())
    return {"image_dim": img_dim, "text_dim": txt_dim,
            "image_l2_norm": round(img_norm, 6), "text_l2_norm": round(txt_norm, 6)}


# ── 主流程 ──────────────────────────────────────────────────────────────────

def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parent.parent
    os.chdir(root)

    data_path = Path(args.data)
    images_dir = Path(args.images_dir)
    with open(data_path, encoding="utf-8") as f:
        annotations = json.load(f)

    image_names = [a["image"] for a in annotations]
    missing = [n for n in image_names if not (images_dir / n).exists()]
    if missing:
        print(f"[ERROR] {len(missing)} 张标注图片缺失，示例: {missing[:3]}", file=sys.stderr)
        return 1

    devices, degradation = resolve_devices(args.device, args.skip_cpu, args.skip_mps)
    print(f"[INFO] 模型: {args.model}")
    print(f"[INFO] torch {torch.__version__} | MPS built={degradation['mps_built']} "
          f"available={degradation['mps_available']}")
    print(f"[INFO] 请求设备={args.device} 实际运行={degradation['devices_run']} "
          f"降级CPU={degradation['degraded_to_cpu']}")

    t_load0 = time.perf_counter()
    model = CLIPModel.from_pretrained(args.model)
    processor = CLIPProcessor.from_pretrained(args.model)
    model.eval()
    load_s = time.perf_counter() - t_load0
    print(f"[INFO] 模型加载耗时 {load_s:.2f}s")

    all_images = load_images(images_dir, image_names)
    raw_paths = [images_dir / n for n in image_names]
    texts = [a["text"] for a in annotations][: args.text_samples]
    print(f"[INFO] 图片 {len(all_images)} 张 | 文本采样 {len(texts)} 条")

    report: dict = {
        "meta": {
            "task": "T-026 本地 CLIP 部署评估 — POC 第一阶段",
            "timestamp_utc": datetime.now(timezone.utc).isoformat(),
            "chip": platform.processor() or platform.machine(),
            "os": f"{platform.system()} {platform.release()} ({platform.platform()})",
            "memory_bytes": os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES"),
            "python": platform.python_version(),
            "torch": torch.__version__,
            "transformers": __import__("transformers").__version__,
            "pillow": __import__("PIL").__version__,
            "model_id": args.model,
            "dataset": {"images": len(all_images), "annotations": len(annotations),
                        "images_dir": str(images_dir), "annotations_file": str(data_path)},
            "model_load_s": round(load_s, 3),
            "latency_samples": args.samples,
            "warmup_iterations": WARMUP_ITERATIONS,
        },
        "degradation": degradation,
        "device_checks": {},
        "results": {},
    }

    probe_img = all_images[0]
    probe_text = texts[0]

    for dev in devices:
        print(f"\n{'=' * 64}\n[设备] {dev}\n{'=' * 64}")
        model = model.to(dev)

        check = check_embedding_dim(model, processor, dev, probe_img, probe_text)
        report["device_checks"][dev] = check
        print(f"[CHECK] embedding dim image={check['image_dim']} text={check['text_dim']} "
              f"L2(image)={check['image_l2_norm']} L2(text)={check['text_l2_norm']}")

        single = encode_image_preprocessed(model, processor, dev, all_images, args.samples)
        e2e = encode_image_end_to_end(model, processor, dev, raw_paths, args.samples)
        txt = encode_text_latency(model, processor, dev, texts, args.samples)
        batch = encode_image_batch(model, processor, dev, all_images, args.batch_size)

        report["results"][dev] = {
            "image_latency_model_only": summarize(single),
            "image_latency_end_to_end": summarize(e2e),
            "text_latency": summarize(txt),
            "batch_throughput": batch,
        }
        r = report["results"][dev]
        print(f"[LAT] model_only  {json.dumps(r['image_latency_model_only'])}")
        print(f"[LAT] end_to_end  {json.dumps(r['image_latency_end_to_end'])}")
        print(f"[LAT] text        {json.dumps(r['text_latency'])}")
        print(f"[TPUT] {json.dumps(batch)}")

    # ── stdout 摘要表 ────────────────────────────────────────────────────────
    print("\n" + "=" * 78)
    print("T-026 CLIP POC 基准摘要")
    print("=" * 78)
    hdr = f"{'指标':<28}{'CPU (ms)':<24}{'MPS (ms)':<24}"
    print(hdr)
    print("-" * 78)

    def row(label: str, key: str, field: str) -> None:
        cells = []
        for d in ("cpu", "mps"):
            r = report["results"].get(d, {}).get(key)
            cells.append(f"{r[field]:.2f}" if r else "—")
        print(f"{label:<28}{cells[0]:<24}{cells[1]:<24}")

    for label, key in (("单张(模型)", "image_latency_model_only"),
                       ("单张(端到端)", "image_latency_end_to_end"),
                       ("文本编码", "text_latency")):
        row(f"{label} P50", key, "p50_ms")
        row(f"{label} P95", key, "p95_ms")
        row(f"{label} P99", key, "p99_ms")

    print("-" * 78)
    for d in ("cpu", "mps"):
        b = report["results"].get(d, {}).get("batch_throughput")
        if b:
            print(f"吞吐 {d.upper():<20} {b['n_images']} 张 / {b['wall_time_s']}s "
                  f"= {b['images_per_s']} img/s   dim={b['embedding_dim']} "
                  f"L2≈{b['l2_norm_mean']}")

    if "mps" in report["results"] and "cpu" in report["results"]:
        cpu_p50 = report["results"]["cpu"]["image_latency_model_only"]["p50_ms"]
        mps_p50 = report["results"]["mps"]["image_latency_model_only"]["p50_ms"]
        if cpu_p50 > 0:
            print(f"\nMPS vs CPU 单张 P50 加速比: {cpu_p50 / mps_p50:.2f}x")
        cpu_tput = report["results"]["cpu"]["batch_throughput"]["images_per_s"]
        mps_tput = report["results"]["mps"]["batch_throughput"]["images_per_s"]
        if cpu_tput > 0:
            print(f"MPS vs CPU 整批吞吐比:     {mps_tput / cpu_tput:.2f}x")

    if degradation["degraded_to_cpu"]:
        print(f"\n[降级] {degradation['degradation_reason']}")

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(report, f, ensure_ascii=False, indent=2)
    print(f"\n[OK] 结果已写入 {out_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
