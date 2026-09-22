import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MessageBubble } from '../components/MessageBubble'
import type { SessionMessage } from '../api/client'

// ── Helpers ─────────────────────────────────────────────────────────────────

function renderBubble(message: Partial<SessionMessage> = {}, isStreaming = false) {
  const msg: SessionMessage = {
    id: 'msg-1',
    role: 'assistant',
    content: '',
    created_at: '2024-01-01T00:00:00Z',
    ...message,
  }
  return render(<MessageBubble message={msg} isStreaming={isStreaming} />)
}

// ── Tests ───────────────────────────────────────────────────────────────────

describe('MessageBubble', () => {
  describe('user messages', () => {
    it('renders user message with content', () => {
      renderBubble({ role: 'user', content: 'Hello assistant' })
      expect(screen.getByText('Hello assistant')).toBeInTheDocument()
    })

    it('renders user avatar icon', () => {
      renderBubble({ role: 'user', content: 'Test' })
      // User icon is present (via lucide User icon)
      const icons = document.querySelectorAll('svg')
      expect(icons.length).toBeGreaterThan(0)
    })
  })

  describe('assistant messages', () => {
    it('renders assistant message with content', () => {
      renderBubble({ role: 'assistant', content: 'Here is my answer.' })
      expect(screen.getByText('Here is my answer.')).toBeInTheDocument()
    })

    it('renders assistant avatar icon', () => {
      renderBubble({ role: 'assistant', content: 'Hi' })
      const icons = document.querySelectorAll('svg')
      expect(icons.length).toBeGreaterThan(0)
    })
  })

  describe('error messages', () => {
    it('renders assistant message even when content is empty', () => {
      renderBubble({ role: 'assistant', content: '' })
      // Should not throw — renders the assistant wrapper
      const icons = document.querySelectorAll('svg')
      expect(icons.length).toBeGreaterThan(0)
    })
  })

  describe('streaming state', () => {
    it('shows streaming hint when content is empty and streaming', async () => {
      renderBubble({ role: 'assistant', content: '' }, true)

      // WAIT_HINTS[0] should be visible
      await screen.findByText(/Coin-AI 正在为您提供专业的服务/)
      expect(screen.getByText(/Coin-AI 正在为您提供专业的服务/)).toBeInTheDocument()
    })

    it('shows typing dots when streaming', () => {
      renderBubble({ role: 'assistant', content: '' }, true)

      const dots = document.querySelectorAll('.typing-dot')
      expect(dots.length).toBe(3)
    })

    it('renders content when streaming with existing content', () => {
      renderBubble({ role: 'assistant', content: 'Partial response...' }, true)
      expect(screen.getByText('Partial response...')).toBeInTheDocument()
    })
  })

  describe('markdown rendering', () => {
    it('renders bold text', () => {
      renderBubble({ role: 'assistant', content: 'This is **bold** text.' })
      // Text is split across <p> and <strong> — target the <p> element directly
      expect(screen.getByRole('paragraph')).toBeInTheDocument()
      expect(screen.getByText('bold')).toBeInTheDocument()
    })

    it('renders code blocks', () => {
      renderBubble({
        role: 'assistant',
        content: '```json\n{"key": "value"}\n```',
      })
      // Code block content is broken across Prism token spans — match by role
      expect(screen.getByRole('code')).toBeInTheDocument()
    })
  })
})
