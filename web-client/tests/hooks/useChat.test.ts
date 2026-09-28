/**
 * useChat 单元测试
 *
 * 测试点：
 * 1. 初始化：创建新会话（无会话时）/ 加载已有会话
 * 2. 消息追加：sendMessage 乐观更新 + 流式回复
 * 3. 会话管理：newSession 创建新会话
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act, waitFor } from '@testing-library/react'
import { useChat } from '../../src/hooks/useChat'
import * as client from '../../src/api/client'

// ── Mock（必须用 vi.hoisted，vi.mock 工厂才能引用）─────────────────────────────

const { mockListSessions, mockListMessages, mockCreateSession, mockChatCompletion } =
  vi.hoisted(() => ({
    mockListSessions: vi.fn(),
    mockListMessages: vi.fn(),
    mockCreateSession: vi.fn(),
    mockChatCompletion: vi.fn(),
  }))

vi.mock('../../src/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof client>()
  return {
    ...actual,
    listSessions: mockListSessions,
    listMessages: mockListMessages,
    createSession: mockCreateSession,
    chatCompletion: mockChatCompletion,
  }
})

// ── 测试数据 ─────────────────────────────────────────────────────────────────

const fakeSession = {
  id: 's-1',
  title: '会话1',
  is_archived: false,
  created_at: '2024-01-01T00:00:00Z',
  updated_at: '2024-01-01T00:00:00Z',
}

const fakeMessages = [
  {
    id: 'm-1',
    role: 'user' as const,
    content: 'Hello',
    created_at: '2024-01-01T00:00:00Z',
  },
  {
    id: 'm-2',
    role: 'assistant' as const,
    content: 'Hi there!',
    created_at: '2024-01-01T00:01:00Z',
  },
]

// ── Tests ───────────────────────────────────────────────────────────────────

describe('useChat 初始化', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('无会话时自动创建新会话', async () => {
    mockListSessions.mockResolvedValue({ sessions: [], total: 0 })
    mockCreateSession.mockResolvedValue({
      id: 's-new',
      title: '新会话',
      is_archived: false,
      created_at: '2024-01-01T00:00:00Z',
      updated_at: '2024-01-01T00:00:00Z',
    })

    const { result } = renderHook(() => useChat())

    expect(result.current.loaded).toBe(false)

    await waitFor(() => {
      expect(result.current.loaded).toBe(true)
    })

    expect(mockCreateSession).toHaveBeenCalled()
    expect(result.current.currentSession).toMatchObject({ id: 's-new' })
  })

  it('有会话时加载第一个会话', async () => {
    mockListSessions.mockResolvedValue({ sessions: [fakeSession], total: 1 })
    mockListMessages.mockResolvedValue({ messages: fakeMessages, has_more: false })

    const { result } = renderHook(() => useChat())

    await waitFor(() => {
      expect(result.current.loaded).toBe(true)
    })

    expect(mockListSessions).toHaveBeenCalled()
    expect(result.current.sessions).toHaveLength(1)
    expect(result.current.currentSession).toMatchObject({ id: fakeSession.id })
    expect(result.current.messages).toHaveLength(2)
  })
})

describe('useChat 消息追加', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // 所有消息追加测试共用同样的初始化数据
    mockListSessions.mockResolvedValue({ sessions: [fakeSession], total: 1 })
    mockListMessages.mockResolvedValue({ messages: fakeMessages, has_more: false })
  })

  it('sendMessage 乐观追加用户消息', async () => {
    mockChatCompletion.mockImplementation(() => Promise.resolve('This is the AI reply.'))

    const { result } = renderHook(() => useChat())
    await waitFor(() => expect(result.current.loaded).toBe(true))

    await act(async () => {
      await result.current.sendMessage('Hello')
    })

    // 用户消息应出现在列表中
    const userMsg = result.current.messages.find(
      (m: client.SessionMessage) => m.role === 'user',
    )
    expect(userMsg?.content).toBe('Hello')
  })

  it('sendMessage 完成后追加 assistant 消息', async () => {
    mockChatCompletion.mockImplementation(() => Promise.resolve('AI 回复内容。'))

    const { result } = renderHook(() => useChat())
    await waitFor(() => expect(result.current.loaded).toBe(true))

    await act(async () => {
      await result.current.sendMessage('Hello')
    })

    await waitFor(() => {
      expect(
        result.current.messages.some((m: client.SessionMessage) => m.role === 'assistant'),
      ).toBe(true)
    })

    // 最后一条 assistant 消息内容匹配（历史消息 + 新增的）
    const allAssistant = result.current.messages.filter(
      (m: client.SessionMessage) => m.role === 'assistant',
    )
    const lastAssistant = allAssistant[allAssistant.length - 1]
    expect(lastAssistant.content).toBe('AI 回复内容。')
  })

  it('sendMessage 结束后 isStreaming 恢复 false', async () => {
    mockChatCompletion.mockImplementation(
      () => new Promise<string>((r) => setTimeout(() => r('延迟回复'), 50)),
    )

    const { result } = renderHook(() => useChat())
    await waitFor(() => expect(result.current.loaded).toBe(true))

    await act(async () => {
      await result.current.sendMessage('Hello')
    })

    // 结束后流式状态应恢复
    expect(result.current.isStreaming).toBe(false)
  })
})

describe('useChat 会话管理', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('newSession 创建会话并追加到列表', async () => {
    mockListSessions.mockResolvedValue({ sessions: [fakeSession], total: 1 })
    mockListMessages.mockResolvedValue({ messages: [], has_more: false })

    const newSession = {
      id: 's-new',
      title: '新会话',
      is_archived: false,
      created_at: '2024-01-02T00:00:00Z',
      updated_at: '2024-01-02T00:00:00Z',
    }
    mockCreateSession.mockResolvedValue(newSession)

    const { result } = renderHook(() => useChat())

    await waitFor(() => expect(result.current.loaded).toBe(true))

    // 初始 1 个会话
    expect(result.current.sessions).toHaveLength(1)

    await act(async () => {
      await result.current.newSession()
    })

    expect(mockCreateSession).toHaveBeenCalled()
    expect(result.current.sessions).toHaveLength(2)
    expect(result.current.sessions[0].id).toBe('s-new')
  })
})
