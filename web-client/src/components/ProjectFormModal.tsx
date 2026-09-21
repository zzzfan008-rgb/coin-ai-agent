import { useEffect, useState } from 'react'
import { X } from 'lucide-react'
import type { Project } from '../api/client'

interface ProjectFormModalProps {
  project?: Project | null
  onClose: () => void
  onSubmit: (input: {
    name: string
    description?: string
    cover_color?: string
  }) => Promise<void>
}

const PRESET_COLORS = [
  '#6366F1',
  '#10B981',
  '#F59E0B',
  '#EF4444',
  '#EC4899',
  '#8B5CF6',
  '#0EA5E9',
  '#64748B',
]

export function ProjectFormModal({
  project,
  onClose,
  onSubmit,
}: ProjectFormModalProps) {
  const [name, setName] = useState(project?.name ?? '')
  const [description, setDescription] = useState(project?.description ?? '')
  const [color, setColor] = useState(project?.cover_color ?? '#6366F1')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const handleSubmit = async () => {
    if (!name.trim()) {
      setError('请输入项目名称')
      return
    }
    setSaving(true)
    setError(null)
    try {
      await onSubmit({
        name: name.trim(),
        description: description.trim() || undefined,
        cover_color: color,
      })
      onClose()
    } catch (e) {
      setError(e instanceof Error ? e.message : '保存失败')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={() => !saving && onClose()}
      />
      <div className="relative z-10 w-full max-w-md rounded-xl border border-border bg-surface p-5 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold text-content">
            {project ? '编辑项目' : '新建项目'}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-faint hover:text-content"
          >
            <X size={18} />
          </button>
        </div>

        <div className="space-y-4">
          <div>
            <label className="mb-1.5 block text-xs font-medium text-muted">
              项目名称 <span className="text-error">*</span>
            </label>
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="如：2027 春夏系列"
              className="w-full rounded-lg border border-border bg-bg px-3 py-2 text-sm text-content outline-none focus:border-primary"
            />
          </div>

          <div>
            <label className="mb-1.5 block text-xs font-medium text-muted">
              项目描述
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              placeholder="可选，记录项目背景、目标等"
              className="w-full resize-none rounded-lg border border-border bg-bg px-3 py-2 text-sm text-content outline-none focus:border-primary"
            />
          </div>

          <div>
            <label className="mb-1.5 block text-xs font-medium text-muted">
              封面颜色
            </label>
            <div className="flex flex-wrap gap-2">
              {PRESET_COLORS.map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => setColor(c)}
                  className={`h-7 w-7 rounded-md transition-transform ${
                    color === c
                      ? 'ring-2 ring-content ring-offset-2 ring-offset-surface'
                      : 'hover:scale-110'
                  }`}
                  style={{ backgroundColor: c }}
                  title={c}
                />
              ))}
            </div>
          </div>

          {error && <p className="text-xs text-error">{error}</p>}
        </div>

        <div className="mt-5 flex justify-end gap-2">
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
            {saving ? '保存中…' : '确定'}
          </button>
        </div>
      </div>
    </div>
  )
}
