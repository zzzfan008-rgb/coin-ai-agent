import { useEffect, useState } from 'react'
import { Check, FolderPlus, X } from 'lucide-react'
import {
  addSessionToProject,
  listProjects,
  listProjectsForSession,
  removeSessionFromProject,
  type Project,
} from '../api/client'

interface AddToProjectsModalProps {
  sessionId: string
  onClose: () => void
}

export function AddToProjectsModal({
  sessionId,
  onClose,
}: AddToProjectsModalProps) {
  const [projects, setProjects] = useState<Project[]>([])
  const [memberIds, setMemberIds] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const [all, members] = await Promise.all([
          listProjects(false),
          listProjectsForSession(sessionId),
        ])
        if (!cancelled) {
          setProjects(all.projects)
          setMemberIds(new Set(members.projects.map((p) => p.id)))
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : '加载失败')
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [sessionId])

  const toggle = async (project: Project) => {
    if (saving) return
    const next = new Set(memberIds)
    const willJoin = !next.has(project.id)
    if (willJoin) next.add(project.id)
    else next.delete(project.id)
    setMemberIds(next) // optimistic
    setSaving(true)
    setError(null)
    try {
      if (willJoin) {
        await addSessionToProject(project.id, sessionId)
      } else {
        await removeSessionFromProject(project.id, sessionId)
      }
    } catch (e) {
      // rollback
      setMemberIds(new Set(memberIds))
      setError(e instanceof Error ? e.message : '操作失败')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={() => !saving && onClose()} />
      <div className="relative z-10 flex max-h-[80vh] w-full max-w-lg flex-col rounded-xl border border-border bg-surface p-5 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <FolderPlus size={17} className="text-primary-light" />
            <h2 className="text-base font-semibold text-content">添加到项目</h2>
          </div>
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
          ) : projects.length === 0 ? (
            <div className="py-8 text-center text-sm text-faint">
              暂无项目，请先在侧边栏创建项目
            </div>
          ) : (
            <ul className="space-y-1">
              {projects.map((p) => {
                const checked = memberIds.has(p.id)
                return (
                  <li key={p.id}>
                    <button
                      type="button"
                      onClick={() => void toggle(p)}
                      disabled={saving}
                      className={`flex w-full items-center gap-3 rounded-lg border px-3 py-2.5 text-left transition-colors disabled:opacity-60 ${
                        checked
                          ? 'border-primary bg-primary/10'
                          : 'border-border bg-bg hover:border-faint'
                      }`}
                    >
                      <span
                        className="h-7 w-1.5 shrink-0 rounded-full"
                        style={{ backgroundColor: p.cover_color }}
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm text-content">
                          {p.name}
                        </span>
                        {p.description && (
                          <span className="block truncate text-[11px] text-faint">
                            {p.description}
                          </span>
                        )}
                      </span>
                      <span
                        className={`flex h-[18px] w-[18px] items-center justify-center rounded-[5px] border ${
                          checked
                            ? 'border-primary bg-primary text-white'
                            : 'border-border text-transparent'
                        }`}
                      >
                        <Check size={12} />
                      </span>
                    </button>
                  </li>
                )
              })}
            </ul>
          )}
        </div>

        {error && <p className="mt-3 text-xs text-error">{error}</p>}

        <div className="mt-4 flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-white hover:bg-primary-hover"
          >
            完成
          </button>
        </div>
      </div>
    </div>
  )
}
