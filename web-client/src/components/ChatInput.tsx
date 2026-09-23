import { useEffect, useRef, useState } from 'react'
import { Send, Square, ImagePlus, X } from 'lucide-react'

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
    <div className="border-t border-border bg-bg px-4 py-3">
      <div className="mx-auto max-w-3xl">
        {/* 图片预览 */}
        {images.length > 0 && (
          <div className="mb-2 flex flex-wrap gap-2">
            {images.map((img, i) => {
              const url = URL.createObjectURL(img)
              return (
                <div
                  key={`${img.name}-${i}`}
                  className="group relative h-16 w-16 overflow-hidden rounded-md border border-border"
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
                    className="absolute right-0.5 top-0.5 rounded-full bg-black/60 p-0.5 text-white opacity-0 transition-opacity group-hover:opacity-100"
                  >
                    <X size={11} />
                  </button>
                </div>
              )
            })}
          </div>
        )}

        <div
          className={`flex items-end gap-2 rounded-lg border bg-surface transition-colors focus-within:border-primary ${
            dragging ? 'border-primary bg-primary/5' : 'border-border'
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
          {/* 图片上传按钮 */}
          <button
            type="button"
            onClick={() => fileRef.current?.click()}
            disabled={disabled || isStreaming}
            title="上传图片以图搜图"
            className="ml-1.5 mb-2 flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-muted transition-colors hover:bg-bg hover:text-content disabled:opacity-50"
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
                : '输入消息 / 拖拽或点击上传款式图片以图搜图'
            }
            className="block max-h-40 w-full resize-none bg-transparent py-2.5 pl-0 pr-1 text-sm text-content placeholder:text-faint focus:outline-none disabled:opacity-50"
          />
          {isStreaming ? (
            <button
              type="button"
              onClick={onStop}
              title="停止生成"
              className="mb-0.5 mr-1 flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border border-border bg-surface text-muted transition-colors hover:text-content"
            >
              <Square size={16} fill="currentColor" />
            </button>
          ) : (
            <button
              type="button"
              onClick={submit}
              disabled={!canSend}
              title="发送"
              className="mb-0.5 mr-1 flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary text-white transition-colors hover:bg-primary-hover disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Send size={17} />
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
