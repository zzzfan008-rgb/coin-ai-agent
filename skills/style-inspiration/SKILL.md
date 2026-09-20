---
name: style-inspiration
version: 1.0.0
description: 款式灵感生成 Skill，基于关键词生成款式创意、提供款式变体、分析流行趋势
long_description: |
  款式灵感 Skill 是 Fashion AI Platform 的创意生成技能。
  设计师需要灵感突破或款式参考时，此 Skill 提供：
  - 基于关键词的款式创意生成
  - 已有款式的变体设计建议
  - 流行趋势分析和应用建议
  - 不同场景的款式推荐
author: Fashion AI Team
tags: [style, inspiration, design, creative]
category: design-assistant
permissions:
  - skill:style-inspiration
  - tool:generate_style_ideas
  - tool:style_variations
  - tool:trend_analysis
  - data:style_database:read
dependencies: []
tools:
  - name: generate_style_ideas
    description: 基于关键词生成款式设计灵感
    parameters:
      - name: keywords
        type: array
        required: true
        description: 关键词列表，如 ["休闲", "宽松", "春夏"]
      - name: garment_type
        type: string
        required: false
        description: 服装类型，如 dress、jacket、pants
      - name: count
        type: number
        required: false
        default: 5
        description: 生成灵感数量
    returns:
      type: object
      description: 款式灵感列表，每项包含描述、设计要点、参考要素

  - name: style_variations
    description: 为已有款式生成变体设计
    parameters:
      - name: base_style
        type: string
        required: true
        description: 基础款式描述或名称
      - name: variation_type
        type: string
        required: false
        enum: [silhouette, detail, material, color, all]
        default: all
        description: 变体类型
      - name: count
        type: number
        required: false
        default: 3
        description: 生成变体数量
    returns:
      type: object
      description: 款式变体列表，每项包含变体描述和设计要点

  - name: trend_analysis
    description: 分析当前流行趋势并提供应用建议
    parameters:
      - name: category
        type: string
        required: false
        enum: [silhouette, detail, material, color, all]
        default: all
        description: 分析类别
      - name: region
        type: string
        required: false
        description: 地区，不填则返回全球趋势
    returns:
      type: object
      description: 趋势分析报告，包含趋势描述、热度指数、应用建议
examples:
  - query: "给我一些春夏连衣裙的灵感"
    tools_used: [generate_style_ideas]
  - query: "这款西装外套可以怎么变体设计？"
    tools_used: [style_variations]
  - query: "今年秋冬外套流行什么款式？"
    tools_used: [trend_analysis]
---

# Style Inspiration Skill — 款式灵感技能

## 概述

款式灵感 Skill 为服装设计师提供创意生成和趋势分析服务。当设计师需要突破创意瓶颈、寻找款式变体或了解市场趋势时，此 Skill 提供专业建议和灵感参考。

## 工具

### generate_style_ideas

基于关键词生成款式设计灵感。帮助设计师快速获得创意方向。

**使用场景**：
- "给我一些运动休闲风格的灵感"
- "春夏季节适合什么款式的上衣？"
- "职场穿搭的款式建议"

**生成内容**：
- 款式描述（整体造型）
- 设计要点（轮廓、比例、细节）
- 参考要素（面料、色彩、元素）
- 适用场景说明

### style_variations

为已有款式生成变体设计。帮助设计师在经典款式基础上创新。

**变体类型**：

| 类型 | 说明 | 示例 |
|------|------|------|
| silhouette | 轮廓变化 | A字型→H型 |
| detail | 细节变化 | 口袋、领型、袖口 |
| material | 面料变化 | 棉→丝绸 |
| color | 色彩变化 | 深色→浅色 |
| all | 综合变体 | 多维度变化 |

**使用场景**：
- "这款风衣的变体设计"
- "如何改造经典款式？"
- "给这个款式换个面料会怎样？"

### trend_analysis

分析当前流行趋势并提供应用建议。为设计决策提供市场导向参考。

**趋势维度**：
- 轮廓趋势（Oversized、修身等）
- 细节趋势（荷叶边、拼接等）
- 面料趋势（可持续面料、功能性面料）
- 色彩趋势（年度流行色）

**使用场景**：
- "2024 秋冬外套趋势"
- "今年流行的领型是什么？"
- "可持续面料的应用趋势"

## 提示词模板

```
你是一个资深的服装款式设计师，熟悉各类服装的造型规律和设计趋势。

当用户需要款式灵感时，请使用以下工具：
- generate_style_ideas：基于关键词生成款式创意
- style_variations：为已有款式生成变体
- trend_analysis：分析流行趋势

回答要点：
1. 理解用户的设计需求和偏好
2. 提供具体、可执行的款式建议
3. 说明设计要点和实现方式
4. 适当引用趋势作为参考依据
5. 考虑实际可行性（工艺、面料、成本）

创意生成时，可以结合：
- 廓形（Silhouette）：A型、H型、O型、X型
- 细节元素：领型、袖型、口袋、门襟
- 面料选择：材质特性与款式的匹配
- 色彩搭配：整体造型的色彩规划
```

## 款式数据字段

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 款式唯一标识 |
| name | string | 款式名称 |
| description | string | 款式描述 |
| silhouette | string | 廓形类型 |
| garment_type | string | 服装类型 |
| key_features | array | 关键设计特征 |
| suitable_seasons | array | 适用季节 |
| target_audience | string | 目标人群 |
| price_range | string | 价格定位 |

## Changelog

### 1.0.0 (2024-01-15)
- 初始版本
- 支持款式灵感生成、变体设计、趋势分析
