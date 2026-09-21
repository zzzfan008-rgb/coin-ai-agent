import { useCallback, useEffect, useState } from 'react'
import {
  Plus,
  Trash2,
  Loader2,
  CheckCircle2,
  XCircle,
  HelpCircle,
  Plug,
} from 'lucide-react'
import {
  deleteMcpServer,
  listMcpServers,
  registerMcpServer,
  toggleMcpServer,
  type McpHealthStatus,
  type McpServer,
} from '../api/client'
import { useAuth } from '../hooks/useAuth'

const HEALTH_META: Record<
  McpHealthStatus,
  { label: string; cls: string; icon: typeof HelpCircle }
> = {
  healthy: {
    label: '健康',
    cls: 'bg-emerald-500/15 text-emerald-400',
    icon: CheckCircle2,
  },
  unhealthy: {
    label: '不可用',
    cls: 'bg-error/15 text-error',
    icon: XCircle,
  },
  unknown: {
    label: '未知',
    cls: 'bg-faint/15 text-faint',
    icon: HelpCircle,
  },
}

function formatTime(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return '-'
  return d.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

export default function McpAdmin() {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const [servers, setServers] = useState<McpServer[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [togglingId, setTogglingId] = useState<string | null>(null)
  const [deletingId, setDeletingId] = useState<string | null>(null)
  const [confirmId, setConfirmId] = useState<string | null>(null)

  // 添加表单
  const [showForm, setShowForm] = useState(false)
  const [formId, setFormId] = useState('')
  const [formName, setFormName] = useState('')
  const [formUrl, setFormUrl] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    listMcpServers()
      .then(({ servers: list }) => {
        if (active) setServers(list)
      })
      .catch((e: Error) => {
        if (active) setError(e.message)
      })
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => {
      active = false
    }
  }, [])

  const handleToggle = useCallback(
    async (server: McpServer) => {
      setTogglingId(server.id)
      setError(null)
      try {
        const updated = await toggleMcpServer(server.id, !server.enabled)
        setServers((prev) =>
          prev.map((s) => (s.id === updated.id ? updated : s)),
        )
      } catch (e) {
        setError(e instanceof Error ? e.message : '操作失败')
      } finally {
        setTogglingId(null)
      }
    },
    [],
  )

  const handleDelete = useCallback(async () => {
    if (!confirmId) return
    setDeletingId(confirmId)
    try {
      await deleteMcpServer(confirmId)
      setServers((prev) => prev.filter((s) => s.id !== confirmId))
      setConfirmId(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : '删除失败')
    } finally {
      setDeletingId(null)
    }
  }, [confirmId])

  const handleAdd = useCallback(async () => {
    setFormError(null)
    if (!formId.trim() || !formUrl.trim()) {
      setFormError('Server ID 与 Endpoint URL 不能为空')
      return
    }
    setSubmitting(true)
    try {
      const created = await registerMcpServer({
        id: formId.trim(),
        name: formName.trim() || undefined,
        endpoint_url: formUrl.trim(),
      })
      setServers((prev) => [...prev, created])
      setShowForm(false)
      setFormId('')
      setFormName('')
      setFormUrl('')
    } catch (e) {
      setFormError(e instanceof Error ? e.message : '注册失败')
    } finally {
      setSubmitting(false)
    }
  }, [formId, formName, formUrl])

  const confirmServer = servers.find((s) => s.id === confirmId)

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg">
      <div className="mx-auto w-full max-w-5xl px-6 py-8">
        <div className="mb-6 flex items-start justify-between">
          <div>
            <h1 className="text-xl font-semibold text-content">MCP 集成管理</h1>
            <p className="mt-1 text-sm text-faint">
              {isAdmin
                ? '注册并管理 Model Context Protocol Server，对话时可调用外部工具（ERP / PLM 等）。'
                : '在此启用或停用已授权的 MCP 集成，启用后对话可调用其工具。'}
            </p>
          </div>
          {isAdmin && !showForm && (
            <button
              type="button"
              onClick={() => setShowForm(true)}
              className="flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-2 text-sm font-medium text-white hover:bg-primary-hover"
            >
              <Plus size={15} />
              添加 Server
            </button>
          )}
        </div>

        {/* 添加表单（仅管理员） */}
        {isAdmin && showForm && (
          <div className="mb-6 rounded-xl border border-border bg-surface p-4">
            <div className="grid gap-3 sm:grid-cols-3">
              <div>
                <label className="mb-1 block text-xs text-faint">Server ID *</label>
                <input
                  value={formId}
                  onChange={(e) => setFormId(e.target.value)}
                  placeholder="如 fabric-erp"
                  className="w-full rounded-lg border border-border bg-bg px-3 py-2 text-sm text-content outline-none focus:border-primary"
                />
              </div>
              <div>
                <label className="mb-1 block text-xs text-faint">显示名称</label>
                <input
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  placeholder="如 面料 ERP"
                  className="w-full rounded-lg border border-border bg-bg px-3 py-2 text-sm text-content outline-none focus:border-primary"
                />
              </div>
              <div>
                <label className="mb-1 block text-xs text-faint">Endpoint URL *</label>
                <input
                  value={formUrl}
                  onChange={(e) => setFormUrl(e.target.value)}
                  placeholder="http://host:9101/mcp"
                  className="w-full rounded-lg border border-border bg-bg px-3 py-2 text-sm text-content outline-none focus:border-primary"
                />
              </div>
            </div>
            {formError && <p className="mt-2 text-xs text-error">{formError}</p>}
            <div className="mt-3 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => {
                  setShowForm(false)
                  setFormError(null)
                }}
                disabled={submitting}
                className="rounded-lg px-3.5 py-2 text-sm text-muted hover:bg-surface-elevated"
              >
                取消
              </button>
              <button
                type="button"
                onClick={handleAdd}
                disabled={submitting}
                className="flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-2 text-sm font-medium text-white hover:bg-primary-hover disabled:opacity-60"
              >
                {submitting && <Loader2 size={14} className="animate-spin" />}
                注册
              </button>
            </div>
          </div>
        )}

        {error && (
          <div className="mb-4 rounded-lg border border-error/30 bg-error/10 px-4 py-2.5 text-sm text-error">
            {error}
          </div>
        )}

        {/* Server 列表 */}
        <div className="overflow-hidden rounded-xl border border-border bg-surface">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border text-left text-xs text-faint">
                <th className="px-4 py-3 font-medium">Server</th>
                <th className="px-4 py-3 font-medium">Endpoint</th>
                <th className="px-4 py-3 font-medium">健康状态</th>
                <th className="px-4 py-3 font-medium">工具数</th>
                <th className="px-4 py-3 font-medium">添加时间</th>
                <th className="px-4 py-3 font-medium">启用</th>
                {isAdmin && <th className="px-4 py-3 font-medium text-right">操作</th>}
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr>
                  <td colSpan={7} className="px-4 py-10 text-center text-faint">
                    <Loader2 size={18} className="mx-auto animate-spin" />
                  </td>
                </tr>
              ) : servers.length === 0 ? (
                <tr>
                  <td colSpan={7} className="px-4 py-10 text-center text-faint">
                    {isAdmin ? '暂无已注册的 MCP Server' : '暂无已授权的 MCP 集成'}
                  </td>
                </tr>
              ) : (
                servers.map((s) => {
                  const meta = HEALTH_META[s.health_status]
                  const Icon = meta.icon
                  return (
                    <tr
                      key={s.id}
                      className="border-b border-border/60 last:border-0 hover:bg-surface-elevated/50"
                    >
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-2">
                          <Plug size={15} className="shrink-0 text-faint" />
                          <div>
                            <p className="text-content">{s.name}</p>
                            <p className="text-xs text-faint">{s.id}</p>
                          </div>
                        </div>
                      </td>
                      <td className="max-w-[220px] px-4 py-3">
                        <span className="truncate text-muted" title={s.endpoint_url}>
                          {s.endpoint_url}
                        </span>
                      </td>
                      <td className="px-4 py-3">
                        <span
                          className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs ${meta.cls}`}
                        >
                          <Icon size={12} />
                          {meta.label}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-muted">{s.tool_count}</td>
                      <td className="px-4 py-3 text-muted">
                        {formatTime(s.created_at)}
                      </td>
                      <td className="px-4 py-3">
                        <button
                          type="button"
                          role="switch"
                          aria-checked={s.enabled}
                          onClick={() => handleToggle(s)}
                          disabled={togglingId === s.id}
                          className={`relative inline-flex h-5.5 w-10 items-center rounded-full transition-colors disabled:opacity-60 ${
                            s.enabled ? 'bg-primary' : 'bg-border'
                          }`}
                          style={{ height: 22, width: 40 }}
                          title={s.enabled ? '点击停用' : '点击启用'}
                        >
                          {togglingId === s.id ? (
                            <Loader2
                              size={12}
                              className="absolute left-1 animate-spin text-white"
                            />
                          ) : (
                            <span
                              className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                                s.enabled ? 'translate-x-[18px]' : 'translate-x-1'
                              }`}
                            />
                          )}
                        </button>
                      </td>
                      {isAdmin && (
                        <td className="px-4 py-3 text-right">
                          <button
                            type="button"
                            onClick={() => setConfirmId(s.id)}
                            title="删除 Server"
                            className="rounded-md p-1.5 text-faint transition-colors hover:bg-error/10 hover:text-error"
                          >
                            <Trash2 size={15} />
                          </button>
                        </td>
                      )}
                    </tr>
                  )
                })
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* 删除二次确认 */}
      {confirmServer && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div
            className="absolute inset-0 bg-black/60"
            onClick={() => !deletingId && setConfirmId(null)}
          />
          <div className="relative w-full max-w-sm rounded-xl border border-border bg-surface p-5 shadow-xl">
            <h2 className="text-base font-semibold text-content">确认删除 MCP Server？</h2>
            <p className="mt-2 text-sm text-muted">
              将从本组织移除「<span className="text-content">{confirmServer.name}</span>
              」，所有用户将无法再调用其工具。
            </p>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmId(null)}
                disabled={Boolean(deletingId)}
                className="rounded-lg px-3.5 py-2 text-sm text-muted hover:bg-surface-elevated"
              >
                取消
              </button>
              <button
                type="button"
                onClick={handleDelete}
                disabled={Boolean(deletingId)}
                className="flex items-center gap-1.5 rounded-lg bg-error px-3.5 py-2 text-sm font-medium text-white hover:bg-error/90 disabled:opacity-60"
              >
                {deletingId && <Loader2 size={14} className="animate-spin" />}
                确认删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
