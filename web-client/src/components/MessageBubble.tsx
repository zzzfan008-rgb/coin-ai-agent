import { memo, useEffect, useState, type ReactElement } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism'
import { User, Sparkles } from 'lucide-react'
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
    const t = setInterval(() => setIdx((i) => (i + 1) % WAIT_HINTS.length), 2600)
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

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="flex max-w-[80%] items-start gap-3">
          <div className="rounded-card rounded-tr-sm bg-primary px-4 py-2.5 text-white shadow-card">
            <p className="whitespace-pre-wrap break-words">{message.content}</p>
          </div>
          <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-primary-hover text-white">
            <User size={16} />
          </div>
        </div>
      </div>
    )
  }

  const empty = message.content.length === 0

  return (
    <div className="flex justify-start">
      <div className="flex max-w-[85%] items-start gap-3">
        <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-surface-elevated text-primary-light ring-1 ring-border">
          <Sparkles size={16} />
        </div>
        <div className="rounded-card rounded-tl-sm border border-border bg-surface px-4 py-2.5 shadow-card">
          {empty && isStreaming ? (
            <StreamingHint />
          ) : (
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
                    const hexMatch = /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{3})$/.exec(
                      text,
                    )
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
          )}
        </div>
      </div>
    </div>
  )
}

export const MessageBubble = memo(MessageBubbleImpl)
