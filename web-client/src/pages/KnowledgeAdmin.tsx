import { useCallback, useEffect, useRef, useState } from 'react'
import {
  UploadCloud,
  Trash2,
  FileText,
  Loader2,
  CheckCircle2,
  XCircle,
  Clock,
} from 'lucide-react'
import {
  deleteKnowledgeDocument,
  listKnowledgeDocuments,
  uploadKnowledgeDocument,
  KNOWLEDGE_ALLOWED_EXT,
  type KnowledgeDocument,
  type KnowledgeDocStatus,
} from '../api/client'

const STATUS_META: Record<
  KnowledgeDocStatus,
  { label: string; cls: string; icon: typeof Clock }
> = {
  pending: {
    label: '等待中',
    cls: 'bg-faint/15 text-faint',
    icon: Clock,
  },
  indexing: {
    label: '索引中',
    cls: 'bg-primary/15 text-primary-light',
    icon: Loader2,
  },
  ready: {
    label: '就绪',
    cls: 'bg-emerald-500/15 text-emerald-400',
    icon: CheckCircle2,
  },
  failed: {
    label: '失败',
    cls: 'bg-error/15 text-error',
    icon: XCircle,
  },
  deleted: {
    label: '已删除',
    cls: 'bg-faint/15 text-faint',
    icon: XCircle,
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

function extOf(name: string): string {
  return name.split('.').pop()?.toUpperCase() ?? '-'
}

export default function KnowledgeAdmin() {
  const [docs, setDocs] = useState<KnowledgeDocument[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [uploading, setUploading] = useState(false)
  const [dragOver, setDragOver] = useState(false)
  const [confirmId, setConfirmId] = useState<string | null>(null)
  const [deleting, setDeleting] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const refresh = useCallback(async () => {
    const { documents } = await listKnowledgeDocuments()
    setDocs(documents)
  }, [])

  useEffect(() => {
    let active = true
    listKnowledgeDocuments()
      .then(({ documents }) => {
        if (active) setDocs(documents)
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

  // 存在处理中的文档时轮询索引进度
  const hasProcessing = docs.some(
    (d) => d.status === 'pending' || d.status === 'indexing',
  )
  useEffect(() => {
    if (!hasProcessing) return
    const timer = setInterval(() => {
      void refresh().catch(() => undefined)
    }, 1500)
    return () => clearInterval(timer)
  }, [hasProcessing, refresh])

  const handleFiles = useCallback(
    async (files: FileList | File[]) => {
      const list = [...files]
      if (list.length === 0) return

      for (const file of list) {
        const ext = file.name.split('.').pop()?.toLowerCase() ?? ''
        if (!KNOWLEDGE_ALLOWED_EXT.includes(ext)) {
          setError(`不支持的文件类型：${file.name}（仅 PDF/DOCX/TXT/MD）`)
          return
        }
      }

      setError(null)
      setUploading(true)
      try {
        for (const file of list) {
          await uploadKnowledgeDocument(file)
        }
        await refresh()
      } catch (e) {
        setError(e instanceof Error ? e.message : '上传失败')
      } finally {
        setUploading(false)
        if (fileInputRef.current) fileInputRef.current.value = ''
      }
    },
    [refresh],
  )

  const handleDelete = useCallback(async () => {
    if (!confirmId) return
    setDeleting(true)
    try {
      await deleteKnowledgeDocument(confirmId)
      await refresh()
      setConfirmId(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : '删除失败')
    } finally {
      setDeleting(false)
    }
  }, [confirmId, refresh])

  const confirmDoc = docs.find((d) => d.id === confirmId)

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg">
      <div className="mx-auto w-full max-w-5xl px-6 py-8">
        <div className="mb-6">
          <h1 className="text-xl font-semibold text-content">知识库管理</h1>
          <p className="mt-1 text-sm text-faint">
            上传设计资料（PDF / DOCX / TXT / Markdown），系统自动切片并向量化，供 RAG 检索增强对话使用。
          </p>
        </div>

        {/* 上传区 */}
        <div
          onDragOver={(e) => {
            e.preventDefault()
            setDragOver(true)
          }}
          onDragLeave={() => setDragOver(false)}
          onDrop={(e) => {
            e.preventDefault()
            setDragOver(false)
            if (!uploading) void handleFiles(e.dataTransfer.files)
          }}
          onClick={() => fileInputRef.current?.click()}
          className={`mb-6 flex cursor-pointer flex-col items-center justify-center rounded-xl border-2 border-dashed px-6 py-10 transition-colors ${
            dragOver
              ? 'border-primary bg-primary/10'
              : 'border-border bg-surface hover:border-primary/50'
          }`}
        >
          {uploading ? (
            <Loader2 size={28} className="animate-spin text-primary-light" />
          ) : (
            <UploadCloud size={28} className="text-faint" />
          )}
          <p className="mt-3 text-sm font-medium text-content">
            {uploading ? '正在上传…' : '拖拽文件到此处，或点击选择文件'}
          </p>
          <p className="mt-1 text-xs text-faint">支持 PDF / DOCX / TXT / MD，可多选</p>
          <input
            ref={fileInputRef}
            type="file"
            multiple
            accept=".pdf,.docx,.txt,.md"
            className="hidden"
            onChange={(e) => {
              if (e.target.files) void handleFiles(e.target.files)
            }}
          />
        </div>

        {error && (
          <div className="mb-4 rounded-lg border border-error/30 bg-error/10 px-4 py-2.5 text-sm text-error">
            {error}
          </div>
        )}

        {/* 文档列表 */}
        <div className="overflow-hidden rounded-xl border border-border bg-surface">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border text-left text-xs text-faint">
                <th className="px-4 py-3 font-medium">文件名</th>
                <th className="px-4 py-3 font-medium">类型</th>
                <th className="px-4 py-3 font-medium">状态</th>
                <th className="px-4 py-3 font-medium">切片数</th>
                <th className="px-4 py-3 font-medium">上传时间</th>
                <th className="px-4 py-3 font-medium text-right">操作</th>
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr>
                  <td colSpan={6} className="px-4 py-10 text-center text-faint">
                    <Loader2 size={18} className="mx-auto animate-spin" />
                  </td>
                </tr>
              ) : docs.length === 0 ? (
                <tr>
                  <td colSpan={6} className="px-4 py-10 text-center text-faint">
                    暂无文档，请上传
                  </td>
                </tr>
              ) : (
                docs.map((d) => {
                  const meta = STATUS_META[d.status]
                  const Icon = meta.icon
                  return (
                    <tr
                      key={d.id}
                      className="border-b border-border/60 last:border-0 hover:bg-surface-elevated/50"
                    >
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-2">
                          <FileText size={15} className="shrink-0 text-faint" />
                          <div className="min-w-0">
                            <p className="truncate text-content" title={d.filename}>
                              {d.filename}
                            </p>
                            {d.status === 'failed' && d.error && (
                              <p className="truncate text-xs text-error" title={d.error}>
                                {d.error}
                              </p>
                            )}
                          </div>
                        </div>
                      </td>
                      <td className="px-4 py-3 text-muted">{extOf(d.filename)}</td>
                      <td className="px-4 py-3">
                        <span
                          className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs ${meta.cls}`}
                        >
                          <Icon
                            size={12}
                            className={d.status === 'indexing' ? 'animate-spin' : ''}
                          />
                          {meta.label}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-muted">{d.chunk_count || '-'}</td>
                      <td className="px-4 py-3 text-muted">
                        {formatTime(d.created_at)}
                      </td>
                      <td className="px-4 py-3 text-right">
                        <button
                          type="button"
                          onClick={() => setConfirmId(d.id)}
                          disabled={d.status === 'pending' || d.status === 'indexing'}
                          title="删除文档"
                          className="rounded-md p-1.5 text-faint transition-colors hover:bg-error/10 hover:text-error disabled:cursor-not-allowed disabled:opacity-40"
                        >
                          <Trash2 size={15} />
                        </button>
                      </td>
                    </tr>
                  )
                })
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* 删除二次确认 */}
      {confirmDoc && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div
            className="absolute inset-0 bg-black/60"
            onClick={() => !deleting && setConfirmId(null)}
          />
          <div className="relative w-full max-w-sm rounded-xl border border-border bg-surface p-5 shadow-xl">
            <h2 className="text-base font-semibold text-content">确认删除文档？</h2>
            <p className="mt-2 text-sm text-muted">
              将删除「<span className="text-content">{confirmDoc.filename}</span>
              」及其全部向量切片，此操作不可恢复。
            </p>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmId(null)}
                disabled={deleting}
                className="rounded-lg px-3.5 py-2 text-sm text-muted hover:bg-surface-elevated"
              >
                取消
              </button>
              <button
                type="button"
                onClick={handleDelete}
                disabled={deleting}
                className="flex items-center gap-1.5 rounded-lg bg-error px-3.5 py-2 text-sm font-medium text-white hover:bg-error/90 disabled:opacity-60"
              >
                {deleting && <Loader2 size={14} className="animate-spin" />}
                确认删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
