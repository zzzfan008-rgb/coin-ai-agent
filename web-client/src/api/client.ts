/**
 * API 客户端 —— 对接 Go 网关（api/gateway.yml 契约）
 *
 * Mock 模式下所有请求由 MSW 在浏览器内拦截，无需真实后端；
 * 对接真实网关时，本文件无需修改（Vite proxy 转发到 :8080）。
 */

const TOKEN_KEY = 'fai_token'
const USER_KEY = 'fai_user'

// ── 类型定义（与 gateway.yml schemas 对齐） ────────────────────────────────

export interface LoginRequest {
  username: string
  password: string
}

export interface RegisterRequest {
  username: string
  password: string
  display_name?: string
  email?: string
  org_name: string
  dept_name: string
}

export type UserRole = 'admin' | 'designer' | 'viewer'

export interface AuthUser {
  user_id: string
  org_id: string
  dept_id: string
  username: string
  display_name?: string
  role: UserRole
}

export interface AuthResponse extends AuthUser {
  token: string
}

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content: string
}

export interface Session {
  id: string
  title: string
  is_archived: boolean
  message_count?: number
  created_at: string
  updated_at: string
}

export interface Project {
  id: string
  org_id: string
  dept_id: string
  owner_id: string
  name: string
  description?: string | null
  cover_color: string
  is_archived: boolean
  created_at: string
  updated_at: string
}

export interface ProjectDetail extends Project {
  sessions: Session[]
}

export interface SessionMessage {
  id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  model?: string | null
  created_at: string
}

export interface SkillInfo {
  id: string
  name: string
  description: string
  tags: string[]
}

/** 与 skills/ 注册表对应的 Skill 目录 */
export const SKILL_CATALOG: SkillInfo[] = [
  {
    id: 'fabric-query',
    name: '面料查询',
    description: '根据面料名称查询成分、适用季节、保养方式',
    tags: ['fabric', 'consultation'],
  },
  {
    id: 'color-matching',
    name: '色彩搭配',
    description: '基于主色推荐搭配色、分析色彩和谐性、提供当季流行色',
    tags: ['color', 'matching'],
  },
  {
    id: 'style-inspiration',
    name: '款式灵感',
    description: '基于关键词生成款式创意、提供款式变体、分析流行趋势',
    tags: ['style', 'inspiration'],
  },
  {
    id: 'dreamina-cli',
    name: 'AI 生图',
    description: '文生图 / 图生图，调用即梦 AI 生成服装设计图、电商主图、场景图',
    tags: ['image', 'dreamina', '生图'],
  },
]

// ── Token 管理 ──────────────────────────────────────────────────────────────

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

export function saveAuth(_token: string, user: AuthUser): void {
  // Token stored in httpOnly cookie by the gateway — never touch localStorage for auth.
  localStorage.setItem(USER_KEY, JSON.stringify(user))
}

export function getStoredUser(): AuthUser | null {
  const raw = localStorage.getItem(USER_KEY)
  if (!raw) return null
  try {
    return JSON.parse(raw) as AuthUser
  } catch {
    return null
  }
}

export function clearAuth(): void {
  // Remove localStorage state; the httpOnly cookie is cleared by POST /api/auth/logout.
  localStorage.removeItem(TOKEN_KEY)
  localStorage.removeItem(USER_KEY)
}

// ── HTTP 基础封装 ─────────────────────────────────────────────────────────

// readError parses an error response body into a human-readable message.
async function readError(res: Response): Promise<Error> {
  let message = `请求失败（${res.status}）`
  try {
    const body = await res.clone().json()
    if (body?.error?.message) message = body.error.message
  } catch {
    // 非 JSON 错误响应，保留默认消息
  }
  if (res.status === 401) message = message || '用户名或密码错误'
  return new Error(message)
}

// apiFetch sends all requests with credentials: 'include' so the httpOnly cookie is always sent.
// Auth is handled by the cookie middleware; no Authorization header needed for browser sessions.
async function apiFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
  const res = await fetch(path, {
    ...init,
    credentials: 'include', // always send cookies (including the httpOnly auth cookie)
    headers: {
      'Content-Type': 'application/json',
      'X-Requested-With': 'XMLHttpRequest',
      ...init.headers,
    },
  })
  if (!res.ok) throw await readError(res)
  if (res.status === 204) return undefined as T
  if (res.headers.get('content-length') === '0') return undefined as T
  return (await res.json()) as T
}

// ── 认证接口 ────────────────────────────────────────────────────────────────

export async function login(req: LoginRequest): Promise<AuthResponse> {
  return apiFetch<AuthResponse>('/auth/login', {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

export async function register(req: RegisterRequest): Promise<AuthResponse> {
  return apiFetch<AuthResponse>('/auth/register', {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

/** POST /api/auth/logout clears the httpOnly cookie on the gateway side. */
export async function logout(): Promise<void> {
  await apiFetch<void>('/api/auth/logout', { method: 'POST' })
}

// ── 会话接口 ────────────────────────────────────────────────────────────────

export async function listSessions(): Promise<{ sessions: Session[]; total: number }> {
  return apiFetch('/api/sessions?limit=50')
}

export async function createSession(title?: string): Promise<Session> {
  return apiFetch('/api/sessions', {
    method: 'POST',
    body: JSON.stringify({ title: title || '新会话' }),
  })
}

export async function updateSession(
  id: string,
  patchBody: { title?: string; is_archived?: boolean },
): Promise<Session> {
  return apiFetch(`/api/sessions/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(patchBody),
  })
}

export async function listMessages(
  sessionId: string,
): Promise<{ messages: SessionMessage[]; has_more: boolean }> {
  return apiFetch(`/api/sessions/${sessionId}/messages?limit=100`)
}

// ── 项目接口 ────────────────────────────────────────────────────────────────

export async function listProjects(archived = false): Promise<{ projects: Project[]; total: number }> {
  return apiFetch(`/api/projects?archived=${archived}`)
}

export async function createProject(body: {
  name: string
  description?: string
  cover_color?: string
}): Promise<Project> {
  return apiFetch('/api/projects', {
    method: 'POST',
    body: JSON.stringify(body),
  })
}

export async function getProject(id: string): Promise<ProjectDetail> {
  return apiFetch(`/api/projects/${id}`)
}

export async function updateProject(
  id: string,
  body: { name?: string; description?: string; cover_color?: string },
): Promise<Project> {
  return apiFetch(`/api/projects/${id}`, {
    method: 'PUT',
    body: JSON.stringify(body),
  })
}

export async function deleteProject(id: string): Promise<void> {
  await apiFetch(`/api/projects/${id}`, { method: 'DELETE' })
}

export async function archiveProject(id: string): Promise<Project> {
  return apiFetch(`/api/projects/${id}/archive`, { method: 'POST' })
}

export async function unarchiveProject(id: string): Promise<Project> {
  return apiFetch(`/api/projects/${id}/unarchive`, { method: 'POST' })
}

export async function addSessionToProject(
  projectId: string,
  sessionId: string,
): Promise<void> {
  await apiFetch(`/api/projects/${projectId}/sessions`, {
    method: 'POST',
    body: JSON.stringify({ session_id: sessionId }),
  })
}

export async function removeSessionFromProject(
  projectId: string,
  sessionId: string,
): Promise<void> {
  await apiFetch(`/api/projects/${projectId}/sessions/${sessionId}`, {
    method: 'DELETE',
  })
}

export async function listProjectsForSession(
  sessionId: string,
): Promise<{ projects: Project[] }> {
  return apiFetch(`/api/sessions/${sessionId}/projects`)
}

// ── CLIP 以图搜图 (T-019) ───────────────────────────────────────────────────

export interface StyleImage {
  id: string
  org_id: string
  dept_id: string
  style_id: string | null
  image_path: string
  file_type: string | null
  uploaded_by: string | null
  created_at: string
}

export interface SimilarImage {
  image_path: string
  style_id: string | null
  style_name: string | null
  similarity: number
}

/** 以图搜图：multipart 上传一张图片，返回 Top-K 相似款式图。
 *  org/dept 由网关从 JWT 注入（X-Auth-*），前端不传身份字段。 */
export async function searchSimilarImages(
  file: File,
  topK = 8,
): Promise<SimilarImage[]> {
  const form = new FormData()
  form.append('file', file)
  form.append('top_k', String(topK))

  const res = await fetch('/api/images/similar', {
    method: 'POST',
    credentials: 'include', // send httpOnly cookie
    body: form,
  })
  if (!res.ok) throw await readError(res)
  const body = (await res.json()) as { results: SimilarImage[] }
  return body.results
}

/** 上传款式图片 */
export async function uploadStyleImage(
  styleId: string,
  file: File,
  uploadedBy?: string,
): Promise<StyleImage> {
  const form = new FormData()
  form.append('file', file)
  if (uploadedBy) form.append('uploaded_by', uploadedBy)

  const res = await fetch(`/api/styles/${styleId}/images`, {
    method: 'POST',
    credentials: 'include', // send httpOnly cookie
    body: form,
  })
  if (!res.ok) throw await readError(res)
  return (await res.json()) as StyleImage
}

/** 款式图片列表 */
export async function listStyleImages(
  styleId: string,
): Promise<{ images: StyleImage[]; total: number }> {
  return apiFetch(`/api/styles/${styleId}/images`)
}

// ── 知识库管理 (T-015 / F-06) ───────────────────────────────────────────────

export type KnowledgeDocStatus =
  | 'pending'
  | 'indexing'
  | 'ready'
  | 'failed'
  | 'deleted'

export interface KnowledgeDocument {
  id: string
  org_id: string
  dept_id: string | null
  filename: string
  file_type: string
  status: KnowledgeDocStatus
  chunk_count: number
  uploaded_by: string | null
  error: string | null
  created_at: string
  updated_at: string
  description: string
}

export const KNOWLEDGE_ALLOWED_EXT = ['pdf', 'docx', 'txt', 'md']

/** 知识库文档列表（管理员） */
export async function listKnowledgeDocuments(): Promise<{
  documents: KnowledgeDocument[]
  total: number
}> {
  return apiFetch('/api/knowledge/documents')
}

/** 上传文档（multipart），上传后由后端异步索引，可轮询列表查看进度 */
export async function uploadKnowledgeDocument(file: File): Promise<KnowledgeDocument> {
  const form = new FormData()
  form.append('file', file)

  const res = await fetch('/api/knowledge/documents', {
    method: 'POST',
    credentials: 'include', // send httpOnly cookie
    headers: { 'X-Requested-With': 'XMLHttpRequest' },
    body: form,
  })
  if (!res.ok) throw await readError(res)
  return (await res.json()) as KnowledgeDocument
}

/** 删除文档（软删除，同时清理向量库） */
export async function deleteKnowledgeDocument(id: string): Promise<void> {
  await apiFetch(`/api/knowledge/documents/${id}`, { method: 'DELETE' })
}

/** 修改文档描述 */
export async function updateKnowledgeDocument(
  id: string,
  patch: { description?: string; filename?: string },
): Promise<KnowledgeDocument> {
  const res = await apiFetch(`/api/knowledge/documents/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(patch),
  })
  return res as KnowledgeDocument
}

// ── MCP Server 管理 (T-016 / F-06) ──────────────────────────────────────────

export type McpHealthStatus = 'healthy' | 'unhealthy' | 'unknown'

export interface McpServer {
  id: string
  org_id: string
  name: string
  endpoint_url: string
  health_status: McpHealthStatus
  /** 当前用户是否启用该 server（用户级开关） */
  enabled: boolean
  tool_count: number
  created_at: string
  updated_at: string
}

/** 已注册 MCP server 列表（管理员看全部；普通用户仅看已授权的） */
export async function listMcpServers(): Promise<{
  servers: McpServer[]
  total: number
}> {
  return apiFetch('/api/mcp/servers')
}

/** 注册 MCP server（管理员） */
export async function registerMcpServer(body: {
  id: string
  name?: string
  endpoint_url: string
}): Promise<McpServer> {
  return apiFetch('/api/mcp/servers', {
    method: 'POST',
    body: JSON.stringify(body),
  })
}

/** 删除 MCP server（管理员） */
export async function deleteMcpServer(id: string): Promise<void> {
  await apiFetch(`/api/mcp/servers/${id}`, { method: 'DELETE' })
}

/** 用户级启用/停用开关 */
export async function toggleMcpServer(
  id: string,
  enabled: boolean,
): Promise<McpServer> {
  return apiFetch(`/api/mcp/servers/${id}/toggle`, {
    method: 'POST',
    body: JSON.stringify({ enabled }),
  })
}

// ── 流式对话（SSE） ─────────────────────────────────────────────────────────

export interface StreamChatParams {
  messages: ChatMessage[]
  sessionId?: string
  skillIds?: string[]
  model?: string
  signal?: AbortSignal
}

/**
 * 调用 OpenAI 兼容的 /v1/chat/completions（stream=true）。
 * 逐块解析 SSE，每收到 content delta 回调一次 onDelta。
 * 返回完整的 assistant 文本。
 */
export async function streamChatCompletion(
  params: StreamChatParams,
  onDelta: (chunk: string) => void,
): Promise<string> {
  const res = await fetch('/v1/chat/completions', {
    method: 'POST',
    credentials: 'include', // send httpOnly cookie for /v1/* (Bearer fallback also works)
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: params.model ?? 'fashion-ai-default',
      messages: params.messages,
      stream: true,
      extra_body: {
        session_id: params.sessionId ?? null,
        skill_ids: params.skillIds ?? [],
        mcp_server_ids: [],
        knowledge_collections: [],
      },
    }),
    signal: params.signal,
  })

  console.log('[SSE] fetch completed:', res.status, res.statusText, 'CT:', res.headers.get('content-type'), 'body:', res.body !== null)
  if (!res.ok) {
    const err = await readError(res)
    console.log('[SSE] non-ok fetch error:', err)
    throw err
  }
  if (!res.body) throw new Error('浏览器不支持流式响应（ReadableStream）')

  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  let full = ''

  const handleEvent = (rawEvent: string): void => {
    for (const line of rawEvent.split('\n')) {
      const trimmed = line.trim()
      if (!trimmed.startsWith('data:')) continue
      const data = trimmed.slice(5).trim()
      if (!data || data === '[DONE]') continue
      try {
        const json = JSON.parse(data)
        const delta: unknown = json?.choices?.[0]?.delta?.content
        if (typeof delta === 'string' && delta) {
          full += delta
          onDelta(delta)
        }
      } catch {
        // 忽略非 JSON 的注释行/心跳
      }
    }
  }

  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    buffer += decoder.decode(value, { stream: true })
    let sep: number
    while ((sep = buffer.indexOf('\n\n')) >= 0) {
      handleEvent(buffer.slice(0, sep))
      buffer = buffer.slice(sep + 2)
    }
  }
  if (buffer.trim()) handleEvent(buffer)

  return full
}

/** Upload a single image for use in chat (e.g. dreamina image2image). */
export async function uploadChatImage(file: File): Promise<string> {
  const fd = new FormData()
  fd.append('file', file)
  const res = await fetch('/v1/chat/upload-image', {
    method: 'POST',
    credentials: 'include',
    headers: { 'X-Requested-With': 'XMLHttpRequest' },
    body: fd,
  })
  if (!res.ok) throw new Error(`upload-chat-image failed: ${res.status}`)
  const data = await res.json()
  return data.url as string
}

/** Non-streaming: waits for the full response, returns complete text. */
export async function chatCompletion(
  params: StreamChatParams,
): Promise<string> {
  const res = await fetch('/v1/chat/completions', {
    method: 'POST',
    credentials: 'include',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: params.model ?? 'fashion-ai-default',
      messages: params.messages,
      stream: false,
      extra_body: {
        session_id: params.sessionId ?? null,
        skill_ids: params.skillIds ?? [],
        mcp_server_ids: [],
        knowledge_collections: [],
      },
    }),
    signal: params.signal,
  })

  if (!res.ok) {
    const err = await readError(res)
    throw err
  }

  const data = await res.json()
  return data?.choices?.[0]?.message?.content ?? ''
}
