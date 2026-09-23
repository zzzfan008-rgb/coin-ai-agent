import { useEffect, useRef, useState } from 'react'
import {
  Menu,
  PanelRight,
  AlertCircle,
  FolderPlus,
  RefreshCw,
  WifiOff,
} from 'lucide-react'
import { useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { useChat } from '../hooks/useChat'
import { useLayout } from '../hooks/useLayout'
import { SkillPanel } from '../components/SkillPanel'
import { MessageBubble } from '../components/MessageBubble'
import { ChatInput } from '../components/ChatInput'
import { AddToProjectsModal } from '../components/AddToProjectsModal'
import { SimilarImagesPanel } from '../components/SimilarImagesPanel'
import {
  searchSimilarImages,
  getStoredUser,
  type SimilarImage,
} from '../api/client'
import { SKILL_CATALOG } from '../api/client'

export default function Chat() {
  const params = useParams()
  const [searchParams, setSearchParams] = useSearchParams()
  const navigate = useNavigate()
  const { openSidebar, setCurrentSessionId } = useLayout()

  const chat = useChat(params.id)
  const [skillPanelOpen, setSkillPanelOpen] = useState(false)
  const [addToProjectOpen, setAddToProjectOpen] = useState(false)

  // 以图搜图面板状态
  const [similarPanel, setSimilarPanel] = useState<{
    queryUrl: string
    results: SimilarImage[]
    loading: boolean
    error: string | null
  } | null>(null)

  const scrollRef = useRef<HTMLDivElement>(null)

  // 同步当前会话到 Layout（用于侧边栏高亮）
  useEffect(() => {
    setCurrentSessionId(chat.currentSession?.id)
  }, [chat.currentSession?.id, setCurrentSessionId])

  // ?new=1：创建新会话
  const wantNew = searchParams.get('new') === '1'
  useEffect(() => {
    if (!wantNew || !chat.loaded || chat.isStreaming) return
    void (async () => {
      const s = await chat.newSession()
      if (s) navigate(`/sessions/${s.id}`, { replace: true })
    })()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wantNew, chat.loaded, chat.isStreaming])

  // 清理 new 参数（非新建路径时）
  useEffect(() => {
    if (searchParams.has('new') && !wantNew) {
      setSearchParams({}, { replace: true })
    }
  }, [searchParams, wantNew, setSearchParams])

  // 自动滚动
  const lastContent = chat.messages[chat.messages.length - 1]?.content
  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
  }, [lastContent, chat.messages.length])

  const error = chat.error

  const handleSend = (text: string, images: File[]) => {
    if (images.length > 0) {
      // 以图搜图（以第一张图为查询；附带文字也作为普通消息发送）
      const file = images[0]
      const queryUrl = URL.createObjectURL(file)
      const user = getStoredUser()

      setSimilarPanel({ queryUrl, results: [], loading: true, error: null })

      if (!user) {
        setSimilarPanel({
          queryUrl,
          results: [],
          loading: false,
          error: '未登录，无法检索',
        })
        return
      }

      void (async () => {
        try {
          const results = await searchSimilarImages(file)
          setSimilarPanel({ queryUrl, results, loading: false, error: null })
        } catch (e) {
          setSimilarPanel({
            queryUrl,
            results: [],
            loading: false,
            error: e instanceof Error ? e.message : '以图搜图失败',
          })
        }
      })()

      if (text) void chat.sendMessage(text)
    } else {
      void chat.sendMessage(text)
    }
  }

  return (
    <>
      <div className="flex min-w-0 flex-1 flex-col">
        {/* 顶栏 */}
        <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
        <button
          type="button"
          onClick={openSidebar}
          className="rounded-md p-1.5 text-muted hover:bg-surface hover:text-content lg:hidden"
          title="侧边栏"
        >
          <Menu size={18} />
        </button>
        <h1 className="min-w-0 flex-1 truncate text-sm font-medium text-content">
          {chat.currentSession?.title || '加载中…'}
        </h1>
        {/* T-022: SSE connection status indicator */}
        {chat.isStreaming && (
          <span className="flex items-center gap-1.5 text-xs text-primary">
            <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-primary" />
            生成中
          </span>
        )}
        {chat.selectedSkillIds.length > 0 && (
          <span className="hidden rounded-md bg-primary/15 px-2 py-0.5 text-[11px] text-primary-light sm:block">
            {chat.selectedSkillIds.length} 个 Skill 已启用
          </span>
        )}
        <button
          type="button"
          onClick={() => setAddToProjectOpen(true)}
          disabled={!chat.currentSession}
          title="添加到项目"
          className="rounded-md p-1.5 text-muted hover:bg-surface hover:text-content disabled:opacity-40"
        >
          <FolderPlus size={18} />
        </button>
        <button
          type="button"
          onClick={() => setSkillPanelOpen((v) => !v)}
          className={`rounded-md p-1.5 transition-colors xl:hidden ${
            skillPanelOpen
              ? 'bg-primary/20 text-primary-light'
              : 'text-muted hover:bg-surface hover:text-content'
          }`}
          title="Skills"
        >
          <PanelRight size={18} />
        </button>
      </header>

      {error && (
        <div className="flex shrink-0 items-center gap-2 border-b border-error/30 bg-error/10 px-4 py-2 text-xs text-error">
          <AlertCircle size={14} />
          {error}
        </div>
      )}

      {/* T-022: stream-disconnect banner — friendly prompt, no tech detail */}
      {chat.streamDisconnected && (
        <div className="flex shrink-0 items-center gap-3 border-b border-warning/40 bg-warning/10 px-4 py-3 text-sm">
          <WifiOff size={16} className="shrink-0 text-warning" />
          <span className="flex-1 text-content">连接中断，请重试</span>
          <button
            type="button"
            onClick={chat.retryStream}
            disabled={chat.isStreaming}
            className="flex items-center gap-1.5 rounded-md bg-warning px-3 py-1 text-xs font-medium text-black transition-colors hover:bg-warning/80 disabled:opacity-50"
          >
            <RefreshCw size={12} className={chat.isStreaming ? 'animate-spin' : ''} />
            {chat.isStreaming ? '重连中…' : '重试'}
          </button>
        </div>
      )}
      {similarPanel && (
        <SimilarImagesPanel
          queryUrl={similarPanel.queryUrl}
          results={similarPanel.results}
          loading={similarPanel.loading}
          error={similarPanel.error}
          onClose={() => {
            URL.revokeObjectURL(similarPanel.queryUrl)
            setSimilarPanel(null)
          }}
        />
      )}

      {/* 消息区 */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-4 py-5">
        <div className="mx-auto max-w-3xl space-y-5">
          {chat.messages.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center pt-24 text-center">
              <p className="text-base font-medium text-content">开始一个新会话</p>
              <p className="mt-2 max-w-xs text-sm text-muted">
                在下方输入框描述你的设计问题，或先在右侧 Skill 面板选择专业技能。
              </p>
            </div>
          ) : (
            chat.messages.map((m) => (
              <MessageBubble
                key={m.id}
                message={m}
                isStreaming={
                  chat.isStreaming &&
                  m.id === chat.messages[chat.messages.length - 1]?.id &&
                  m.role === 'assistant'
                }
              />
            ))
          )}
        </div>
      </div>

      <ChatInput
        onSend={handleSend}
        onStop={chat.stopStreaming}
        isStreaming={chat.isStreaming}
        disabled={!chat.loaded || !chat.currentSession}
      />
      </div>

      {/* 桌面端 Skill 面板 */}
      <div className="hidden shrink-0 xl:block">
        <SkillPanel
          skills={SKILL_CATALOG}
          selectedIds={chat.selectedSkillIds}
          onToggle={chat.toggleSkill}
        />
      </div>

      {/* xl 以下 Skill 抽屉 —— 需要脱离 main 的 flex 列布局，使用 fixed */}
      {skillPanelOpen && (
        <div className="fixed inset-0 z-40 xl:hidden">
          <div
            className="absolute inset-0 bg-black/60"
            onClick={() => setSkillPanelOpen(false)}
          />
          <div className="absolute right-0 top-0 h-full">
            <SkillPanel
              skills={SKILL_CATALOG}
              selectedIds={chat.selectedSkillIds}
              onToggle={chat.toggleSkill}
              onClose={() => setSkillPanelOpen(false)}
            />
          </div>
        </div>
      )}

      {addToProjectOpen && chat.currentSession && (
        <AddToProjectsModal
          sessionId={chat.currentSession.id}
          onClose={() => setAddToProjectOpen(false)}
        />
      )}
    </>
  )
}
