import { defineConfig } from '@playwright/test'
import path from 'node:path'

/**
 * Playwright 配置 —— Phase 1A E2E
 *
 * 前端以 MSW mock 模式运行（VITE_USE_MOCK=true），无需真实后端。
 * webServer 会自动启动 web-client（端口 5174）；若本地已在该端口启动，
 * 非 CI 环境下直接复用。
 */
export default defineConfig({
  testDir: './tests',
  /* 单个用例最长 30s（SSE 流式回复可能耗时数秒） */
  timeout: 30_000,
  expect: {
    timeout: 8_000,
  },
  /* 单 worker 串行：共享同一个 Vite dev server 与 SW，避免并发干扰 */
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:5174',
    headless: true,
    viewport: { width: 1440, height: 900 },
    actionTimeout: 10_000,
    navigationTimeout: 15_000,
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: {
        browserName: 'chromium',
      },
    },
  ],
  webServer: {
    command: 'npm run dev -- --port 5174 --strictPort',
    cwd: path.resolve(__dirname, '../../web-client'),
    url: 'http://localhost:5174',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    env: {
      VITE_USE_MOCK: 'true',
    },
  },
})
