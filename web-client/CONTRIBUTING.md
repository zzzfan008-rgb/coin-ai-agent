# 前端开发指南

## 开发环境

```bash
npm install
npm run dev
```

## 工程化门禁

提交前必须运行全部门禁：

```bash
npm run gate
```

`gate` 依次执行：
1. `npm run typecheck` — TypeScript 类型检查（`tsc --noEmit`）
2. `npm run lint` — ESLint 代码风格检查
3. `npm run test` — Vitest 单元测试

## 测试

### 运行测试

```bash
# 全量测试（CI 用）
npm run test

# 增量测试（开发用，文件变更后自动重跑）
npm run test:watch
```

### 测试文件位置

- `src/__tests__/` — 现有组件测试（ChatInput, Login, MessageBubble）
- `tests/` — 新增单元测试
  - `tests/components/` — React 组件测试
  - `tests/hooks/` — Hook 测试（useChat 等）
  - `tests/utils/` — 纯函数/工具测试（token 管理等）

### 编写测试

- 使用 **Vitest** + **@testing-library/react**
- Mock 通过 `vi.mock()` + `vi.hoisted()` 声明
- 每个测试文件有 `setupTests.ts` 提供 jest-dom 扩展和 localStorage mock

### localStorage 在测试中

jsdom 环境默认无 localStorage，已通过 Node.js `--localstorage-file` flag 启用。
测试中的 `localStorage.clear()` 会在每次测试后自动清理。

## 代码规范

```bash
# 自动修复风格问题
npm run lint:fix
```

ESLint 规则：
- `no-unused-vars` — 禁止未使用变量
- `no-console.error` — 禁止 `console.error`
- `react-hooks/exhaustive-deps` — Hook 依赖完整性
- `jsx-a11y/*` — 无障碍访问

## TypeScript

```bash
npm run typecheck
```

使用 `tsconfig.json`，`tests/` 目录已纳入类型检查范围。
