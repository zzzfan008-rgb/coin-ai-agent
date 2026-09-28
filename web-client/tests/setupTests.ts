import '@testing-library/jest-dom'
import { cleanup } from '@testing-library/react'
import { afterEach } from 'vitest'

// JSDOM 中 window.localStorage 和 globalThis.localStorage 可能不一致。
// 在 vitest jsdom 环境里，window.localStorage 存在但 globalThis.localStorage 可能 undefined。
// 这里统一用 window.localStorage 覆盖 globalThis。
;(globalThis as Record<string, unknown>).localStorage = window.localStorage

// Automatically unmount React trees after each test
afterEach(() => {
  cleanup()
})
