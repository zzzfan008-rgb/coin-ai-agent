import { memo, useEffect, useState, type ReactElement } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism'
import { User, Sparkles, Copy, Check } from 'lucide-react'
import type { SessionMessage } from '../api/client'

interface MessageBubbleProps {
  message: SessionMessage
  isStreaming?: boolean
}

// ── 等待中的两段式文案轮播 ────────────────────────────────────────────────

const WAIT_HINTS = [
  'Coin-AI 正在为您提供专业的服务…',
  '请稍等下哦，专业的建议值得您的期待 ✨',
]

function StreamingHint() {
  const [idx, setIdx] = useState(0)

  useEffect(() => {
    const t = setInterval(() => setIdx((i) => (i + 1) % WAIT_HINTS.length), 5200)
    return () => clearInterval(t)
  }, [])

  return (
    <div className="flex items-center gap-2.5 py-1">
      <span className="flex items-center gap-1">
        <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
        <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
        <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
      </span>
      <span key={idx} className="streaming-hint-text text-sm text-muted">
        {WAIT_HINTS[idx]}
      </span>
    </div>
  )
}

// ── 复制按钮 ────────────────────────────────────────────────────────────────
function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 2000)
    } catch {
      // clipboard not available
    }
  }

  return (
    <button
      type="button"
      onClick={handleCopy}
      title={copied ? '已复制' : '复制'}
      className={`rounded-md p-1.5 text-faint transition-all hover:bg-surface-elevated hover:text-primary ${
        copied ? 'text-success' : ''
      }`}
    >
      {copied ? <Check size={13} /> : <Copy size={13} />}
    </button>
  )
}

// ── 流式骨架屏（内容为空时显示） ───────────────────────────────────────
function StreamingSkeleton() {
  return (
    <div className="space-y-2 py-1">
      <div className="skeleton-shimmer h-3 w-3/4 rounded" />
      <div className="skeleton-shimmer h-3 w-full rounded" />
      <div className="skeleton-shimmer h-3 w-2/3 rounded" />
      <div className="flex items-center gap-2 py-1">
        <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
        <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
        <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
      </div>
    </div>
  )
}

// ── HEX 色号 → 色块（rehype 插件） ────────────────────────────────────────

const HEX_RE = /#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b/g
const SKIP_TAGS = new Set(['code', 'pre'])

/**
 * 遍历 hast 树，把文本节点中的 HEX 色值（#RGB / #RRGGBB）替换成
 * 「色块 + 色号」的内联元素。跳过 code/pre，避免破坏代码高亮。
 */
function rehypeColorSwatches() {
  return (tree: unknown) => {
    const visit = (node: any): void => {
      if (!node || typeof node !== 'object') return
      if (node.type === 'element' && SKIP_TAGS.has(node.tagName)) return

      if (node.type === 'text') {
        const text: string = node.value ?? ''
        HEX_RE.lastIndex = 0
        const matches: RegExpExecArray[] = []
        let m: RegExpExecArray | null
        while ((m = HEX_RE.exec(text)) !== null) matches.push(m)
        if (matches.length === 0) return

        const parts: any[] = []
        let last = 0
        for (const mm of matches) {
          if (mm.index > last) {
            parts.push({ type: 'text', value: text.slice(last, mm.index) })
          }
          const hex = mm[0]
          parts.push({
            type: 'element',
            tagName: 'span',
            properties: { className: ['inline-color'] },
            children: [
              {
                type: 'element',
                tagName: 'span',
                properties: {
                  className: ['color-chip'],
                  style: `background-color:${hex}`,
                },
                children: [],
              },
              { type: 'text', value: hex },
            ],
          })
          last = mm.index + hex.length
        }
        if (last < text.length) parts.push({ type: 'text', value: text.slice(last) })

        // 原地替换 text 节点为 element（父节点 children 数组持有引用）
        node.type = 'element'
        node.tagName = 'span'
        node.properties = {}
        node.children = parts
        return
      }

      if (Array.isArray(node.children)) {
        for (const child of node.children) visit(child)
      }
    }
    visit(tree)
  }
}

function MessageBubbleImpl({ message, isStreaming }: MessageBubbleProps) {
  const isUser = message.role === 'user'
  const hasContent = message.content.length > 0

  if (isUser) {
    return (
      <div className="flex justify-end msg-enter">
        <div className="flex max-w-[75%] items-start gap-2.5">
          {/* 用户操作按钮（仅在非空消息时显示） */}
          {hasContent && (
            <div className="hidden items-center gap-0.5 pt-1 sm:flex">
              <CopyButton text={message.content} />
            </div>
          )}
          {/* 用户消息气泡：渐变背景 */}
          <div className="group relative">
            <div className="rounded-2xl rounded-tr-md bg-gradient-to-br from-primary to-primary-dark px-4 py-3 text-white shadow-glow">
              <p className="whitespace-pre-wrap break-words leading-relaxed">
                {message.content}
              </p>
            </div>
          </div>
          {/* 用户头像：圆形渐变 */}
          <div className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-gradient-to-br from-primary to-fabric-purple text-white shadow-glow">
            <User size={16} />
          </div>
        </div>
      </div>
    )
  }

  // AI 消息
  return (
    <div className="flex justify-start msg-enter">
      <div className="flex max-w-[82%] items-start gap-2.5">
        {/* AI 头像：圆形带光晕 */}
        <div className="relative mt-0.5 shrink-0">
          <div className="flex h-9 w-9 items-center justify-center rounded-full bg-gradient-to-br from-fabric-purple/30 to-primary/30 text-primary-light ring-2 ring-primary/20 shadow-glow">
            <Sparkles size={16} />
          </div>
          {/* 在线状态点 */}
          {isStreaming && (
            <span className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-bg bg-primary">
              <span className="absolute inset-0 animate-ping rounded-full bg-primary opacity-60" />
            </span>
          )}
        </div>

        {/* 消息气泡容器 */}
        <div className="relative">
          {/* AI 气泡：边框渐变 + 背景纹理 */}
          <div
            className={`rounded-2xl rounded-tl-md border border-border/60 bg-surface shadow-card ${
              isStreaming ? 'ai-bubble-glow' : ''
            }`}
          >
            {/* 顶部装饰条 */}
            <div className="h-0.5 w-16 rounded-t-2xl bg-gradient-to-r from-primary to-fabric-purple opacity-60" />

            <div className="px-4 py-3">
              {hasContent ? (
                <>
                  {/* AI 消息操作栏 */}
                  <div className="mb-2 flex items-center justify-between border-b border-border/40 pb-2">
                    <span className="text-[11px] font-medium text-primary-light">
                      Coin-AI 助手
                    </span>
                    <CopyButton text={message.content} />
                  </div>

                  {/* 消息内容 */}
                  <div
                    className={`md-body ${isStreaming ? 'streaming-cursor' : ''}`}
                  >
                    <ReactMarkdown
                      remarkPlugins={[remarkGfm]}
                      rehypePlugins={[rehypeColorSwatches]}
                      components={{
                        // inline code 里的纯 HEX 色值 → 色块 + 色号
                        code(props) {
                          const { children, className } = props
                          const text = String(children ?? '').trim()
                          const hexMatch =
                            /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{3})$/.exec(text)
                          if (hexMatch && !className?.includes('language-')) {
                            const hex = hexMatch[0]
                            return (
                              <span className="inline-color">
                                <span
                                  className="color-chip"
                                  style={{ backgroundColor: hex }}
                                />
                                {hex}
                              </span>
                            )
                          }
                          return <code className={className}>{children}</code>
                        },
                        // react-markdown v9：在 pre 层接管代码块高亮
                        pre(props) {
                          const child = props.children as ReactElement<{
                            className?: string
                            children?: React.ReactNode
                          }>
                          const className = child?.props?.className ?? ''
                          const match = /language-(\w+)/.exec(className)
                          const code = String(child?.props?.children ?? '').replace(
                            /\n$/,
                            '',
                          )
                          return (
                            <SyntaxHighlighter
                              language={match?.[1] ?? 'text'}
                              style={oneDark}
                              customStyle={{
                                margin: '10px 0',
                                borderRadius: '8px',
                                fontSize: '12.5px',
                                background: '#0F172A',
                                border: '1px solid #334155',
                              }}
                              wrapLongLines
                            >
                              {code}
                            </SyntaxHighlighter>
                          )
                        },
                      }}
                    >
                      {message.content}
                    </ReactMarkdown>
                  </div>
                </>
              ) : isStreaming ? (
                <StreamingSkeleton />
              ) : (
                <StreamingHint />
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export const MessageBubble = memo(MessageBubbleImpl)
