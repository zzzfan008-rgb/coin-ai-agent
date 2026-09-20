import { memo, type ReactElement } from 'react'
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

function TypingIndicator() {
  return (
    <span className="inline-flex items-center gap-1 py-1">
      <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
      <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
      <span className="typing-dot h-1.5 w-1.5 rounded-full bg-primary" />
    </span>
  )
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
            <TypingIndicator />
          ) : (
            <div
              className={`md-body ${isStreaming ? 'streaming-cursor' : ''}`}
            >
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
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
