---
name: fabric-query
version: 1.0.0
description: 面料查询 Skill，根据面料名称查询成分、适用季节、保养方式
long_description: |
  面料查询 Skill 是 Fashion AI Platform 的核心技能之一。
  设计师在设计过程中询问面料属性时，此 Skill 提供快速准确的面料信息查询。
  支持按名称、成分、季节等多维度查询，返回面料详细信息包括：
  - 基本属性（名称、成分比例、克重）
  - 适用季节和场景
  - 保养方式
  - 常见款式推荐
author: Fashion AI Team
tags: [fabric, consultation, fashion]
category: design-assistant
permissions:
  - skill:fabric-query
  - tool:search_fabric
  - tool:filter_by_season
  - tool:get_applicable_styles
  - data:fabric_database:read
dependencies: []
tools:
  - name: search_fabric
    description: 根据关键词搜索面料库，返回匹配的面料列表
    parameters:
      - name: query
        type: string
        required: true
        description: 搜索关键词（面料名称、成分关键词）
      - name: limit
        type: number
        required: false
        default: 10
        description: 返回结果数量限制
    returns:
      type: object
      description: 面料列表，每项包含 id、name、成分、适用季节

  - name: filter_by_season
    description: 按季节过滤面料，返回适合该季节的所有面料
    parameters:
      - name: season
        type: string
        required: true
        enum: [spring, summer, autumn, winter, all-season]
        description: 季节类型
    returns:
      type: object
      description: 适合指定季节的面料列表

  - name: get_applicable_styles
    description: 根据面料 ID 获取其适用的款式类型
    parameters:
      - name: fabric_id
        type: string
        required: true
        description: 面料 ID
    returns:
      type: object
      description: 适用款式列表及推荐理由
examples:
  - query: "纯棉面料适合做什么？"
    tools_used: [search_fabric, get_applicable_styles]
  - query: "夏天穿什么面料凉快？"
    tools_used: [filter_by_season]
---

# Fabric Query Skill — 面料查询技能

## 概述

面料查询 Skill 为服装设计师提供全面的面料信息咨询服务。当设计师询问面料属性、成分、适用季节或款式时，此 Skill 通过查询面料数据库返回专业建议。

## 工具

### search_fabric

根据关键词搜索面料库。适用于用户询问具体面料名称或成分的场景。

**使用场景**：
- "搜索纯棉面料"
- "有哪些涤纶面料？"
- "找轻薄的夏季面料"

### filter_by_season

按季节过滤面料。适用于用户询问特定季节适用面料的场景。

**使用场景**：
- "冬季适合什么面料？"
- "春秋季外套用什么面料好？"
- "四季都能穿的面料有哪些？"

### get_applicable_styles

根据面料特性推荐适用款式。适用于用户询问某面料适合做什么款式的场景。

**使用场景**：
- "这款面料适合做连衣裙吗？"
- "牛仔布一般做什么款式？"
- "雪纺面料适合什么场合？"

## 提示词模板

```
你是一个专业的面料顾问，熟悉各类纺织面料的特性、应用和保养方式。

当用户询问面料相关信息时，请使用以下工具提供准确答案：
- search_fabric：搜索面料库，获取面料基本信息
- filter_by_season：按季节筛选适合的面料
- get_applicable_styles：查询面料适用的款式类型

回答要点：
1. 清晰说明面料的成分比例和特性
2. 指出适用的季节和穿着场景
3. 推荐适合的款式类型
4. 提供基础的保养建议

如需更详细的面料信息，可结合多个工具提供综合建议。
```

## 面料数据字段

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 面料唯一标识 |
| name | string | 面料名称 |
| composition | object | 成分比例，如 { cotton: 80, polyester: 20 } |
| weight | string | 克重，如 "150-200g/m²" |
| season | array | 适用季节 |
| applicable_styles | array | 适用款式 |
| care_instructions | object | 保养说明 |
| features | array | 面料特性 |

## Changelog

### 1.0.0 (2024-01-15)
- 初始版本
- 支持面料搜索、季节过滤、款式推荐
