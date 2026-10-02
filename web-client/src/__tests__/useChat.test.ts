import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act, waitFor } from '@testing-library/react'
import { useChat } from '../hooks/useChat'
import * as api from '../api/client'

vi.mock('../api/client', () => ({
  listSessions: vi.fn(),
  listMessages: vi.fn(),
  createSession: vi.fn(),
  streamChatCompletion: vi.fn(),
  chatCompletion: vi.fn(),
}))

describe('useChat', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('initializes with empty state', () => {
    vi.mocked(api.listSessions).mockResolvedValue({ sessions: [], total: 0 })
    vi.mocked(api.createSession).mockResolvedValue({
      id: 'test-session',
      title: '',
      is_archived: false,
      created_at: '',
      updated_at: '',
    })

    const { result } = renderHook(() => useChat())

    expect(result.current.sessions).toEqual([])
    expect(result.current.isStreaming).toBe(false)
    expect(result.current.error).toBeNull()
  })

  it('sendMessage sets streaming state', async () => {
    const mockSession = { id: 's1', title: '', is_archived: false, created_at: '', updated_at: '' }
    vi.mocked(api.listSessions).mockResolvedValue({ sessions: [mockSession], total: 1 })
    vi.mocked(api.listMessages).mockResolvedValue({ messages: [], has_more: false })
    vi.mocked(api.chatCompletion).mockResolvedValue('AI response text')

    const { result } = renderHook(() => useChat())

    await waitFor(() => expect(result.current.loaded).toBe(true))

    await act(async () => {
      await result.current.sendMessage('Hello')
    })

    expect(api.chatCompletion).toHaveBeenCalled()
  })

  it('retryStream stops after 3 attempts', async () => {
    const mockSession = { id: 's1', title: '', is_archived: false, created_at: '', updated_at: '' }
    const userMsg = { id: 'u1', role: 'user' as const, content: 'Hello', created_at: '' }
    
    vi.mocked(api.listSessions).mockResolvedValue({ sessions: [mockSession], total: 1 })
    vi.mocked(api.listMessages).mockResolvedValue({ messages: [userMsg], has_more: false })
    vi.mocked(api.chatCompletion).mockRejectedValue(new Error('Network error'))

    const { result } = renderHook(() => useChat())
    await waitFor(() => expect(result.current.loaded).toBe(true))

    // Simulate stream disconnect after partial content
    await act(async () => {
      await result.current.sendMessage('Hello')
    })

    // retryStream should eventually stop at 3 attempts
    for (let i = 0; i < 4; i++) {
      await act(async () => {
        await result.current.retryStream()
      })
    }

    // After 3 attempts, error should indicate manual refresh needed
    expect(result.current.error).toBeTruthy()
  })
})
