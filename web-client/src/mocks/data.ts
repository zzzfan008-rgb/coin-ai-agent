/**
 * Mock 内存数据 —— 页面刷新后重置
 */

export interface MockUser {
  id: string
  org_id: string
  dept_id: string
  username: string
  password: string
  display_name: string
  email?: string
  role: 'admin' | 'designer' | 'viewer'
}

export interface MockSession {
  id: string
  user_id: string
  title: string
  is_archived: boolean
  created_at: string
  updated_at: string
}

export interface MockMessage {
  id: string
  session_id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  model: string | null
  created_at: string
}

function uuid(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16)
  })
}

export function newId(): string {
  return uuid()
}

export function fakeJwt(userId: string): string {
  const header = btoa(JSON.stringify({ alg: 'HS256', typ: 'JWT' }))
  const payload = btoa(
    JSON.stringify({
      sub: userId,
      iat: Math.floor(Date.now() / 1000),
      exp: Math.floor(Date.now() / 1000) + 86400,
      mock: true,
    }),
  )
  return `${header}.${payload}.mock-signature`
}

const now = new Date().toISOString()

// ── 种子用户（demo 账号） ──────────────────────────────────────────────────

export const users: MockUser[] = [
  {
    id: 'u-demo-0001',
    org_id: 'org-demo-0001',
    dept_id: 'dept-demo-0001',
    username: 'designer',
    password: 'designer123',
    display_name: '示例设计师',
    email: 'designer@example.com',
    role: 'designer',
  },
]

// ── 种子会话 + 消息 ────────────────────────────────────────────────────────

export const sessions: MockSession[] = [
  {
    id: 'sess-demo-0001',
    user_id: 'u-demo-0001',
    title: '欢迎使用 Fashion AI',
    is_archived: false,
    created_at: now,
    updated_at: now,
  },
]

export const messages: MockMessage[] = [
  {
    id: newId(),
    session_id: 'sess-demo-0001',
    role: 'assistant',
    content:
      '你好，我是 **Fashion AI 设计助手** 👋\n\n我可以帮你完成：\n\n- 🔍 **面料查询**：成分、适用季节、保养方式\n- 🎨 **色彩搭配**：配色方案、流行色分析\n- 💡 **款式灵感**：创意生成、款式变体\n\n试试在右侧面板选择一个 Skill，然后向我提问吧！',
    model: 'fashion-ai-default',
    created_at: now,
  },
]
