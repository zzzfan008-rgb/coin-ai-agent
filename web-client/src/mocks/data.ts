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

export interface MockProject {
  id: string
  org_id: string
  dept_id: string
  owner_id: string
  name: string
  description: string | null
  cover_color: string
  is_archived: boolean
  created_at: string
  updated_at: string
}

export interface MockSessionProject {
  session_id: string
  project_id: string
  added_at: string
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

// ── 知识库 (T-015) ──────────────────────────────────────────────────────────

export interface MockKnowledgeDoc {
  id: string
  org_id: string
  dept_id: string | null
  filename: string
  file_type: string
  status: 'pending' | 'indexing' | 'ready' | 'failed' | 'deleted'
  chunk_count: number
  uploaded_by: string | null
  error: string | null
  created_at: string
  updated_at: string
}

// ── MCP Server (T-016) ──────────────────────────────────────────────────────

export interface MockMcpServer {
  id: string
  org_id: string
  name: string
  endpoint_url: string
  health_status: 'healthy' | 'unhealthy' | 'unknown'
  tool_count: number
  created_at: string
  updated_at: string
}

/** 用户级启用状态：key = `${userId}:${serverId}` */
export const mcpUserEnabled: Record<string, boolean> = {}

/** 普通用户被授权可见的 server（mock：designer 仅授权一个） */
export const mcpGrants: Record<string, string[]> = {}

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
  {
    id: 'u-demo-admin',
    org_id: 'org-demo-0001',
    dept_id: 'dept-demo-0001',
    username: 'admin',
    password: 'admin123',
    display_name: '平台管理员',
    email: 'admin@example.com',
    role: 'admin',
  },
]

// ── 种子会话 + 消息 ────────────────────────────────────────────────────────

export const sessions: MockSession[] = [
  {
    id: 'sess-demo-0001',
    user_id: 'u-demo-0001',
    title: '欢迎使用 Coin-AI',
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
      '你好，我是 **Coin-AI 设计助手** 👋\n\n我可以帮你完成：\n\n- 🔍 **面料查询**：成分、适用季节、保养方式\n- 🎨 **色彩搭配**：配色方案、流行色分析\n- 💡 **款式灵感**：创意生成、款式变体\n\n试试在右侧面板选择一个 Skill，然后向我提问吧！',
    model: 'fashion-ai-default',
    created_at: now,
  },
]

// ── 种子项目 + 关联 ─────────────────────────────────────────────────────────

export const projects: MockProject[] = [
  {
    id: 'proj-demo-0001',
    org_id: 'org-demo-0001',
    dept_id: 'dept-demo-0001',
    owner_id: 'u-demo-0001',
    name: '2027 春夏系列',
    description: '春夏季度灵感与面料调研',
    cover_color: '#6366F1',
    is_archived: false,
    created_at: now,
    updated_at: now,
  },
  {
    id: 'proj-demo-0002',
    org_id: 'org-demo-0001',
    dept_id: 'dept-demo-0001',
    owner_id: 'u-demo-0001',
    name: '可持续面料专题',
    description: null,
    cover_color: '#10B981',
    is_archived: false,
    created_at: now,
    updated_at: now,
  },
  {
    id: 'proj-demo-0003',
    org_id: 'org-demo-0001',
    dept_id: 'dept-demo-0001',
    owner_id: 'u-demo-0001',
    name: '2026 秋冬系列（已归档）',
    description: '历史归档项目',
    cover_color: '#F59E0B',
    is_archived: true,
    created_at: now,
    updated_at: now,
  },
]

export const sessionProjects: MockSessionProject[] = [
  {
    session_id: 'sess-demo-0001',
    project_id: 'proj-demo-0001',
    added_at: now,
  },
]

// ── 种子知识库文档 ─────────────────────────────────────────────────────────

export const knowledgeDocs: MockKnowledgeDoc[] = [
  {
    id: 'doc-demo-0001',
    org_id: 'org-demo-0001',
    dept_id: null,
    filename: '2027春夏面料趋势报告.pdf',
    file_type: 'pdf',
    status: 'ready',
    chunk_count: 42,
    uploaded_by: 'u-demo-admin',
    error: null,
    created_at: now,
    updated_at: now,
  },
  {
    id: 'doc-demo-0002',
    org_id: 'org-demo-0001',
    dept_id: 'dept-demo-0001',
    filename: '面料保养手册.docx',
    file_type: 'docx',
    status: 'ready',
    chunk_count: 18,
    uploaded_by: 'u-demo-admin',
    error: null,
    created_at: now,
    updated_at: now,
  },
  {
    id: 'doc-demo-0003',
    org_id: 'org-demo-0001',
    dept_id: null,
    filename: '供应链对接规范.txt',
    file_type: 'txt',
    status: 'failed',
    chunk_count: 0,
    uploaded_by: 'u-demo-admin',
    error: '文件编码无法识别（非 UTF-8）',
    created_at: now,
    updated_at: now,
  },
]

// ── 种子 MCP Servers ───────────────────────────────────────────────────────

export const mcpServers: MockMcpServer[] = [
  {
    id: 'fabric-erp',
    org_id: 'org-demo-0001',
    name: '面料 ERP 系统',
    endpoint_url: 'http://localhost:9101/mcp',
    health_status: 'healthy',
    tool_count: 6,
    created_at: now,
    updated_at: now,
  },
  {
    id: 'plm-tools',
    org_id: 'org-demo-0001',
    name: 'PLM 款式库',
    endpoint_url: 'http://localhost:9102/mcp',
    health_status: 'unhealthy',
    tool_count: 0,
    created_at: now,
    updated_at: now,
  },
]

// designer 被授权 fabric-erp 且默认启用
mcpGrants['u-demo-0001'] = ['fabric-erp']
mcpUserEnabled['u-demo-0001:fabric-erp'] = true
// admin 默认启用全部
mcpUserEnabled['u-demo-admin:fabric-erp'] = true
mcpUserEnabled['u-demo-admin:plm-tools'] = false
