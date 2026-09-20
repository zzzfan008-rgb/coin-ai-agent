---
name: color-matching
version: 1.0.0
description: 色彩搭配建议 Skill，基于主色推荐搭配色、分析色彩和谐性、提供当季流行色
long_description: |
  色彩搭配 Skill 是 Fashion AI Platform 的创意辅助技能。
  设计师在配色阶段需要专业建议时，此 Skill 提供：
  - 基于主色的配色方案推荐
  - 色彩和谐性分析（色相、饱和度、明度）
  - 当季流行色趋势参考
  - 特定场景的配色建议（如正式场合、休闲风格）
author: Fashion AI Team
tags: [color, matching, design, fashion]
category: design-assistant
permissions:
  - skill:color-matching
  - tool:suggest_palette
  - tool:color_harmony
  - tool:trend_colors
  - data:color_database:read
dependencies: []
tools:
  - name: suggest_palette
    description: 基于主色推荐完整的配色方案
    parameters:
      - name: primary_color
        type: string
        required: true
        description: 主色，HEX 格式（如 #6366F1）或颜色名称
      - name: palette_type
        type: string
        required: false
        default: complementary
        enum: [complementary, analogous, triadic, split-complementary, monochromatic, custom]
        description: 配色方案类型
      - name: style
        type: string
        required: false
        description: 风格关键词，如 casual、formal、vintage
    returns:
      type: object
      description: 配色方案，包含主色和搭配色列表及 HEX 值

  - name: color_harmony
    description: 分析一组颜色的和谐程度
    parameters:
      - name: colors
        type: array
        required: true
        description: 待分析的颜色列表，HEX 格式
    returns:
      type: object
      description: 和谐性评分、分析说明、调整建议

  - name: trend_colors
    description: 获取当季流行色彩趋势
    parameters:
      - name: season
        type: string
        required: false
        description: 季节，不填则返回当前季节趋势
      - name: category
        type: string
        required: false
        enum: [apparel, accessories, all]
        default: apparel
        description: 品类分类
    returns:
      type: object
      description: 当季流行色列表及趋势说明
examples:
  - query: "蓝色系怎么搭配？"
    tools_used: [suggest_palette]
  - query: "这个配色方案和谐吗？"
    tools_used: [color_harmony]
  - query: "2024 秋冬流行什么颜色？"
    tools_used: [trend_colors]
---

# Color Matching Skill — 色彩搭配技能

## 概述

色彩搭配 Skill 为服装设计师提供专业的配色建议和色彩分析服务。从确定主色到完成整套配色方案，此 Skill 提供全流程的色彩咨询服务。

## 工具

### suggest_palette

基于主色推荐完整的配色方案。支持多种配色逻辑，适用于不同设计风格需求。

**配色方案类型**：

| 类型 | 说明 | 适用场景 |
|------|------|----------|
| complementary | 互补色 | 强对比、时尚感 |
| analogous | 类似色 | 和谐、柔和 |
| triadic | 三色配色 | 丰富、平衡 |
| split-complementary | 分裂互补 | 现代感、温和对比 |
| monochromatic | 单色系 | 简约、高级感 |

**使用场景**：
- "帮我搭配蓝色系的服装配色"
- "正式场合的正装配色方案"
- "复古风格的配色推荐"

### color_harmony

分析一组颜色的和谐程度。帮助设计师评估配色方案的视觉效果。

**和谐性维度**：
- 色相差（色相环上的距离）
- 饱和度平衡
- 明度对比
- 整体协调度（0-100 分）

**使用场景**：
- "检查一下这个配色方案"
- "这三个颜色放在一起好看吗？"
- "需要调整哪个颜色？"

### trend_colors

获取当季流行色彩趋势。为设计提供市场导向的配色参考。

**使用场景**：
- "2024 秋冬流行什么颜色？"
- "今年春夏的流行色有哪些？"
- "配饰流行什么颜色？"

## 提示词模板

```
你是一个专业的色彩顾问，精通色彩理论和时尚配色趋势。

当用户询问配色建议时，请使用以下工具：
- suggest_palette：基于主色推荐配色方案
- color_harmony：分析现有配色的和谐性
- trend_colors：提供当季流行色参考

回答要点：
1. 理解用户的配色需求（主色、风格、场合）
2. 推荐合适的配色方案并说明理由
3. 提供 HEX 色值便于实际应用
4. 必要时给出配色调整建议

专业术语使用说明：
- 色相（Hue）：颜色的基本属性
- 饱和度（Saturation）：颜色的纯度
- 明度（Lightness）：颜色的明暗程度
```

## 色彩数据字段

| 字段 | 类型 | 说明 |
|------|------|------|
| hex | string | HEX 色值 |
| name | string | 颜色名称 |
| rgb | object | RGB 值 { r, g, b } |
| hsl | object | HSL 值 { h, s, l } |
| category | string | 色彩分类（冷色/暖色/中性色） |
| season | string | 所属季节趋势 |

## Changelog

### 1.0.0 (2024-01-15)
- 初始版本
- 支持配色方案推荐、色彩和谐分析、流行色查询
