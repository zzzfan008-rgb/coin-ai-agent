import { Plus, MessageSquare, LogOut, X, Shirt } from 'lucide-react'
import type { Session, AuthUser } from '../api/client'

interface SidebarProps {
  sessions: Session[]
  currentSessionId?: string
  user: AuthUser | null
  onSelect: (session: Session) => void
  onNew: () => void
  onLogout: () => void
  onClose?: () => void
  disabled?: boolean
}

function formatTime(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  const now = new Date()
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
    })
  }
  return d.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' })
}

export function Sidebar({
  sessions,
  currentSessionId,
  user,
  onSelect,
  onNew,
  onLogout,
  onClose,
  disabled,
}: SidebarProps) {
  return (
    <div className="flex h-full w-64 flex-col border-r border-border bg-surface">
      {/* 品牌 + 关闭 */}
      <div className="flex items-center justify-between px-4 py-3.5">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 items-center justify-center rounded-md bg-primary">
            <Shirt size={16} className="text-white" />
          </div>
          <span className="text-[15px] font-semibold text-content">
            Fashion AI
          </span>
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

      {/* 新建会话 */}
      <div className="px-3 pb-2">
        <button
          type="button"
          onClick={onNew}
          disabled={disabled}
          className="flex w-full items-center justify-center gap-1.5 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-primary-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Plus size={16} />
          新建会话
        </button>
      </div>

      {/* 会话列表 */}
      <div className="flex-1 overflow-y-auto px-3 py-1">
        {sessions.length === 0 ? (
          <p className="px-2 py-4 text-center text-xs text-faint">
            暂无会话，点击上方按钮开始
          </p>
        ) : (
          <ul className="space-y-0.5">
            {sessions.map((s) => {
              const active = s.id === currentSessionId
              return (
                <li key={s.id}>
                  <button
                    type="button"
                    onClick={() => onSelect(s)}
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
            })}
          </ul>
        )}
      </div>

      {/* 用户区 */}
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
