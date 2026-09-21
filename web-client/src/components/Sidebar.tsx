import {
  Plus,
  MessageSquare,
  LogOut,
  X,
  Shirt,
  Folder,
  ChevronDown,
  ChevronRight,
  Archive,
  BookOpen,
  Plug,
} from 'lucide-react'
import { useState } from 'react'
import { useNavigate, useLocation } from 'react-router-dom'
import type { Session, Project, AuthUser } from '../api/client'

interface SidebarProps {
  sessions: Session[]
  projects: Project[]
  archivedProjects: Project[]
  currentSessionId?: string
  currentProjectId?: string
  user: AuthUser | null
  onNewSession: () => void
  onCreateProject: () => void
  onLogout: () => void
  onClose?: () => void
  disabled?: boolean
}

function formatTime(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  const now = new Date()
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
  }
  return d.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' })
}

export function Sidebar({
  sessions,
  projects,
  archivedProjects,
  currentSessionId,
  currentProjectId,
  user,
  onNewSession,
  onCreateProject,
  onLogout,
  onClose,
  disabled,
}: SidebarProps) {
  const navigate = useNavigate()
  const location = useLocation()
  const [archivedOpen, setArchivedOpen] = useState(false)

  const chatActive = location.pathname === '/' || location.pathname.startsWith('/sessions/')

  return (
    <div className="flex h-full w-64 flex-col border-r border-border bg-surface">
      {/* 品牌 */}
      <div className="flex items-center justify-between px-4 py-3.5">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 items-center justify-center rounded-md bg-primary">
            <Shirt size={16} className="text-white" />
          </div>
          <span className="text-[15px] font-semibold text-content">Fashion AI</span>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-faint hover:text-content lg:hidden"
            title="收起侧边栏"
          >
            <X size={18} />
          </button>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3">
        {/* 新建会话 */}
        <button
          type="button"
          onClick={onNewSession}
          disabled={disabled}
          className="mb-2 flex w-full items-center justify-center gap-1.5 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-primary-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Plus size={16} />
          新建会话
        </button>

        {/* 会话区 */}
        <p className="px-1 pb-1 pt-1 text-[11px] font-medium uppercase tracking-wide text-faint">
          会话
        </p>
        <ul className="space-y-0.5">
          {sessions.length === 0 ? (
            <p className="px-1 py-2 text-xs text-faint">暂无会话</p>
          ) : (
            sessions.map((s) => {
              const active = chatActive && s.id === currentSessionId
              return (
                <li key={s.id}>
                  <button
                    type="button"
                    onClick={() => navigate(`/sessions/${s.id}`)}
                    disabled={disabled}
                    title={s.title}
                    className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors disabled:cursor-not-allowed ${
                      active
                        ? 'bg-primary/15 text-primary-light'
                        : 'text-muted hover:bg-surface-elevated hover:text-content'
                    }`}
                  >
                    <MessageSquare
                      size={15}
                      className={active ? 'text-primary-light' : 'text-faint'}
                      shrink-0
                    />
                    <span className="flex-1 truncate">{s.title}</span>
                    <span className="shrink-0 text-[11px] text-faint">
                      {formatTime(s.updated_at)}
                    </span>
                  </button>
                </li>
              )
            })
          )}
        </ul>

        {/* 项目区 */}
        <div className="mb-1 mt-3 flex items-center justify-between px-1">
          <p className="text-[11px] font-medium uppercase tracking-wide text-faint">
            项目
          </p>
          <button
            type="button"
            onClick={onCreateProject}
            title="新建项目"
            className="rounded p-0.5 text-faint hover:text-content"
          >
            <Plus size={14} />
          </button>
        </div>
        <ul className="space-y-0.5">
          {projects.length === 0 ? (
            <p className="px-1 py-2 text-xs text-faint">暂无项目</p>
          ) : (
            projects.map((p) => {
              const active = p.id === currentProjectId
              return (
                <li key={p.id}>
                  <button
                    type="button"
                    onClick={() => navigate(`/projects/${p.id}`)}
                    title={p.name}
                    className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${
                      active
                        ? 'bg-primary/15 text-primary-light'
                        : 'text-muted hover:bg-surface-elevated hover:text-content'
                    }`}
                  >
                    <span
                      className="h-3.5 w-3.5 shrink-0 rounded"
                      style={{ backgroundColor: p.cover_color }}
                    />
                    <span className="flex-1 truncate">{p.name}</span>
                    <Folder
                      size={13}
                      className={active ? 'text-primary-light' : 'text-faint'}
                      shrink-0
                    />
                  </button>
                </li>
              )
            })
          )}
        </ul>

        {/* 已归档分组（折叠） */}
        {archivedProjects.length > 0 && (
          <div className="mt-2">
            <button
              type="button"
              onClick={() => setArchivedOpen((v) => !v)}
              className="flex w-full items-center gap-1.5 rounded-lg px-2 py-1.5 text-left text-xs text-faint hover:text-muted"
            >
              {archivedOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
              <Archive size={12} />
              已归档（{archivedProjects.length}）
            </button>
            {archivedOpen && (
              <ul className="space-y-0.5 pt-0.5">
                {archivedProjects.map((p) => {
                  const active = p.id === currentProjectId
                  return (
                    <li key={p.id}>
                      <button
                        type="button"
                        onClick={() => navigate(`/projects/${p.id}`)}
                        title={p.name}
                        className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${
                          active
                            ? 'bg-primary/15 text-primary-light'
                            : 'text-faint hover:bg-surface-elevated hover:text-muted'
                        }`}
                      >
                        <span
                          className="h-3.5 w-3.5 shrink-0 rounded opacity-60"
                          style={{ backgroundColor: p.cover_color }}
                        />
                        <span className="flex-1 truncate">{p.name}</span>
                      </button>
                    </li>
                  )
                })}
              </ul>
            )}
          </div>
        )}
        {/* 管理入口（仅 admin） */}
        {user?.role === 'admin' && (
          <>
            <p className="px-1 pb-1 pt-4 text-[11px] font-medium uppercase tracking-wide text-faint">
              管理
            </p>
            <ul className="space-y-0.5">
              <li>
                <button
                  type="button"
                  onClick={() => navigate('/knowledge')}
                  className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${
                    location.pathname === '/knowledge'
                      ? 'bg-primary/15 text-primary-light'
                      : 'text-muted hover:bg-surface-elevated hover:text-content'
                  }`}
                >
                  <BookOpen
                    size={15}
                    className={
                      location.pathname === '/knowledge'
                        ? 'text-primary-light'
                        : 'text-faint'
                    }
                    shrink-0
                  />
                  知识库
                </button>
              </li>
              <li>
                <button
                  type="button"
                  onClick={() => navigate('/mcp')}
                  className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${
                    location.pathname === '/mcp'
                      ? 'bg-primary/15 text-primary-light'
                      : 'text-muted hover:bg-surface-elevated hover:text-content'
                  }`}
                >
                  <Plug
                    size={15}
                    className={
                      location.pathname === '/mcp'
                        ? 'text-primary-light'
                        : 'text-faint'
                    }
                    shrink-0
                  />
                  MCP 集成
                </button>
              </li>
            </ul>
          </>
        )}
      </div>
      <div className="border-t border-border px-3 py-2.5">
        <div className="flex items-center gap-2.5">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-primary/20 text-sm font-medium text-primary-light">
            {(user?.display_name || user?.username || '?').slice(0, 1)}
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm text-content">
              {user?.display_name || user?.username}
            </p>
            <p className="truncate text-[11px] text-faint">{user?.role}</p>
          </div>
          <button
            type="button"
            onClick={onLogout}
            title="退出登录"
            className="rounded-md p-1.5 text-faint transition-colors hover:bg-surface-elevated hover:text-error"
          >
            <LogOut size={16} />
          </button>
        </div>
      </div>
    </div>
  )
}
