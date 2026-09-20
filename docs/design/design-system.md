# Fashion AI Platform — 设计系统

> 本文档定义 Fashion AI Platform 的视觉设计规范，供前端开发、UI 评审和质量检查使用。

---

## 1. 品牌概述

- **产品名称**：Fashion AI Platform
- **产品定位**：服装设计行业本地部署 AI SaaS，面向 20 人设计团队
- **设计关键词**：专业、简洁、创意、智能化

---

## 2. 色彩系统

### 2.1 品牌主色

| 名称 | 色值 | 用途 |
|------|------|------|
| 主色 / Primary | `#6366F1` | 按钮、链接、选中态、高亮 |
| 主色深 / Primary Dark | `#4F46E5` | 按钮悬停、强调 |
| 主色浅 / Primary Light | `#A5B4FC` | 背景、标签 |

### 2.2 功能色

| 名称 | 色值 | 用途 |
|------|------|------|
| 成功 / Success | `#10B981` | 成功提示、正向反馈 |
| 警告 / Warning | `#F59E0B` | 警告提示 |
| 错误 / Error | `#EF4444` | 错误提示、表单校验失败 |
| 信息 / Info | `#3B82F6` | 信息提示 |

### 2.3 中性色

| 名称 | 色值 | 用途 |
|------|------|------|
| 背景 / Background | `#0F172A` | 深色主题主背景 |
| 表面 / Surface | `#1E293B` | 卡片、对话框背景 |
| 边框 / Border | `#334155` | 分割线、输入框边框 |
| 占位符 / Placeholder | `#64748B` | 输入框占位符文字 |
| 次要文字 / Text Secondary | `#94A3B8` | 辅助说明文字 |
| 主要文字 / Text Primary | `#F1F5F9` | 正文、标题 |

### 2.4 深色主题色板（默认）

```css
:root {
  --color-bg: #0F172A;
  --color-surface: #1E293B;
  --color-surface-elevated: #273449;
  --color-border: #334155;
  --color-text-primary: #F1F5F9;
  --color-text-secondary: #94A3B8;
  --color-text-muted: #64748B;
  --color-primary: #6366F1;
  --color-primary-hover: #4F46E5;
  --color-primary-light: #A5B4FC;
  --color-success: #10B981;
  --color-warning: #F59E0B;
  --color-error: #EF4444;
  --color-info: #3B82F6;
}
```

---

## 3. 字体规范

### 3.1 字体栈

| 用途 | 字体 | 回退 |
|------|------|------|
| 中文字体 | `"Noto Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif` | 系统默认无衬线 |
| 英文字体 | `"Inter", "SF Pro Display", system-ui, sans-serif` | 系统默认无衬线 |
| 代码字体 | `"JetBrains Mono", "Fira Code", monospace` | 等宽 |

### 3.2 字号系统

| 级别 | 字号 | 行高 | 用途 |
|------|------|------|------|
| Display | 32px | 1.2 | 大标题 |
| H1 | 24px | 1.3 | 页面标题 |
| H2 | 20px | 1.4 | 区块标题 |
| H3 | 16px | 1.5 | 卡片标题 |
| Body | 14px | 1.6 | 正文 |
| Small | 12px | 1.5 | 辅助说明 |
| Caption | 11px | 1.4 | 标签、时间戳 |

### 3.3 字重

| 字重 | 值 | 用途 |
|------|------|------|
| Regular | 400 | 正文 |
| Medium | 500 | 按钮、标签 |
| Semibold | 600 | 标题、强调 |

---

## 4. 组件规范

### 4.1 按钮（Button）

| 变体 | 背景色 | 文字色 | 边框 | 用途 |
|------|--------|--------|------|------|
| Primary | `#6366F1` | `#FFFFFF` | 无 | 主要操作 |
| Primary Hover | `#4F46E5` | `#FFFFFF` | 无 | 悬停态 |
| Secondary | 透明 | `#A5B4FC` | 1px `#334155` | 次要操作 |
| Ghost | 透明 | `#94A3B8` | 无 | 辅助操作 |
| Danger | `#EF4444` | `#FFFFFF` | 无 | 危险操作 |

**规格**：
- 高度：36px（默认）/ 32px（小）/ 44px（大）
- 圆角：8px
- 内边距：12px 16px
- 字体：14px Medium
- 禁用：不透明度 50%，cursor: not-allowed

### 4.2 输入框（Input）

**规格**：
- 高度：40px
- 背景：`#1E293B`
- 边框：1px `#334155`，聚焦时 `#6366F1`
- 圆角：8px
- 内边距：10px 14px
- 字体：14px
- 错误态：边框 `#EF4444`

### 4.3 卡片（Card）

**规格**：
- 背景：`#1E293B`
- 边框：1px `#334155`
- 圆角：12px
- 内边距：20px
- 阴影：`0 4px 6px rgba(0,0,0,0.3)`

### 4.4 对话框（Dialog）

**规格**：
- 背景：`#1E293B`
- 遮罩：`rgba(0,0,0,0.6)`
- 圆角：16px
- 最大宽度：560px
- 内边距：24px
- 标题：20px Semibold
- 关闭按钮：右上角，24px 圆形

### 4.5 标签（Tag）

**规格**：
- 高度：24px
- 圆角：6px
- 内边距：4px 10px
- 字体：12px Medium
- 颜色变体：Primary、Success、Warning、Error

---

## 5. 布局规范

### 5.1 网格系统

- 基础网格：8px
- 容器最大宽度：1280px
- 边距：24px（桌面）/ 16px（移动端）
- 列数：12 列

### 5.2 间距系统

| 名称 | 值 | 用途 |
|------|------|------|
| xs | 4px | 紧凑元素间 |
| sm | 8px | 组件内部 |
| md | 16px | 组件之间 |
| lg | 24px | 区块之间 |
| xl | 32px | 大区块之间 |
| 2xl | 48px | 页面级区块 |

### 5.3 响应式断点

| 断点 | 宽度 | 用途 |
|------|------|------|
| sm | 640px | 平板竖屏 |
| md | 768px | 平板横屏 |
| lg | 1024px | 笔记本 |
| xl | 1280px | 桌面 |

---

## 6. 图标规范

### 6.1 图标库

- **主图标库**：Lucide Icons（开源、一致性强）
- **图标尺寸**：16px / 20px / 24px（默认）
- **图标颜色**：跟随文字颜色（currentColor）

### 6.2 图标命名规范

```
icon-{category}-{name}.svg
icon-action-send.svg
icon-nav-chat.svg
icon-status-success.svg
```

### 6.3 常用图标

| 用途 | 图标名称 |
|------|----------|
| 发送消息 | `icon-action-send` |
| 导航对话 | `icon-nav-chat` |
| 会话列表 | `icon-nav-sessions` |
| 技能选择 | `icon-nav-skills` |
| 设置 | `icon-nav-settings` |
| 成功状态 | `icon-status-success` |
| 错误状态 | `icon-status-error` |
| 加载中 | `icon-status-loading` |
| 搜索 | `icon-action-search` |
| 关闭 | `icon-action-close` |
| 展开/折叠 | `icon-action-expand` |

---

## 7. 动效规范

### 7.1 时长

| 类型 | 时长 |
|------|------|
| 微交互 | 150ms |
| 展开/折叠 | 250ms |
| 页面过渡 | 300ms |
| 加载动画 | 循环 |

### 7.2 缓动函数

| 类型 | 曲线 | 用途 |
|------|------|------|
| 标准 | `ease-out` | 大多数过渡 |
| 进入 | `ease-in-out` | 展开动画 |
| 弹性 | `cubic-bezier(0.34, 1.56, 0.64, 1)` | 按钮点击反馈 |

### 7.3 加载状态

- 使用旋转圆环（Ring）或点脉冲（Pulse）
- 颜色：`#6366F1`
- 尺寸：20px（内联）/ 40px（页面级）

---

## 8. 暗色主题（默认）

Fashion AI Platform 默认使用暗色主题，所有组件和布局均基于上述深色色板定义。

如需亮色主题适配，请参考 Neutral 色板反转规则：
- 背景：明色替代深色
- 文字：深色替代浅色
- Surface 上浮 1-2 级
