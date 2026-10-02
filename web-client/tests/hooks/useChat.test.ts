/**
 * useChat 单元测试
 *
 * 测试点：
 * 1. 初始化：创建新会话（无会话时）/ 加载已有会话
 * 2. 消息追加：sendMessage 乐观更新 + 流式回复（SSE onDelta 逐块写入）
 * 3. 会话管理：newSession 创建新会话
 * 4. T-022 重连：retryStream 退避重试上限 3 次，超出后提示手动刷新
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act, waitFor } from '@testing-library/react'
import { useChat } from '../../src/hooks/useChat'
import * as client from '../../src/api/client'

// ── Mock（必须用 vi.hoisted，vi.mock 工厂才能引用）─────────────────────────────

const { mockListSessions, mockListMessages, mockCreateSession, mockStreamChatCompletion } =
  vi.hoisted(() => ({
    mockListSessions: vi.fn(),
    mockListMessages: vi.fn(),
    mockCreateSession: vi.fn(),
    mockStreamChatCompletion: vi.fn(),
  }))

vi.mock('../../src/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof client>()
  return {
    ...actual,
    listSessions: mockListSessions,
    listMessages: mockListMessages,
    createSession: mockCreateSession,
    streamChatCompletion: mockStreamChatCompletion,
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

// 模拟 SSE 正常完成：逐块回调 onDelta 后返回全文
function okStream(fullText: string, chunkSize = 5) {
  return (_params: unknown, onDelta: (c: string) => void) => {
    for (let i = 0; i < fullText.length; i += chunkSize) {
      onDelta(fullText.slice(i, i + chunkSize))
    }
    return Promise.resolve(fullText)
  }
}

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
    mockStreamChatCompletion.mockImplementation(okStream('This is the AI reply.'))

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
    mockStreamChatCompletion.mockImplementation(okStream('AI 回复内容。'))

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

  it('流式 onDelta 逐块累积写入 assistant 消息', async () => {
    mockStreamChatCompletion.mockImplementation(okStream('abcdefghij', 3))

    const { result } = renderHook(() => useChat())
    await waitFor(() => expect(result.current.loaded).toBe(true))

    await act(async () => {
      await result.current.sendMessage('Hello')
    })

    const allAssistant = result.current.messages.filter(
      (m: client.SessionMessage) => m.role === 'assistant',
    )
    const lastAssistant = allAssistant[allAssistant.length - 1]
    // 3+3+3+1 四块累积后应等于完整文本（无重复、无丢失）
    expect(lastAssistant.content).toBe('abcdefghij')
  })

  it('sendMessage 结束后 isStreaming 恢复 false', async () => {
    mockStreamChatCompletion.mockImplementation(
      (_params: unknown, onDelta: (c: string) => void) =>
        new Promise<string>((r) => {
          setTimeout(() => {
            onDelta('延迟回复')
            r('延迟回复')
          }, 50)
        }),
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

describe('useChat T-022 断连重连', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockListSessions.mockResolvedValue({ sessions: [fakeSession], total: 1 })
    mockListMessages.mockResolvedValue({ messages: fakeMessages, has_more: false })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('部分输出后断连 → streamDisconnected 置位，重连成功后归位', async () => {
    // 首次：先吐 2 块内容再 reject（模拟中途断流）
    mockStreamChatCompletion.mockImplementationOnce(
      (_params: unknown, onDelta: (c: string) => void) => {
        onDelta('部分内容')
        return Promise.reject(new Error('连接中断：流式响应超时（60 秒未收到数据）'))
      },
    )

    const { result } = renderHook(() => useChat())
    await waitFor(() => expect(result.current.loaded).toBe(true))

    await act(async () => {
      await result.current.sendMessage('Hello')
    })

    // 断连横幅应出现（部分输出后失败，而非普通 error）
    expect(result.current.streamDisconnected).toBe(true)

    // 重连：成功完成流 → 归位
    mockStreamChatCompletion.mockImplementation(okStream('完整回复'))
    vi.useFakeTimers()
    await act(async () => {
      const p = result.current.retryStream()
      await vi.advanceTimersByTimeAsync(1000) // 跳过 1s 退避
      await p
    })

    expect(result.current.streamDisconnected).toBe(false)
    const allAssistant = result.current.messages.filter(
      (m: client.SessionMessage) => m.role === 'assistant',
    )
    expect(allAssistant[allAssistant.length - 1].content).toBe('完整回复')
  })

  it('retryStream 在 3 次重试耗尽后停止并提示手动刷新', async () => {
    // 每次都部分输出后断连
    mockStreamChatCompletion.mockImplementation(
      (_params: unknown, onDelta: (c: string) => void) => {
        onDelta('部分')
        return Promise.reject(new Error('连接中断：流式响应超时（60 秒未收到数据）'))
      },
    )

    const { result } = renderHook(() => useChat())
    await waitFor(() => expect(result.current.loaded).toBe(true))

    await act(async () => {
      await result.current.sendMessage('Hello')
    })
    expect(result.current.streamDisconnected).toBe(true)

    // 3 次真实重连：1s、2s、4s 退避（fake timers 快进）；每次仍以
    // 部分输出后断连告终 → 横幅在每次失败后重新置位
    vi.useFakeTimers()
    for (const delay of [1000, 2000, 4000]) {
      await act(async () => {
        const p = result.current.retryStream()
        await vi.advanceTimersByTimeAsync(delay)
        await p
      })
      expect(result.current.streamDisconnected).toBe(true)
    }

    // 第 4 次调用：计数器已达 3 上限 → 不再发起请求，横幅清除，
    // 转为普通 error 提示手动刷新
    mockStreamChatCompletion.mockClear()
    await act(async () => {
      await result.current.retryStream()
    })
    expect(mockStreamChatCompletion).not.toHaveBeenCalled()
    expect(result.current.streamDisconnected).toBe(false)
    expect(result.current.error).toContain('手动刷新')
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
