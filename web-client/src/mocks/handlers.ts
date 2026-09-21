import { http, HttpResponse } from 'msw'
import {
  users,
  sessions,
  messages,
  projects,
  sessionProjects,
  newId,
  fakeJwt,
  type MockUser,
  type MockProject,
} from './data'

// ── 工具函数 ────────────────────────────────────────────────────────────────

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

function errorResponse(status: number, code: string, message: string) {
  return HttpResponse.json(
    { error: { code, message } },
    { status },
  )
}

/** 从 Authorization 头解析 mock 用户 */
function authenticate(request: Request): MockUser | null {
  const auth = request.headers.get('Authorization') ?? ''
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : ''
  if (!token) return null
  try {
    const payload = JSON.parse(atob(token.split('.')[1]))
    return users.find((u) => u.id === payload.sub) ?? null
  } catch {
    return null
  }
}

function publicUser(u: MockUser) {
  const { password: _password, ...rest } = u
  return rest
}

// ── AI 回复生成（含 Markdown，用于验证渲染与代码高亮） ──────────────────────

function buildReply(userText: string, skillIds: string[]): string {
  const q = userText.trim()

  if (skillIds.includes('fabric-query')) {
    return [
      `关于「**${q}**」的面料咨询，结果如下：`,
      '',
      '## 基本属性',
      '',
      '| 属性 | 信息 |',
      '|------|------|',
      '| 成分 | 65% 棉 / 32% 聚酯纤维 / 3% 氨纶 |',
      '| 克重 | 180 g/m² |',
      '| 适用季节 | 春、秋 |',
      '',
      '## 保养建议',
      '',
      '- 水温不超过 **30℃**，轻柔机洗',
      '- 避免暴晒，阴凉处悬挂晾干',
      '- 中温蒸汽熨烫（≤150℃）',
      '',
      '```json',
      JSON.stringify(
        { fabric: q, breathable: true, stretch: '3%', shrinkage: '<2%' },
        null,
        2,
      ),
      '```',
      '',
      '需要我再推荐适合该面料的**款式**吗？',
    ].join('\n')
  }

  if (skillIds.includes('color-matching')) {
    return [
      `基于「**${q}**」为你生成配色方案：`,
      '',
      '## 推荐配色',
      '',
      '1. **主色** `#6366F1` — 靛蓝，专业而富有创意',
      '2. **辅助色** `#F59E0B` — 琥珀，形成暖色对比',
      '3. **点缀色** `#10B981` — 祖母绿，增加层次',
      '',
      '> 三色在色相环上近似三等分，属于高和谐度的三角配色。',
      '',
      '## 当季趋势',
      '',
      '- 低饱和大地色持续流行',
      '- 电光蓝作为点缀色回归',
      '',
      '需要导出色卡（ASE / JSON）吗？',
    ].join('\n')
  }

  if (skillIds.includes('style-inspiration')) {
    return [
      `围绕「**${q}**」的款式灵感：`,
      '',
      '## 创意方向',
      '',
      '- **解构西装外套**：不对称门襟 + 落肩剪裁',
      '- **层次穿搭**：长款马甲叠穿阔腿裤',
      '- **机能细节**：隐形拉链口袋与调节扣',
      '',
      '## 款式变体',
      '',
      '1. 通勤版：收腰 + 中长款',
      '2. 休闲版：Oversize + 短款',
      '3. 秀场版：拼接材质 + 夸张轮廓',
      '',
      '```ts',
      '// 款式变体参数示例',
      'const variant = {',
      '  theme: ' + JSON.stringify(q) + ',',
      '  silhouette: "deconstructed",',
      '  occasions: ["office", "runway"],',
      '}',
      '```',
      '',
      '想针对哪个方向深入展开？',
    ].join('\n')
  }

  return [
    `收到你的问题：「${q}」`,
    '',
    '我可以从以下几个方面协助你：',
    '',
    '- 查询**面料**成分与保养方式',
    '- 生成**配色方案**与流行色分析',
    '- 提供**款式灵感**与变体设计',
    '',
    '在右侧 **Skill 面板**选择对应技能后，我会给出更专业的回答。',
  ].join('\n')
}

/** 构造 SSE chunk */
function sseChunk(data: unknown): Uint8Array {
  const encoder = new TextEncoder()
  return encoder.encode(`data: ${JSON.stringify(data)}\n\n`)
}

// ── Handlers ────────────────────────────────────────────────────────────────

export const handlers = [
  // ── 注册 ─────────────────────────────────────────────────────────────────
  http.post('*/auth/register', async ({ request }) => {
    const body = (await request.json()) as Record<string, string>
    const { username, password, org_name, dept_name } = body

    if (!username || !password || !org_name || !dept_name) {
      return errorResponse(400, 'bad_request', '请填写完整的注册信息')
    }
    if (username.length < 3) {
      return errorResponse(400, 'bad_request', '用户名至少 3 个字符')
    }
    if (password.length < 6) {
      return errorResponse(400, 'bad_request', '密码至少 6 位')
    }
    if (users.some((u) => u.username === username)) {
      return errorResponse(409, 'conflict', '用户名已存在')
    }

    const user: MockUser = {
      id: newId(),
      org_id: newId(),
      dept_id: newId(),
      username,
      password,
      display_name: body.display_name || username,
      email: body.email,
      role: 'designer',
    }
    users.push(user)

    return HttpResponse.json(
      { ...publicUser(user), token: fakeJwt(user.id) },
      { status: 201 },
    )
  }),

  // ── 登录 ─────────────────────────────────────────────────────────────────
  http.post('*/auth/login', async ({ request }) => {
    const body = (await request.json()) as { username?: string; password?: string }
    const user = users.find(
      (u) => u.username === body.username && u.password === body.password,
    )
    if (!user) {
      return errorResponse(401, 'unauthorized', '用户名或密码错误')
    }
    return HttpResponse.json({
      ...publicUser(user),
      token: fakeJwt(user.id),
    })
  }),

  // ── 模型列表 ─────────────────────────────────────────────────────────────
  http.get('*/v1/models', ({ request }) => {
    if (!authenticate(request)) {
      return errorResponse(401, 'unauthorized', '未认证或 Token 无效')
    }
    return HttpResponse.json({
      object: 'list',
      data: [
        {
          id: 'fashion-ai-default',
          object: 'model',
          created: 1726000000,
          owned_by: 'fashion-ai',
        },
      ],
    })
  }),

  // ── 流式对话 ─────────────────────────────────────────────────────────────
  http.post('*/v1/chat/completions', async ({ request }) => {
    const user = authenticate(request)
    if (!user) {
      return errorResponse(401, 'unauthorized', '未认证或 Token 无效')
    }

    const body = (await request.json()) as {
      messages: { role: string; content: string }[]
      extra_body?: {
        session_id?: string
        skill_ids?: string[]
      }
      model?: string
    }

    const sessionId = body.extra_body?.session_id
    const session = sessions.find((s) => s.id === sessionId)
    const lastUser = [...body.messages].reverse().find((m) => m.role === 'user')
    const skillIds = body.extra_body?.skill_ids ?? []
    const reply = buildReply(lastUser?.content ?? '', skillIds)
    const timestamp = new Date().toISOString()
    const completionId = `chatcmpl_${newId()}`

    // 持久化到 mock 存储
    if (session) {
      if (lastUser) {
        messages.push({
          id: newId(),
          session_id: session.id,
          role: 'user',
          content: lastUser.content,
          model: null,
          created_at: timestamp,
        })
      }
      messages.push({
        id: newId(),
        session_id: session.id,
        role: 'assistant',
        content: reply,
        model: body.model ?? 'fashion-ai-default',
        created_at: timestamp,
      })
      session.updated_at = timestamp
      if (session.title === '新会话' && lastUser) {
        session.title =
          lastUser.content.slice(0, 20) +
          (lastUser.content.length > 20 ? '…' : '')
      }
    }

    // 用 ReadableStream 输出 SSE，模拟打字机效果
    const encoder = new TextEncoder()
    const stream = new ReadableStream<Uint8Array>({
      async start(controller) {
        // 首帧：role
        controller.enqueue(
          sseChunk({
            id: completionId,
            object: 'chat.completion.chunk',
            created: Math.floor(Date.now() / 1000),
            model: body.model ?? 'fashion-ai-default',
            choices: [
              { index: 0, delta: { role: 'assistant' }, finish_reason: null },
            ],
          }),
        )

        // 按 2~5 个字符切片
        const chars = [...reply]
        let i = 0
        while (i < chars.length) {
          const size = 2 + Math.floor(Math.random() * 4)
          const piece = chars.slice(i, i + size).join('')
          i += size
          controller.enqueue(
            sseChunk({
              id: completionId,
              object: 'chat.completion.chunk',
              created: Math.floor(Date.now() / 1000),
              model: body.model ?? 'fashion-ai-default',
              choices: [
                { index: 0, delta: { content: piece }, finish_reason: null },
              ],
            }),
          )
          await sleep(24)
        }

        // 结束帧
        controller.enqueue(
          encoder.encode(
            `data: ${JSON.stringify({
              id: completionId,
              choices: [{ index: 0, delta: {}, finish_reason: 'stop' }],
            })}\n\n`,
          ),
        )
        controller.enqueue(encoder.encode('data: [DONE]\n\n'))
        controller.close()
      },
    })

    return new Response(stream, {
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
      },
    })
  }),

  // ── 会话列表 ─────────────────────────────────────────────────────────────
  http.get('*/api/sessions', ({ request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const url = new URL(request.url)
    const archived = url.searchParams.get('archived') === 'true'
    const list = sessions
      .filter((s) => s.user_id === user.id && s.is_archived === archived)
      .map((s) => ({
        ...s,
        message_count: messages.filter((m) => m.session_id === s.id).length,
      }))
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at))

    return HttpResponse.json({ sessions: list, total: list.length })
  }),

  // ── 创建会话 ─────────────────────────────────────────────────────────────
  http.post('*/api/sessions', async ({ request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const body = (await request.json().catch(() => ({}))) as { title?: string }
    const ts = new Date().toISOString()
    const session = {
      id: newId(),
      user_id: user.id,
      title: body.title || '新会话',
      is_archived: false,
      created_at: ts,
      updated_at: ts,
    }
    sessions.push(session)
    return HttpResponse.json(session, { status: 201 })
  }),

  // ── 会话详情 ─────────────────────────────────────────────────────────────
  http.get('*/api/sessions/:id', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const session = sessions.find(
      (s) => s.id === params.id && s.user_id === user.id,
    )
    if (!session) return errorResponse(404, 'not_found', '会话不存在')
    return HttpResponse.json(session)
  }),

  // ── 更新会话 ─────────────────────────────────────────────────────────────
  http.patch('*/api/sessions/:id', async ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const session = sessions.find(
      (s) => s.id === params.id && s.user_id === user.id,
    )
    if (!session) return errorResponse(404, 'not_found', '会话不存在')

    const body = (await request.json()) as {
      title?: string
      is_archived?: boolean
    }
    if (typeof body.title === 'string') session.title = body.title
    if (typeof body.is_archived === 'boolean') {
      session.is_archived = body.is_archived
    }
    return HttpResponse.json(session)
  }),

  // ── 消息历史 ─────────────────────────────────────────────────────────────
  http.get('*/api/sessions/:id/messages', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const session = sessions.find(
      (s) => s.id === params.id && s.user_id === user.id,
    )
    if (!session) return errorResponse(404, 'not_found', '会话不存在')

    const list = messages
      .filter((m) => m.session_id === session.id)
      .sort((a, b) => a.created_at.localeCompare(b.created_at))

    return HttpResponse.json({
      messages: list,
      has_more: false,
      next_cursor: null,
    })
  }),

  // ── 项目列表（部门隔离） ──────────────────────────────────────────────────
  http.get('*/api/projects', ({ request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const url = new URL(request.url)
    const archived = url.searchParams.get('archived') === 'true'
    const list = projects.filter(
      (p) => p.dept_id === user.dept_id && p.is_archived === archived,
    )
    return HttpResponse.json({ projects: list, total: list.length })
  }),

  // ── 创建项目 ─────────────────────────────────────────────────────────────
  http.post('*/api/projects', async ({ request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const body = (await request.json().catch(() => null)) as {
      name?: string
      description?: string
      cover_color?: string
    } | null
    if (!body || !body.name) {
      return errorResponse(400, 'bad_request', '项目名称不能为空')
    }

    const ts = new Date().toISOString()
    const project: MockProject = {
      id: newId(),
      org_id: user.org_id,
      dept_id: user.dept_id,
      owner_id: user.id,
      name: body.name,
      description: body.description ?? null,
      cover_color: body.cover_color || '#6366F1',
      is_archived: false,
      created_at: ts,
      updated_at: ts,
    }
    projects.push(project)
    return HttpResponse.json(project, { status: 201 })
  }),

  // ── 项目详情（含会话） ────────────────────────────────────────────────────
  http.get('*/api/projects/:id', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const project = findProject(user, String(params.id))
    if (!project) return errorResponse(404, 'not_found', '项目不存在')

    return HttpResponse.json({
      ...project,
      sessions: sessionsInProject(project.id),
    })
  }),

  // ── 更新项目 ─────────────────────────────────────────────────────────────
  http.put('*/api/projects/:id', async ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const project = findProject(user, String(params.id))
    if (!project) return errorResponse(404, 'not_found', '项目不存在')

    const body = (await request.json()) as {
      name?: string
      description?: string
      cover_color?: string
    }
    if (typeof body.name === 'string') project.name = body.name
    if (typeof body.description === 'string') project.description = body.description
    if (typeof body.cover_color === 'string') project.cover_color = body.cover_color
    project.updated_at = new Date().toISOString()

    return HttpResponse.json(project)
  }),

  // ── 删除项目（软删除） ───────────────────────────────────────────────────
  http.delete('*/api/projects/:id', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const project = findProject(user, String(params.id))
    if (!project) return errorResponse(404, 'not_found', '项目不存在')

    project.is_archived = true
    project.updated_at = new Date().toISOString()
    return new Response(null, { status: 204 })
  }),

  // ── 归档 / 取消归档 ─────────────────────────────────────────────────────
  http.post('*/api/projects/:id/archive', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const project = findProject(user, String(params.id))
    if (!project) return errorResponse(404, 'not_found', '项目不存在')

    project.is_archived = true
    project.updated_at = new Date().toISOString()
    return HttpResponse.json(project)
  }),

  http.post('*/api/projects/:id/unarchive', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const project = findProject(user, String(params.id))
    if (!project) return errorResponse(404, 'not_found', '项目不存在')

    project.is_archived = false
    project.updated_at = new Date().toISOString()
    return HttpResponse.json(project)
  }),

  // ── 会话加入项目 ─────────────────────────────────────────────────────────
  http.post('*/api/projects/:id/sessions', async ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const project = findProject(user, String(params.id))
    if (!project) return errorResponse(404, 'not_found', '项目不存在')

    const body = (await request.json().catch(() => null)) as {
      session_id?: string
    } | null
    if (!body?.session_id) {
      return errorResponse(400, 'bad_request', 'session_id 不能为空')
    }
    const session = sessions.find(
      (s) => s.id === body.session_id && s.user_id === user.id,
    )
    if (!session) return errorResponse(404, 'not_found', '会话不存在')

    const exists = sessionProjects.some(
      (sp) => sp.session_id === session.id && sp.project_id === project.id,
    )
    if (!exists) {
      sessionProjects.push({
        session_id: session.id,
        project_id: project.id,
        added_at: new Date().toISOString(),
      })
    }
    return new Response(null, { status: 204 })
  }),

  // ── 从项目移除会话 ───────────────────────────────────────────────────────
  http.delete('*/api/projects/:id/sessions/:session_id', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const project = findProject(user, String(params.id))
    if (!project) return errorResponse(404, 'not_found', '项目不存在')

    const idx = sessionProjects.findIndex(
      (sp) =>
        sp.project_id === project.id && sp.session_id === String(params.session_id),
    )
    if (idx < 0) return errorResponse(404, 'not_found', '关联不存在')
    sessionProjects.splice(idx, 1)
    return new Response(null, { status: 204 })
  }),

  // ── 会话所属项目 ─────────────────────────────────────────────────────────
  http.get('*/api/sessions/:id/projects', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const list = projects.filter(
      (p) =>
        p.dept_id === user.dept_id &&
        sessionProjects.some(
          (sp) => sp.session_id === String(params.id) && sp.project_id === p.id,
        ),
    )
    return HttpResponse.json({ projects: list })
  }),

  // ── 以图搜图（T-019 CLIP） ────────────────────────────────────────────────
  http.post('*/internal/images/similar', async ({ request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    let orgId = ''
    let deptId = ''
    try {
      const form = await request.formData()
      orgId = String(form.get('org_id') ?? '')
      deptId = String(form.get('dept_id') ?? '')
    } catch {
      return errorResponse(400, 'bad_request', '无法解析上传的图片')
    }

    if (orgId !== user.org_id || deptId !== user.dept_id) {
      return errorResponse(403, 'forbidden', '不能检索其他部门的图片')
    }

    await sleep(500)

    const mockStyles = [
      { name: 'A字连衣裙', seed: 'dress1' },
      { name: '落肩西装外套', seed: 'blazer2' },
      { name: '高腰阔腿裤', seed: 'pants3' },
      { name: '雪纺衬衫', seed: 'shirt4' },
      { name: '百褶半裙', seed: 'skirt5' },
      { name: '针织开衫', seed: 'knit6' },
      { name: '风衣外套', seed: 'coat7' },
      { name: '直筒牛仔裤', seed: 'jeans8' },
    ]

    const results = mockStyles.map((s, i) => ({
      image_path: `https://picsum.photos/seed/${s.seed}/480/640`,
      style_id: `mock-style-${s.seed}`,
      style_name: s.name,
      similarity: Number((0.94 - i * 0.035).toFixed(4)),
    }))

    return HttpResponse.json({ results })
  }),

  // ── 款式图片上传 ───────────────────────────────────────────────────────────
  http.post('*/api/styles/:id/images', async ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    let fileName = 'uploaded'
    try {
      const form = await request.formData()
      const file = form.get('file')
      if (file instanceof File) fileName = file.name
    } catch {
      return errorResponse(400, 'bad_request', '无法解析上传的文件')
    }

    const ext = fileName.split('.').pop()?.toLowerCase() ?? 'jpg'
    if (!['jpg', 'jpeg', 'png', 'webp'].includes(ext)) {
      return errorResponse(400, 'bad_request', '仅支持 jpg/png/webp 图片')
    }

    return HttpResponse.json(
      {
        id: newId(),
        org_id: user.org_id,
        dept_id: user.dept_id,
        style_id: String(params.id),
        image_path: `uploads/style-images/${user.org_id}/${newId()}.${ext}`,
        file_type: ext,
        uploaded_by: user.id,
        created_at: new Date().toISOString(),
      },
      { status: 201 },
    )
  }),

  // ── 款式图片列表 ───────────────────────────────────────────────────────────
  http.get('*/api/styles/:id/images', ({ params, request }) => {
    const user = authenticate(request)
    if (!user) return errorResponse(401, 'unauthorized', '未认证或 Token 无效')

    const images = [1, 2].map((i) => ({
      id: newId(),
      org_id: user.org_id,
      dept_id: user.dept_id,
      style_id: String(params.id),
      image_path: `https://picsum.photos/seed/style${i}${String(params.id).slice(0, 4)}/480/640`,
      file_type: 'jpg',
      uploaded_by: user.id,
      created_at: new Date().toISOString(),
    }))

    return HttpResponse.json({ images, total: images.length })
  }),
]

// ── 项目工具函数 ─────────────────────────────────────────────────────────────

function findProject(user: MockUser, id: string): MockProject | undefined {
  return projects.find((p) => p.id === id && p.dept_id === user.dept_id)
}

interface ProjectSessionRow {
  id: string
  title: string
  is_archived: boolean
  message_count: number
  created_at: string
  updated_at: string
  added_at: string
}

function sessionsInProject(projectId: string): ProjectSessionRow[] {
  return sessionProjects
    .filter((sp) => sp.project_id === projectId)
    .map((sp) => {
      const s = sessions.find((x) => x.id === sp.session_id)
      if (!s) return null
      return {
        id: s.id,
        title: s.title,
        is_archived: s.is_archived,
        message_count: messages.filter((m) => m.session_id === s.id).length,
        created_at: s.created_at,
        updated_at: s.updated_at,
        added_at: sp.added_at,
      }
    })
    .filter((x): x is ProjectSessionRow => x !== null)
    .sort((a, b) => b.added_at.localeCompare(a.added_at))
}
