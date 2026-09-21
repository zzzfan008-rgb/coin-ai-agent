import { useCallback, useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import {
  Menu,
  Pencil,
  Archive,
  ArchiveRestore,
  UserPlus,
  X,
  MessageSquare,
} from 'lucide-react'
import {
  archiveProject,
  getProject,
  removeSessionFromProject,
  unarchiveProject,
  updateProject,
  type ProjectDetail,
} from '../api/client'
import { useLayout } from '../components/Layout'
import { ProjectFormModal } from '../components/ProjectFormModal'
import { AddSessionsModal } from '../components/AddSessionsModal'

function formatTime(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  return d.toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  })
}

export default function ProjectPage() {
  const { id = '' } = useParams()
  const navigate = useNavigate()
  const { openSidebar, refreshAll } = useLayout()

  const [detail, setDetail] = useState<ProjectDetail | null>(null)
  const [notFound, setNotFound] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [editing, setEditing] = useState(false)
  const [adding, setAdding] = useState(false)

  const reload = useCallback(async () => {
    try {
      const p = await getProject(id)
      setDetail(p)
      setNotFound(false)
    } catch (e) {
      setNotFound(true)
      setError(e instanceof Error ? e.message : '加载失败')
    }
  }, [id])

  useEffect(() => {
    void reload()
  }, [reload])

  const handleRemoveSession = async (sessionId: string) => {
    try {
      await removeSessionFromProject(id, sessionId)
      await reload()
      await refreshAll()
    } catch (e) {
      setError(e instanceof Error ? e.message : '移除失败')
    }
  }

  if (notFound && !detail) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center text-center">
        <p className="text-sm font-medium text-content">项目不存在</p>
        <p className="mt-2 text-xs text-faint">{error}</p>
        <button
          type="button"
          onClick={() => navigate('/')}
          className="mt-4 rounded-lg bg-primary px-4 py-2 text-sm text-white"
        >
          返回首页
        </button>
      </div>
    )
  }

  if (!detail) {
    return <p className="flex flex-1 items-center justify-center text-sm text-faint">加载中…</p>
  }

  const archived = detail.is_archived

  return (
    <>
      <div className="flex min-w-0 flex-1 flex-col">
        {/* 顶栏 */}
        <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
          <button
            type="button"
            onClick={openSidebar}
            className="rounded-md p-1.5 text-muted hover:bg-surface hover:text-content lg:hidden"
            title="侧边栏"
          >
            <Menu size={18} />
          </button>
          <span
            className="h-4 w-4 shrink-0 rounded"
            style={{ backgroundColor: detail.cover_color }}
          />
          <h1 className="min-w-0 flex-1 truncate text-sm font-medium text-content">
            {detail.name}
          </h1>
          {archived ? (
            <button
              type="button"
              onClick={async () => {
                await unarchiveProject(id)
                await reload()
                await refreshAll()
              }}
              title="取消归档"
              className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs text-muted hover:bg-surface hover:text-content"
            >
              <ArchiveRestore size={15} />
              <span className="hidden sm:inline">取消归档</span>
            </button>
          ) : (
            <>
              <button
                type="button"
                onClick={() => setAdding(true)}
                title="添加会话"
                className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs text-muted hover:bg-surface hover:text-content"
              >
                <UserPlus size={15} />
                <span className="hidden sm:inline">添加会话</span>
              </button>
              <button
                type="button"
                onClick={() => setEditing(true)}
                title="编辑项目"
                className="rounded-md p-1.5 text-muted hover:bg-surface hover:text-content"
              >
                <Pencil size={16} />
              </button>
              <button
                type="button"
                onClick={async () => {
                  if (!window.confirm(`确定归档项目「${detail.name}」？`)) return
                  await archiveProject(id)
                  await reload()
                  await refreshAll()
                }}
                title="归档项目"
                className="rounded-md p-1.5 text-muted hover:bg-surface hover:text-content"
              >
                <Archive size={16} />
              </button>
            </>
          )}
        </header>

        {error && (
          <div className="shrink-0 border-b border-error/30 bg-error/10 px-4 py-2 text-xs text-error">
            {error}
          </div>
        )}

        {/* 项目信息 */}
        <div className="shrink-0 border-b border-border bg-surface/50 px-6 py-4">
          <div className="flex items-center gap-2">
            <h2 className="text-base font-semibold text-content">{detail.name}</h2>
            {archived && (
              <span className="rounded bg-surface-elevated px-2 py-0.5 text-[11px] text-faint">
                已归档
              </span>
            )}
          </div>
          {detail.description ? (
            <p className="mt-1 text-sm text-muted">{detail.description}</p>
          ) : (
            <p className="mt-1 text-sm text-faint">暂无描述</p>
          )}
          <p className="mt-1 text-[11px] text-faint">
            创建于 {formatTime(detail.created_at)} · {detail.sessions.length} 个会话
          </p>
        </div>

        {/* 会话卡片 */}
        <div className="flex-1 overflow-y-auto px-6 py-5">
          {detail.sessions.length === 0 ? (
            <div className="flex flex-col items-center justify-center pt-16 text-center">
              <MessageSquare size={28} className="text-faint" />
              <p className="mt-3 text-sm text-muted">项目中还没有会话</p>
              {!archived && (
                <button
                  type="button"
                  onClick={() => setAdding(true)}
                  className="mt-3 rounded-lg bg-primary px-4 py-2 text-sm text-white hover:bg-primary-hover"
                >
                  添加会话
                </button>
              )}
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
              {detail.sessions.map((s) => (
                <div
                  key={s.id}
                  className="group relative rounded-xl border border-border bg-surface p-4 transition-colors hover:border-primary/50"
                >
                  <button
                    type="button"
                    onClick={() => navigate(`/sessions/${s.id}`)}
                    className="block w-full text-left"
                  >
                    <div className="flex items-center gap-2 pr-6">
                      <MessageSquare size={15} className="shrink-0 text-primary-light" />
                      <span className="truncate text-sm font-medium text-content">
                        {s.title}
                      </span>
                    </div>
                    <div className="mt-3 flex items-center justify-between text-[11px] text-faint">
                      <span>{s.message_count ?? 0} 条消息</span>
                      <span>{formatTime(s.updated_at)}</span>
                    </div>
                  </button>
                  {!archived && (
                    <button
                      type="button"
                      onClick={() => void handleRemoveSession(s.id)}
                      title="从项目移除"
                      className="absolute right-2.5 top-2.5 rounded-md p-1 text-faint opacity-0 transition-opacity hover:bg-surface-elevated hover:text-error group-hover:opacity-100"
                    >
                      <X size={14} />
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* 编辑弹窗 */}
      {editing && (
        <ProjectFormModal
          project={detail}
          onClose={() => setEditing(false)}
          onSubmit={async (input) => {
            await updateProject(id, input)
            await reload()
            await refreshAll()
          }}
        />
      )}

      {/* 添加会话弹窗 */}
      {adding && (
        <AddSessionsModal
          projectId={id}
          existingSessionIds={detail.sessions.map((s) => s.id)}
          onClose={() => setAdding(false)}
          onDone={() => {
            setAdding(false)
            void reload().then(() => refreshAll())
          }}
        />
      )}
    </>
  )
}
