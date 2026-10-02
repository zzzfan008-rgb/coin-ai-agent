import { useCallback, useEffect, useRef, useState } from 'react'
import {
  createSession,
  listMessages,
  listSessions,
  streamChatCompletion,
  type ChatMessage,
  type Session,
  type SessionMessage,
} from '../api/client'

let tempSeq = 0
function tempId(): string {
  tempSeq += 1
  return `temp-${Date.now()}-${tempSeq}`
}

export function useChat(initialSessionId?: string) {
  const [sessions, setSessions] = useState<Session[]>([])
  const [currentSession, setCurrentSession] = useState<Session | null>(null)
  const [messages, setMessages] = useState<SessionMessage[]>([])
  const [isStreaming, setIsStreaming] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // T-022: stream-disconnect flag — distinct from transient errors so
  // the UI can show a dedicated "connection lost" banner instead of
  // re-throwing the same fetch error on every keystroke.
  const [streamDisconnected, setStreamDisconnected] = useState(false)
  const [selectedSkillIds, setSelectedSkillIds] = useState<string[]>([])
  const [loaded, setLoaded] = useState(false)

  const abortRef = useRef<AbortController | null>(null)
  // T-022: consecutive stream-reconnect attempts (max 3), reset on success.
  const retryAttemptRef = useRef(0)

  const loadSession = useCallback(
    async (s: Session) => {
      setError(null)
      setCurrentSession(s)
      try {
        const { messages: msgs } = await listMessages(s.id)
        setMessages(msgs ?? [])
      } catch (e) {
        setError(e instanceof Error ? e.message : '消息加载失败')
      }
    },
    [],
  )

  // ── 初始化：加载会话列表，选中指定/首个会话，无会话则自动新建 ────────────
  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const { sessions: list } = await listSessions()
        if (cancelled) return
        setSessions(list)

        const target =
          (initialSessionId && list.find((s) => s.id === initialSessionId)) ||
          list[0]

        if (target) {
          await loadSession(target)
        } else {
          const s = await createSession()
          if (!cancelled) {
            setSessions([s])
            setCurrentSession(s)
            setMessages([])
          }
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : '加载失败')
      } finally {
        if (!cancelled) setLoaded(true)
      }
    })()
    return () => {
      cancelled = true
    }
    // 仅在挂载时执行；后续路由切换由下面的 effect 处理
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // ── 路由驱动的会话切换（/sessions/:id） ──────────────────────────────────
  useEffect(() => {
    if (!loaded || !initialSessionId) return
    if (initialSessionId === currentSession?.id) return
    const target = sessions.find((s) => s.id === initialSessionId)
    if (target) void loadSession(target)
  }, [initialSessionId, loaded, sessions, currentSession, loadSession])

  const refreshSessions = useCallback(async () => {
    try {
      const { sessions: list } = await listSessions()
      setSessions(list)
      return list
    } catch {
      return null
    }
  }, [])

  const selectSession = useCallback(
    async (session: Session) => {
      if (isStreaming) return
      await loadSession(session)
    },
    [isStreaming, loadSession],
  )

  const newSession = useCallback(async () => {
    if (isStreaming) return
    setError(null)
    try {
      const s = await createSession()
      setSessions((prev) => [s, ...prev])
      setCurrentSession(s)
      setMessages([])
      return s
    } catch (e) {
      setError(e instanceof Error ? e.message : '创建会话失败')
      return null
    }
  }, [isStreaming])

  const toggleSkill = useCallback((skillId: string) => {
    setSelectedSkillIds((prev) =>
      prev.includes(skillId)
        ? prev.filter((id) => id !== skillId)
        : [...prev, skillId],
    )
  }, [])

  const stopStreaming = useCallback(() => {
    abortRef.current?.abort()
  }, [])

  const sendMessage = useCallback(
    async (text: string) => {
      const content = text.trim()
      if (!content || isStreaming || !currentSession) return

      setError(null)
      const ts = new Date().toISOString()
      const userMsg: SessionMessage = {
        id: tempId(),
        role: 'user',
        content,
        created_at: ts,
      }
      const assistantId = tempId()
      const assistantMsg: SessionMessage = {
        id: assistantId,
        role: 'assistant',
        content: '',
        model: 'fashion-ai-default',
        created_at: ts,
      }
      const history = [...messages, userMsg]
      setMessages([...history, assistantMsg])
      setIsStreaming(true)

      const controller = new AbortController()
      abortRef.current = controller

      const patchAssistant = (updater: (m: SessionMessage) => SessionMessage) => {
        setMessages((prev) =>
          prev.map((m) => (m.id === assistantId ? updater(m) : m)),
        )
      }

      const payload: ChatMessage[] = history.map((m) => ({
        role: m.role === 'tool' ? 'tool' : m.role,
        content: m.content,
      }))

      // Stream deltas incrementally: the SSE path (streamChatCompletion)
      // gives the typewriter effect and its 60s connect / 30s idle guards.
      // streamedText tracks what actually arrived so the catch block can
      // tell a mid-reply disconnect (show reconnect banner) from a
      // pre-first-byte failure (plain error) — assistantMsg.content is a
      // stale closure snapshot and can never be used for that.
      let streamedText = ''
      try {
        const text = await streamChatCompletion(
          {
            messages: payload,
            sessionId: currentSession.id,
            skillIds: selectedSkillIds,
            signal: controller.signal,
          },
          (delta) => {
            streamedText += delta
            patchAssistant((m) => ({ ...m, content: m.content + delta }))
          },
        )
        streamedText = text
        patchAssistant((m) => ({ ...m, content: text }))
        // Successful stream: reset the reconnect-attempt counter so later
        // unrelated disconnects get a fresh backoff budget.
        retryAttemptRef.current = 0
      } catch (e) {
        if (controller.signal.aborted) {
          patchAssistant((m) => ({
            ...m,
            content: m.content + (m.content ? '\n\n_（已停止生成）_' : ''),
          }))
        } else {
          // T-022: distinguish stream-disconnect from other errors.
          // If partial content arrived before the error, the stream was
          // interrupted mid-reply → show a friendly reconnect prompt.
          const hadContent = streamedText !== ''
          if (hadContent) {
            setStreamDisconnected(true)
          }
          const msg = e instanceof Error ? e.message : 'AI 回复失败'
          setError(hadContent ? null : msg)
          patchAssistant((m) => ({
            ...m,
            content: m.content || `⚠️ 连接中断，请重试`,
          }))
        }
      } finally {
        setIsStreaming(false)
        abortRef.current = null
        const latest = await refreshSessions()
        if (latest) {
          setCurrentSession((cur) => {
            if (!cur) return cur
            return latest.find((s) => s.id === cur.id) ?? cur
          })
        }
      }
    },
    [messages, isStreaming, currentSession, selectedSkillIds, refreshSessions],
  )

  // T-022: manual reconnect with exponential backoff (max 3 attempts).
  // Resends the last user message to re-establish the stream.
  const retryStream = useCallback(async () => {
    if (!streamDisconnected || messages.length === 0 || isStreaming) return
    const attempt = retryAttemptRef.current
    if (attempt >= 3) {
      setStreamDisconnected(false)
      retryAttemptRef.current = 0
      setError('连接中断，请检查网络后手动刷新页面重试')
      return
    }
    const lastUser = [...messages].reverse().find((m) => m.role === 'user')
    if (lastUser) {
      retryAttemptRef.current = attempt + 1
      setStreamDisconnected(false)
      setError(null)
      // exponential backoff: 1s, 2s, 4s
      await new Promise((r) => setTimeout(r, 1000 * Math.pow(2, attempt)))
      await sendMessage(lastUser.content)
    }
  }, [streamDisconnected, messages, isStreaming, sendMessage])

  return {
    sessions,
    currentSession,
    messages,
    isStreaming,
    error,
    streamDisconnected,
    loaded,
    selectedSkillIds,
    selectSession,
    newSession,
    sendMessage,
    stopStreaming,
    retryStream,
    toggleSkill,
  }
}
