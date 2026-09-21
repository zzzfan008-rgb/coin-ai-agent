import { useEffect, useMemo, useState } from 'react'
import { Check, X } from 'lucide-react'
import { addSessionToProject, listSessions, type Session } from '../api/client'

interface AddSessionsModalProps {
  projectId: string
  existingSessionIds: string[]
  onClose: () => void
  onDone: () => void
}

export function AddSessionsModal({
  projectId,
  existingSessionIds,
  onClose,
  onDone,
}: AddSessionsModalProps) {
  const [sessions, setSessions] = useState<Session[]>([])
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const { sessions: list } = await listSessions()
        if (!cancelled) setSessions(list)
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : '会话加载失败')
        }
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])

  const candidates = useMemo(
    () => sessions.filter((s) => !existingSessionIds.includes(s.id)),
    [sessions, existingSessionIds],
  )

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const handleSubmit = async () => {
    if (selected.size === 0) {
      setError('请至少选择一个会话')
      return
    }
    setSaving(true)
    setError(null)
    try {
      await Promise.all(
        [...selected].map((sid) => addSessionToProject(projectId, sid)),
      )
      onDone()
    } catch (e) {
      setError(e instanceof Error ? e.message : '添加失败')
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={() => !saving && onClose()}
      />
      <div className="relative z-10 flex max-h-[80vh] w-full max-w-lg flex-col rounded-xl border border-border bg-surface p-5 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold text-content">添加会话到项目</h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-faint hover:text-content"
          >
            <X size={18} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {loading ? (
            <p className="py-8 text-center text-sm text-faint">加载中…</p>
          ) : candidates.length === 0 ? (
            <p className="py-8 text-center text-sm text-faint">
              所有会话都已在此项目中
            </p>
          ) : (
            <ul className="space-y-1">
              {candidates.map((s) => {
                const checked = selected.has(s.id)
                return (
                  <li key={s.id}>
                    <button
                      type="button"
                      onClick={() => toggle(s.id)}
                      className={`flex w-full items-center gap-3 rounded-lg border px-3 py-2.5 text-left transition-colors ${
                        checked
                          ? 'border-primary bg-primary/10'
                          : 'border-border bg-bg hover:border-faint'
                      }`}
                    >
                      <span
                        className={`flex h-[18px] w-[18px] items-center justify-center rounded-[5px] border ${
                          checked
                            ? 'border-primary bg-primary text-white'
                            : 'border-border text-transparent'
                        }`}
                      >
                        <Check size={12} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm text-content">
                          {s.title}
                        </span>
                        <span className="text-[11px] text-faint">
                          {s.message_count ?? 0} 条消息
                        </span>
                      </span>
                    </button>
                  </li>
                )
              })}
            </ul>
          )}
        </div>

        {error && <p className="mt-3 text-xs text-error">{error}</p>}

        <div className="mt-4 flex items-center justify-between">
          <span className="text-xs text-faint">已选 {selected.size} 个</span>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={onClose}
              disabled={saving}
              className="rounded-lg px-4 py-2 text-sm text-muted hover:bg-surface-elevated"
            >
              取消
            </button>
            <button
              type="button"
              onClick={() => void handleSubmit()}
              disabled={saving}
              className="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-white hover:bg-primary-hover disabled:opacity-50"
            >
              {saving ? '添加中…' : '添加'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
