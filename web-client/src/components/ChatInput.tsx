import { useEffect, useRef, useState } from 'react'
import { Send, Square, ImagePlus, X, Sparkles } from 'lucide-react'

interface ChatInputProps {
  onSend: (text: string, images: File[]) => void
  onStop: () => void
  isStreaming: boolean
  disabled?: boolean
  initialValue?: string
  onChange?: (value: string) => void
}

export function ChatInput({
  onSend,
  onStop,
  isStreaming,
  disabled,
  initialValue = '',
  onChange,
}: ChatInputProps) {
  const [value, setValue] = useState(initialValue)
  const [images, setImages] = useState<File[]>([])
  const [dragging, setDragging] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileRef = useRef<HTMLInputElement>(null)

  // 自适应高度
  useEffect(() => {
    const el = textareaRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`
  }, [value])

  const addFiles = (files: FileList | File[]) => {
    const imgs = [...files].filter((f) => f.type.startsWith('image/'))
    if (imgs.length > 0) setImages((prev) => [...prev, ...imgs].slice(0, 4))
  }

  const removeImage = (idx: number) => {
    setImages((prev) => prev.filter((_, i) => i !== idx))
  }

  const submit = () => {
    const text = value.trim()
    if ((!text && images.length === 0) || isStreaming || disabled) return
    onSend(text, images)
    setValue('')
    setImages([])
    if (fileRef.current) fileRef.current.value = ''
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Enter 发送，Shift+Enter 换行
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault()
      submit()
    }
  }

  const canSend = (!!value.trim() || images.length > 0) && !disabled

  return (
    <div className="border-t border-border bg-bg/80 px-4 py-3 backdrop-blur-sm">
      <div className="mx-auto max-w-3xl">
        {/* 图片预览 */}
        {images.length > 0 && (
          <div className="mb-2.5 flex flex-wrap gap-2">
            {images.map((img, i) => {
              const url = URL.createObjectURL(img)
              return (
                <div
                  key={`${img.name}-${i}`}
                  className="group relative h-16 w-16 overflow-hidden rounded-xl border border-border/60 shadow-card transition-transform hover:scale-105"
                >
                  <img
                    src={url}
                    alt={img.name}
                    className="h-full w-full object-cover"
                    onLoad={() => URL.revokeObjectURL(url)}
                  />
                  <button
                    type="button"
                    onClick={() => removeImage(i)}
                    title="移除"
                    className="absolute right-0.5 top-0.5 rounded-full bg-black/70 p-0.5 text-white opacity-0 transition-opacity group-hover:opacity-100"
                  >
                    <X size={11} />
                  </button>
                </div>
              )
            })}
          </div>
        )}

        {/* 输入区：玻璃态容器 */}
        <div
          className={`relative overflow-hidden rounded-2xl border transition-all duration-200 ${
            dragging
              ? 'border-primary bg-primary/8 shadow-glow'
              : 'border-border/70 bg-surface/80 backdrop-blur-md hover:border-primary/50 focus-within:border-primary focus-within:shadow-glow'
          }`}
          onDragOver={(e) => {
            e.preventDefault()
            setDragging(true)
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={(e) => {
            e.preventDefault()
            setDragging(false)
            if (!disabled && !isStreaming) addFiles(e.dataTransfer.files)
          }}
        >
          {/* 左侧装饰条（流式时发光） */}
          <div
            className={`absolute left-0 top-0 h-full w-0.5 rounded-l-2xl transition-all duration-300 ${
              isStreaming
                ? 'bg-gradient-to-b from-primary via-fabric-purple to-primary animate-pulse-slow'
                : 'bg-gradient-to-b from-primary/40 to-transparent'
            }`}
          />

          <div className="flex items-end gap-1.5 pl-3 pr-2 py-2">
            {/* 图片上传按钮 */}
            <button
              type="button"
              onClick={() => fileRef.current?.click()}
              disabled={disabled || isStreaming}
              title="上传图片（发消息给 AI 生图 / 文字含「搜图」时以图搜图）"
              className="mb-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-muted transition-all hover:bg-surface-elevated hover:text-primary disabled:opacity-40"
            >
              <ImagePlus size={18} />
            </button>
            <input
              ref={fileRef}
              type="file"
              accept="image/*"
              multiple
              hidden
              onChange={(e) => {
                if (e.target.files) addFiles(e.target.files)
              }}
            />

            <textarea
              ref={textareaRef}
              rows={1}
              value={value}
              onChange={(e) => {
                setValue(e.target.value)
                onChange?.(e.target.value)
              }}
              onKeyDown={handleKeyDown}
              disabled={disabled}
              placeholder={
                disabled
                  ? '正在加载会话…'
                  : '输入消息，可拖拽或点击上传图片（AI 生图 / 输入「搜图」找相似款）'
              }
              className="block max-h-40 w-full resize-none bg-transparent py-2 pl-0 pr-1 text-sm text-content placeholder:text-faint/70 focus:outline-none disabled:opacity-50"
            />

            {/* 发送 / 停止按钮 */}
            {isStreaming ? (
              <button
                type="button"
                onClick={onStop}
                title="停止生成"
                className="mb-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-error/40 bg-error/10 text-error shadow-card transition-all hover:bg-error/20 hover:shadow-glow"
              >
                <Square size={15} fill="currentColor" />
              </button>
            ) : (
              <button
                type="button"
                onClick={submit}
                disabled={!canSend}
                title="发送"
                className={`mb-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-xl font-medium text-white shadow-card transition-all ${
                  canSend
                    ? 'btn-glow bg-gradient-to-br from-primary to-primary-dark hover:from-primary-dark hover:to-primary-dark'
                    : 'bg-surface-elevated text-faint cursor-not-allowed'
                }`}
              >
                {canSend ? (
                  <Sparkles size={16} className="text-white/90" />
                ) : (
                  <Send size={16} />
                )}
              </button>
            )}
          </div>

          {/* 底部提示文字 */}
          {canSend && (
            <div className="flex items-center justify-between px-4 pb-2 text-[11px] text-faint/60">
              <span>Enter 发送 · Shift+Enter 换行</span>
              <span>{value.trim().length} 字</span>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
