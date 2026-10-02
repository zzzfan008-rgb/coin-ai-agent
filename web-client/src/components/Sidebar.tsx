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
  RotateCcw,
  Sparkles,
  Trash2,
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
  archivedSessions?: Session[]
  onArchiveSession?: (id: string) => Promise<void> | void
  onDeleteSession?: (id: string) => Promise<void> | void
  onRestoreSession?: (id: string) => Promise<void> | void
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
  archivedSessions = [],
  onArchiveSession,
  onDeleteSession,
  onRestoreSession,
}: SidebarProps) {
  const navigate = useNavigate()
  const location = useLocation()
  const [archivedOpen, setArchivedOpen] = useState(false)

  const chatActive = location.pathname === '/' || location.pathname.startsWith('/sessions/')

  return (
    <div className="flex h-full w-64 flex-col border-r border-border/60 bg-surface/95 backdrop-blur-sm">
      {/* 品牌区 */}
      <div className="relative px-4 py-4">
        {/* 顶部渐变光晕 */}
        <div className="pointer-events-none absolute inset-x-0 top-0 h-20 rounded-br-full rounded-bl-full bg-gradient-to-b from-primary/10 to-transparent opacity-60" />
        <div className="relative flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            {/* Logo 容器：渐变背景 + 光晕 */}
            <div className="relative">
              <div className="logo-glow flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-primary to-fabric-purple shadow-glow">
                <Shirt size={18} className="text-white" />
              </div>
              {/* AI 状态点 */}
              <span className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-surface bg-success">
                <span className="absolute inset-0 animate-ping rounded-full bg-success opacity-60" />
              </span>
            </div>
            <div className="flex flex-col">
              <span className="text-[15px] font-bold text-content">Coin-AI</span>
              <span className="text-[10px] text-faint">智能设计工作台</span>
            </div>
          </div>
          {onClose && (
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg p-1.5 text-faint transition-colors hover:bg-surface-elevated hover:text-content lg:hidden"
              title="收起侧边栏"
            >
              <X size={16} />
            </button>
          )}
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3">
        {/* 新建会话：渐变按钮 */}
        <button
          type="button"
          onClick={onNewSession}
          disabled={disabled}
          className="btn-glow mb-3 flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-primary to-fabric-purple px-3 py-2.5 text-sm font-semibold text-white shadow-glow transition-all hover:from-primary-dark hover:to-fabric-purple"
        >
          <Sparkles size={15} className="text-white/90" />
          新建会话
        </button>

        {/* 会话区 */}
        <p className="mb-1.5 flex items-center gap-1 px-1 pt-1 text-[11px] font-semibold uppercase tracking-wider text-faint">
          <MessageSquare size={11} />
          会话
        </p>
        <ul className="space-y-0.5">
          {sessions.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border/50 bg-surface-elevated/30 py-6 text-center">
              <MessageSquare size={20} className="mx-auto mb-1.5 text-faint/40" />
              <p className="text-xs text-faint/60">暂无会话记录</p>
            </div>
          ) : (
            sessions.map((s) => {
              const active = chatActive && s.id === currentSessionId
              return (
                <li key={s.id} className="group relative">
                  <button
                    type="button"
                    onClick={() => navigate(`/sessions/${s.id}`)}
                    disabled={disabled}
                    title={s.title}
                    className={`sidebar-item-hover group flex w-full items-center gap-2 rounded-xl px-3 py-2.5 text-left text-sm transition-all disabled:cursor-not-allowed ${
                      active
                        ? 'bg-primary/15 text-primary-light shadow-inner-glow'
                        : 'text-muted hover:text-content'
                    }`}
                  >
                    <div
                      className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg transition-colors ${
                        active
                          ? 'bg-primary/20 text-primary-light'
                          : 'bg-surface-elevated text-faint group-hover:bg-border'
                      }`}
                    >
                      <MessageSquare size={13} />
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="truncate font-medium">{s.title}</p>
                      <p className="truncate text-[10px] text-faint">{formatTime(s.updated_at)}</p>
                    </div>
                    {active && (
                      <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
                    )}
                  </button>
                  <span className="absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-0.5 rounded-lg bg-surface/95 p-0.5 opacity-0 shadow-sm transition-opacity group-hover:opacity-100">
                    {onArchiveSession && (
                      <button
                        type="button"
                        title="归档"
                        onClick={(e) => {
                          e.stopPropagation()
                          void onArchiveSession(s.id)
                        }}
                        className="rounded-md p-1 text-faint transition-colors hover:bg-surface-elevated hover:text-primary"
                      >
                        <Archive size={12} />
                      </button>
                    )}
                    {onDeleteSession && (
                      <button
                        type="button"
                        title="删除"
                        onClick={(e) => {
                          e.stopPropagation()
                          if (window.confirm(`确定删除会话「${s.title || '未命名会话'}」？此操作不可恢复。`)) {
                            void onDeleteSession(s.id)
                          }
                        }}
                        className="rounded-md p-1 text-faint transition-colors hover:bg-error/10 hover:text-error"
                      >
                        <Trash2 size={12} />
                      </button>
                    )}
                  </span>
                </li>
              )
            })
          )}
        </ul>

        {/* 项目区 */}
        <div className="mb-1 mt-4 flex items-center justify-between px-1">
          <p className="flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wider text-faint">
            <Folder size={11} />
            项目
          </p>
          <button
            type="button"
            onClick={onCreateProject}
            title="新建项目"
            className="rounded-md p-1 text-faint transition-colors hover:bg-surface-elevated hover:text-primary"
          >
            <Plus size={13} />
          </button>
        </div>
        <ul className="space-y-0.5">
          {projects.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border/50 bg-surface-elevated/30 py-6 text-center">
              <Folder size={20} className="mx-auto mb-1.5 text-faint/40" />
              <p className="text-xs text-faint/60">暂无项目</p>
            </div>
          ) : (
            projects.map((p) => {
              const active = p.id === currentProjectId
              return (
                <li key={p.id}>
                  <button
                    type="button"
                    onClick={() => navigate(`/projects/${p.id}`)}
                    title={p.name}
                    className={`sidebar-item-hover group flex w-full items-center gap-2 rounded-xl px-3 py-2.5 text-left text-sm transition-all ${
                      active
                        ? 'bg-primary/15 text-primary-light shadow-inner-glow'
                        : 'text-muted hover:text-content'
                    }`}
                  >
                    <span
                      className={`h-7 w-7 shrink-0 rounded-lg shadow-sm ${
                        active ? 'ring-2 ring-primary/40' : ''
                      }`}
                      style={{ backgroundColor: p.cover_color }}
                    />
                    <span className="min-w-0 flex-1 truncate font-medium">{p.name}</span>
                    <Folder
                      size={12}
                      className={active ? 'text-primary-light' : 'text-faint opacity-0 group-hover:opacity-100 transition-opacity'}
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
              className="flex w-full items-center gap-1.5 rounded-xl px-2.5 py-2 text-left text-xs text-faint transition-colors hover:bg-surface-elevated hover:text-muted"
            >
              {archivedOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              <Archive size={11} />
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
                        className={`sidebar-item-hover group flex w-full items-center gap-2 rounded-xl px-3 py-2 text-left text-sm transition-all ${
                          active
                            ? 'bg-primary/15 text-primary-light'
                            : 'text-faint hover:bg-surface-elevated hover:text-muted'
                        }`}
                      >
                        <span
                          className="h-6 w-6 shrink-0 rounded opacity-50"
                          style={{ backgroundColor: p.cover_color }}
                        />
                        <span className="flex-1 truncate text-xs">{p.name}</span>
                      </button>
                    </li>
                  )
                })}
              </ul>
            )}
          </div>
        )}

        {/* 已归档会话（折叠） */}
        {archivedSessions.length > 0 && (
          <div className="mt-2">
            <button
              type="button"
              onClick={() => setArchivedOpen((v) => !v)}
              className="flex w-full items-center gap-1.5 rounded-xl px-2.5 py-2 text-left text-xs text-faint transition-colors hover:bg-surface-elevated hover:text-muted"
            >
              {archivedOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              <MessageSquare size={11} />
              已归档会话（{archivedSessions.length}）
            </button>
            {archivedOpen && (
              <ul className="space-y-0.5 pt-0.5">
                {archivedSessions.map((s) => (
                  <li key={s.id} className="group relative">
                    <button
                      type="button"
                      onClick={() => navigate(`/sessions/${s.id}`)}
                      title={s.title}
                      className="flex w-full items-center gap-2 rounded-xl px-3 py-2 text-left text-sm text-faint transition-colors hover:bg-surface-elevated hover:text-muted"
                    >
                      <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-surface-elevated opacity-50">
                        <MessageSquare size={11} />
                      </div>
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-xs">{s.title}</p>
                        <p className="truncate text-[10px] text-faint">{formatTime(s.updated_at)}</p>
                      </div>
                    </button>
                    <span className="absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-0.5 rounded-lg bg-surface/95 p-0.5 opacity-0 shadow-sm transition-opacity group-hover:opacity-100">
                      {onRestoreSession && (
                        <button
                          type="button"
                          title="恢复"
                          onClick={(e) => {
                            e.stopPropagation()
                            void onRestoreSession(s.id)
                          }}
                          className="rounded-md p-1 text-faint transition-colors hover:bg-surface-elevated hover:text-primary"
                        >
                          <RotateCcw size={12} />
                        </button>
                      )}
                      {onDeleteSession && (
                        <button
                          type="button"
                          title="删除"
                          onClick={(e) => {
                            e.stopPropagation()
                            if (window.confirm(`确定删除会话「${s.title || '未命名会话'}」？此操作不可恢复。`)) {
                              void onDeleteSession(s.id)
                            }
                          }}
                          className="rounded-md p-1 text-faint transition-colors hover:bg-error/10 hover:text-error"
                        >
                          <Trash2 size={12} />
                        </button>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {/* 管理入口（仅 admin） */}
        {user?.role === 'admin' && (
          <>
            <p className="mb-1 mt-4 flex items-center gap-1 px-1 text-[11px] font-semibold uppercase tracking-wider text-faint">
              <Plug size={11} />
              管理
            </p>
            <ul className="space-y-0.5">
              <li>
                <button
                  type="button"
                  onClick={() => navigate('/knowledge')}
                  className={`sidebar-item-hover group flex w-full items-center gap-2 rounded-xl px-3 py-2.5 text-left text-sm transition-all ${
                    location.pathname === '/knowledge'
                      ? 'bg-primary/15 text-primary-light shadow-inner-glow'
                      : 'text-muted hover:text-content'
                  }`}
                >
                  <div
                    className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg transition-colors ${
                      location.pathname === '/knowledge'
                        ? 'bg-primary/20 text-primary-light'
                        : 'bg-surface-elevated text-faint group-hover:bg-border'
                    }`}
                  >
                    <BookOpen size={13} />
                  </div>
                  <span className="font-medium">知识库</span>
                </button>
              </li>
              <li>
                <button
                  type="button"
                  onClick={() => navigate('/mcp')}
                  className={`sidebar-item-hover group flex w-full items-center gap-2 rounded-xl px-3 py-2.5 text-left text-sm transition-all ${
                    location.pathname === '/mcp'
                      ? 'bg-primary/15 text-primary-light shadow-inner-glow'
                      : 'text-muted hover:text-content'
                  }`}
                >
                  <div
                    className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg transition-colors ${
                      location.pathname === '/mcp'
                        ? 'bg-primary/20 text-primary-light'
                        : 'bg-surface-elevated text-faint group-hover:bg-border'
                    }`
                    }
                  >
                    <Plug size={13} />
                  </div>
                  <span className="font-medium">MCP 集成</span>
                </button>
              </li>
            </ul>
          </>
        )}
      </div>

      {/* 用户信息区 */}
      <div className="border-t border-border/60 px-3 py-3">
        <div className="flex items-center gap-2.5 rounded-xl bg-surface-elevated/50 p-2">
          {/* 头像：渐变圆形 */}
          <div className="relative flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-gradient-to-br from-primary to-fabric-purple text-sm font-bold text-white shadow-glow">
            {(user?.display_name || user?.username || '?').slice(0, 1)}
            {/* 角色徽章 */}
            {user?.role === 'admin' && (
              <span className="absolute -top-0.5 -right-0.5 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-warning text-[8px] font-bold text-black">
                ★
              </span>
            )}
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold text-content">
              {user?.display_name || user?.username}
            </p>
            <p className="truncate text-[10px] capitalize text-faint">
              {user?.role === 'admin' ? '管理员' : '设计师'}
            </p>
          </div>
          <button
            type="button"
            onClick={onLogout}
            title="退出登录"
            className="rounded-lg p-1.5 text-faint transition-all hover:bg-error/10 hover:text-error"
          >
            <LogOut size={15} />
          </button>
        </div>
      </div>
    </div>
  )
}
