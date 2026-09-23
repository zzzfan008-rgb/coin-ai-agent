#!/usr/bin/env python3
"""
CLIP 评测脚本 - T-024
基于真实服装数据集（82条）评估 CLIP 图像-文本匹配准确性。

用法:
    python scripts/eval_clip.py [--limit N] [--model MODEL_ID] [--output OUTPUT_JSON]

指标:
    - Top-1 准确率: text_score > hard_negative_score 为正确
    - 每品类/风格/颜色分项准确率
"""

import argparse
import json
import sys
from pathlib import Path
from collections import defaultdict

import torch
from PIL import Image
from transformers import CLIPProcessor, CLIPModel


def parse_args():
    parser = argparse.ArgumentParser(description="CLIP 服装评测")
    parser.add_argument(
        "--model",
        default="openai/clip-vit-base-patch32",
        help="HuggingFace 模型 ID (默认: openai/clip-vit-base-patch32)",
    )
    parser.add_argument(
        "--data",
        default="data/annotations.json",
        help="标注文件路径 (默认: data/annotations.json)",
    )
    parser.add_argument(
        "--images-dir",
        default="data/images",
        help="图片目录路径 (默认: data/images)",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help="限制评测条数（用于 CPU 快速验证，默认: None 全量）",
    )
    parser.add_argument(
        "--output",
        default="data/eval_results.json",
        help="结果 JSON 输出路径 (默认: data/eval_results.json)",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=8,
        help="批处理大小 (默认: 8)",
    )
    return parser.parse_args()


def load_model(model_id: str):
    """加载 CLIP 模型和 processor"""
    print(f"[INFO] 加载模型: {model_id}")
    device = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"[INFO] 设备: {device}")

    model = CLIPModel.from_pretrained(model_id)
    processor = CLIPProcessor.from_pretrained(model_id)
    model = model.to(device)
    model.eval()
    return model, processor, device


def load_data(annotations_path: str, images_dir: str, limit: int | None) -> list[dict]:
    """加载并验证数据集"""
    with open(annotations_path, encoding="utf-8") as f:
        data = json.load(f)

    if limit:
        data = data[:limit]

    images_path = Path(images_dir)
    validated = []
    for item in data:
        img_file = images_path / item["image"]
        if img_file.exists():
            validated.append(item)
        else:
            print(f"[WARN] 图片不存在: {img_file}, 跳过")

    print(f"[INFO] 加载 {len(validated)} 条数据 (共 {len(data)} 条, 跳过 {len(data) - len(validated)} 条)")
    return validated


def _normalize(t: torch.Tensor) -> torch.Tensor:
    """L2 normalize a tensor along the last dimension."""
    return t / t.norm(dim=-1, keepdim=True)


def _get_pooled(model, inputs: dict) -> torch.Tensor:
    """Extract pooled output (512-dim) from CLIP model, compatible with transformers>=5.0."""
    with torch.no_grad():
        out = model.get_image_features(**inputs) if "pixel_values" in inputs else model.get_text_features(**inputs)
    # transformers >= 5.0 returns BaseModelOutputWithPooling with pooler_output
    pooled = out.pooler_output if hasattr(out, "pooler_output") else out
    return _normalize(pooled)


def compute_embeddings_batch(
    model, processor, device, images: list[Image.Image], texts: list[str], batch_size: int
):
    """批量计算图像和文本 embedding"""
    all_image_embeds = []
    all_text_embeds = []

    # 分批处理图像
    for i in range(0, len(images), batch_size):
        batch_imgs = images[i : i + batch_size]
        inputs = processor(images=batch_imgs, return_tensors="pt", padding=True)
        inputs = {k: v.to(device) for k, v in inputs.items()}
        all_image_embeds.append(_get_pooled(model, inputs).cpu())

    # 分批处理文本
    for i in range(0, len(texts), batch_size):
        batch_texts = texts[i : i + batch_size]
        inputs = processor(text=batch_texts, return_tensors="pt", padding=True, truncation=True)
        inputs = {k: v.to(device) for k, v in inputs.items()}
        all_text_embeds.append(_get_pooled(model, inputs).cpu())

    image_embeds = torch.cat(all_image_embeds, dim=0) if all_image_embeds else torch.empty(0)
    text_embeds = torch.cat(all_text_embeds, dim=0) if all_text_embeds else torch.empty(0)
    return image_embeds, text_embeds


def cosine_similarity_batch(a: torch.Tensor, b: torch.Tensor) -> torch.Tensor:
    """批量余弦相似度 (a @ b.T) -> (n, m)"""
    return a @ b.T


def main():
    args = parse_args()

    # 确保输出目录存在
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)

    # 加载数据
    data = load_data(args.data, args.images_dir, args.limit)

    # 加载模型
    model, processor, device = load_model(args.model)

    # 收集所有图像和文本
    images: list[Image.Image] = []
    texts: list[str] = []
    hard_negatives: list[str] = []

    for item in data:
        img_path = Path(args.images_dir) / item["image"]
        try:
            img = Image.open(img_path).convert("RGB")
        except Exception as e:
            print(f"[ERROR] 无法打开图片 {img_path}: {e}")
            continue
        images.append(img)
        texts.append(item["text"])
        hard_negatives.append(item["hard_negative"])

    # 批量计算所有 embedding
    print("[INFO] 计算图像 embedding...")
    image_embeds, _ = compute_embeddings_batch(
        model, processor, device, images, [], args.batch_size
    )
    print("[INFO] 计算文本 embedding (text + hard_negative)...")
    all_texts = texts + hard_negatives
    _, text_embeds = compute_embeddings_batch(
        model, processor, device, [], all_texts, args.batch_size
    )

    n = len(texts)
    text_embeds_np = text_embeds[:n]
    hn_embeds_np = text_embeds[n:]

    # 计算相似度
    text_scores = (image_embeds * text_embeds_np).sum(dim=-1).tolist()
    hn_scores = (image_embeds * hn_embeds_np).sum(dim=-1).tolist()
    correct = [t > n for t, n in zip(text_scores, hn_scores)]

    # 汇总每条记录
    results = []
    for i, item in enumerate(data):
        results.append(
            {
                "image": item["image"],
                "text": item["text"],
                "hard_negative": item["hard_negative"],
                "category": item["category"],
                "color": item["color"],
                "style": item["style"],
                "text_score": round(text_scores[i], 4),
                "hard_negative_score": round(hn_scores[i], 4),
                "correct": correct[i],
            }
        )

    # 统计
    total = len(results)
    correct_count = sum(1 for r in results if r["correct"])
    overall_accuracy = correct_count / total if total > 0 else 0

    # 按品类/风格/颜色统计
    by_category: dict[str, dict] = defaultdict(lambda: {"correct": 0, "total": 0})
    by_style: dict[str, dict] = defaultdict(lambda: {"correct": 0, "total": 0})
    by_color: dict[str, dict] = defaultdict(lambda: {"correct": 0, "total": 0})

    for r in results:
        by_category[r["category"]]["total"] += 1
        by_category[r["category"]]["correct"] += int(r["correct"])
        by_style[r["style"]]["total"] += 1
        by_style[r["style"]]["correct"] += int(r["correct"])
        by_color[r["color"]]["total"] += 1
        by_color[r["color"]]["correct"] += int(r["correct"])

    def build_sub_stats(d):
        return {
            k: {
                "total": v["total"],
                "correct": v["correct"],
                "accuracy": round(v["correct"] / v["total"], 4) if v["total"] > 0 else 0,
            }
            for k, v in sorted(d.items(), key=lambda x: x[1]["total"], reverse=True)
        }

    # 找出易错样本
    wrong_samples = sorted(
        [r for r in results if not r["correct"]],
        key=lambda x: (x["text_score"] - x["hard_negative_score"]),
    )

    output = {
        "summary": {
            "total": total,
            "correct": correct_count,
            "accuracy": round(overall_accuracy, 4),
            "model": args.model,
            "device": device,
        },
        "by_category": build_sub_stats(by_category),
        "by_style": build_sub_stats(by_style),
        "by_color": build_sub_stats(by_color),
        "wrong_samples": wrong_samples[:10],
        "records": results,
    }

    # 保存结果
    with open(args.output, "w", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, indent=2)

    # 打印摘要
    print(f"\n{'='*60}")
    print(f"评测完成! 准确率: {overall_accuracy:.2%} ({correct_count}/{total})")
    print(f"结果已保存: {args.output}")
    print(f"\n按品类准确率:")
    for k, v in list(output["by_category"].items())[:8]:
        print(f"  {k}: {v['accuracy']:.2%} ({v['correct']}/{v['total']})")
    print(f"\n易错样本 ({len(wrong_samples)} 条):")
    for r in wrong_samples[:5]:
        print(f"  [{r['category']}] {r['text'][:20]}... vs {r['hard_negative'][:20]}...")
        print(f"    text={r['text_score']:.3f} hn={r['hard_negative_score']:.3f} diff={r['text_score']-r['hard_negative_score']:.3f}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
