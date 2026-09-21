import { X, Loader2, ImageOff } from 'lucide-react'
import type { SimilarImage } from '../api/client'

interface SimilarImagesPanelProps {
  /** Local object URL of the query image */
  queryUrl: string
  results: SimilarImage[]
  loading: boolean
  error: string | null
  onClose: () => void
}

/** 以图搜图结果面板：查询图 + 相似款式网格 */
export function SimilarImagesPanel({
  queryUrl,
  results,
  loading,
  error,
  onClose,
}: SimilarImagesPanelProps) {
  return (
    <div className="border-b border-border bg-surface/60 px-4 py-3">
      <div className="mx-auto max-w-3xl">
        <div className="mb-2.5 flex items-center gap-2">
          <img
            src={queryUrl}
            alt="查询图片"
            className="h-9 w-9 rounded-md border border-border object-cover"
          />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-content">以图搜图结果</p>
            <p className="truncate text-xs text-muted">
              {loading
                ? '正在 CLIP 编码并检索相似款式…'
                : `共 ${results.length} 个相似结果`}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            title="关闭"
            className="rounded-md p-1.5 text-muted hover:bg-surface hover:text-content"
          >
            <X size={16} />
          </button>
        </div>

        {loading && (
          <div className="flex items-center gap-2 py-6 text-sm text-muted">
            <Loader2 size={16} className="animate-spin" />
            检索中…
          </div>
        )}

        {!loading && error && (
          <p className="py-4 text-xs text-error">{error}</p>
        )}

        {!loading && !error && results.length === 0 && (
          <div className="flex items-center gap-2 py-4 text-sm text-muted">
            <ImageOff size={16} />
            未找到相似款式图片
          </div>
        )}

        {!loading && !error && results.length > 0 && (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
            {results.map((r, i) => (
              <div
                key={`${r.image_path}-${i}`}
                className="overflow-hidden rounded-lg border border-border bg-bg transition-colors hover:border-primary"
              >
                <div className="aspect-[3/4] w-full overflow-hidden bg-surface">
                  <img
                    src={r.image_path}
                    alt={r.style_name ?? '款式图'}
                    loading="lazy"
                    className="h-full w-full object-cover"
                  />
                </div>
                <div className="px-2 py-1.5">
                  <p className="truncate text-xs font-medium text-content">
                    {r.style_name ?? '未命名款式'}
                  </p>
                  <p className="text-[11px] text-primary-light">
                    相似度 {(r.similarity * 100).toFixed(1)}%
                  </p>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
