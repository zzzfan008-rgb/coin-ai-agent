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

export function useChat() {
  const [sessions, setSessions] = useState<Session[]>([])
  const [currentSession, setCurrentSession] = useState<Session | null>(null)
  const [messages, setMessages] = useState<SessionMessage[]>([])
  const [isStreaming, setIsStreaming] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [selectedSkillIds, setSelectedSkillIds] = useState<string[]>([])
  const [loaded, setLoaded] = useState(false)

  const abortRef = useRef<AbortController | null>(null)

  // ── 初始化：加载会话列表，无会话则自动新建 ─────────────────────────────
  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const { sessions: list } = await listSessions()
        if (cancelled) return
        if (list.length > 0) {
          setSessions(list)
          const first = list[0]
          setCurrentSession(first)
          const { messages: msgs } = await listMessages(first.id)
          if (!cancelled) setMessages(msgs)
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
  }, [])

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
      setError(null)
      setCurrentSession(session)
      try {
        const { messages: msgs } = await listMessages(session.id)
        setMessages(msgs)
      } catch (e) {
        setError(e instanceof Error ? e.message : '消息加载失败')
      }
    },
    [isStreaming],
  )

  const newSession = useCallback(async () => {
    if (isStreaming) return
    setError(null)
    try {
      const s = await createSession()
      setSessions((prev) => [s, ...prev])
      setCurrentSession(s)
      setMessages([])
    } catch (e) {
      setError(e instanceof Error ? e.message : '创建会话失败')
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

      try {
        await streamChatCompletion(
          {
            messages: payload,
            sessionId: currentSession.id,
            skillIds: selectedSkillIds,
            signal: controller.signal,
          },
          (chunk) => {
            patchAssistant((m) => ({ ...m, content: m.content + chunk }))
          },
        )
      } catch (e) {
        if (controller.signal.aborted) {
          patchAssistant((m) => ({
            ...m,
            content: m.content + (m.content ? '\n\n_（已停止生成）_' : ''),
          }))
        } else {
          const msg = e instanceof Error ? e.message : 'AI 回复失败'
          setError(msg)
          patchAssistant((m) => ({
            ...m,
            content: m.content || `⚠️ ${msg}`,
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

  return {
    sessions,
    currentSession,
    messages,
    isStreaming,
    error,
    loaded,
    selectedSkillIds,
    selectSession,
    newSession,
    sendMessage,
    stopStreaming,
    toggleSkill,
  }
}
