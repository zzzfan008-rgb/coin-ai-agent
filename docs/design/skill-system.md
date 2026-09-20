# Fashion AI Platform — SKILL 系统规范

> 本文档定义 Skill 的目录结构、SKILL.md 元数据格式、依赖管理和权限模型。

---

## 1. 目录结构规范

每个 Skill 是一个独立目录，位于仓库根 `skills/` 下：

```
skills/
└── {skill-id}/                # Skill ID，全小写、中划线分隔
    ├── SKILL.md               # 必需：Skill 元数据 + 描述
    ├── references/            # 可选：参考文档
    │   ├── fabric-database.md
    │   └── color-theory.md
    ├── prompts/               # 可选：提示词模板
    │   └── system-prompt.md
    ├── scripts/               # 可选：辅助脚本
    │   └── validate.sh
    └── assets/                # 可选：图标、图片
        └── icon.svg
```

### 命名规则

| 字段 | 规则 |
|------|------|
| 目录名 | 全小写，中划线分隔，如 `fabric-query` |
| Skill ID | 与目录名一致 |
| 文件名 | 蛇形或中划线，如 `SKILL.md` |

---

## 2. SKILL.md 元数据格式

SKILL.md 是 Skill 的入口文件，采用 YAML Frontmatter + Markdown 结构：

```markdown
---
name: {skill-id}               # Skill ID，全小写
version: {semver}              # 语义化版本，如 1.0.0
description: {一句话描述}       # 面向用户的简短说明
long_description: |             # 可选：详细说明
  多行描述，支持 Markdown 格式。
author: Fashion AI Team
tags: [fabric, consultation]   # 标签，用于分类和搜索
category: design-assistant     # 可选：技能分类
permissions:                   # 必需：权限声明
  - skill:{skill-id}           # Skill 自身权限
dependencies:                  # 可选：依赖的其他 Skill
  - skill:color-matching
tools:                         # 必需：暴露给 Agent 的工具
  - name: {tool-name}
    description: {说明}
    parameters:                # 可选：参数定义
      - name: param
        type: string
        required: true
---
```

### 必需字段

| 字段 | 类型 | 说明 |
|------|------|------|
| name | string | Skill ID |
| version | string | 语义化版本 |
| description | string | 简短描述（≤120 字符） |
| permissions | array | 权限声明列表 |
| tools | array | 工具定义 |

### 可选字段

| 字段 | 类型 | 说明 |
|------|------|------|
| long_description | string | 详细说明 |
| author | string | 作者 |
| tags | array | 标签 |
| category | string | 分类 |
| dependencies | array | 依赖的 Skill |
| examples | array | 使用示例 |

---

## 3. 权限模型

### 3.1 权限声明

每个 Skill 必须在 `permissions` 中声明自身权限：

```yaml
permissions:
  - skill:{skill-id}            # Skill 自身
  - tool:search_fabric          # 使用的工具
  - data:fabric_database:read   # 数据访问
```

### 3.2 权限层级

| 层级 | 格式 | 说明 |
|------|------|------|
| Skill 级 | `skill:{skill-id}` | 使用该 Skill 的权限 |
| 工具级 | `tool:{tool-name}` | 调用特定工具的权限 |
| 数据级 | `data:{resource}:{action}` | 数据读写权限 |

### 3.3 权限检查点

- **Skill 加载**：检查用户是否有 `skill:{skill-id}` 权限
- **工具调用**：检查用户是否有 `tool:{tool-name}` 权限
- **数据访问**：检查用户是否有 `data:{resource}:{action}` 权限

---

## 4. 依赖管理

### 4.1 Skill 依赖

Skill 可以声明对其他 Skill 的依赖：

```yaml
dependencies:
  - skill: color-matching       # 依赖色彩搭配 Skill
```

### 4.2 工具依赖

Skill 声明的工具由运行时提供，Skill 本身不管理工具实现。

### 4.3 版本约束

| 符号 | 含义 | 示例 |
|------|------|------|
| `^` | 兼容版本 | `^1.0.0` |
| `~` | 补丁版本 | `~1.0.0` |
| `>=` | 最低版本 | `>=1.0.0` |

---

## 5. 工具定义规范

每个 Skill 的 `tools` 数组定义其暴露给 Agent 的工具：

```yaml
tools:
  - name: {tool-name}           # 工具名，蛇形命名
    description: {说明}          # Agent 理解工具用途
    parameters:                 # 参数定义
      - name: {param}
        type: string | number | boolean | object | array
        required: true | false
        default: {值}
        enum: [{值1}, {值2}]
    returns:                    # 可选：返回值说明
      type: object
      description: {说明}
```

### 工具命名规则

- 工具名使用蛇形命名：`search_fabric`
- 动词 + 名词结构：`search_*, filter_*, get_*, generate_*`

---

## 6. 版本管理

### 6.1 版本号规则

遵循 Semantic Versioning（语义化版本）：
- **MAJOR**：破坏性变更
- **MINOR**：新增功能（向后兼容）
- **PATCH**：修复（向后兼容）

### 6.2 变更记录

在 SKILL.md 末尾维护变更日志：

```markdown
## Changelog

### 1.1.0 (2024-01-15)
- 新增：`get_applicable_styles` 工具
- 优化：扩展面料数据库覆盖范围

### 1.0.0 (2024-01-01)
- 初始版本
```

---

## 7. 注册与发现

### 7.1 Skill 注册

在 `skills/` 目录下创建 Skill 目录并添加 SKILL.md，即完成注册。

### 7.2 Skill 列表 API

```
GET /api/skills

Response:
{
  "skills": [
    {
      "id": "fabric-query",
      "name": "面料查询",
      "version": "1.0.0",
      "description": "面料成分、季节、适用款式查询",
      "tags": ["fabric", "consultation"]
    }
  ]
}
```

### 7.3 Skill 详情 API

```
GET /api/skills/:id

Response:
{
  "id": "fabric-query",
  "name": "fabric-query",
  "version": "1.0.0",
  "description": "...",
  "tools": [...],
  "permissions": [...]
}
```

---

## 8. 开发工作流

1. **创建 Skill**：`skills/{skill-id}/SKILL.md`
2. **定义工具**：在 `tools` 数组中声明工具
3. **编写提示词**：在 `prompts/system-prompt.md` 中编写系统提示
4. **添加参考文档**：在 `references/` 目录添加参考资料
5. **版本发布**：更新版本号，添加变更记录
