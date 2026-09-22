import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ChatInput } from '../components/ChatInput'

// ── Helpers ─────────────────────────────────────────────────────────────────

function renderChatInput(props: Partial<React.ComponentProps<typeof ChatInput>> = {}) {
  const defaults = {
    onSend: vi.fn(),
    onStop: vi.fn(),
    isStreaming: false,
    disabled: false,
    ...props,
  }
  return render(<ChatInput {...defaults} />)
}

// ── Tests ───────────────────────────────────────────────────────────────────

describe('ChatInput', () => {
  it('renders textarea with placeholder', () => {
    renderChatInput()
    expect(
      screen.getByPlaceholderText(/输入消息/),
    ).toBeInTheDocument()
  })

  it('send button is disabled when input is empty', () => {
    renderChatInput()
    const sendBtn = screen.getByRole('button', { name: '发送' })
    expect(sendBtn).toBeDisabled()
  })

  it('send button is enabled when textarea has content', async () => {
    const user = userEvent.setup()
    renderChatInput()

    await user.type(screen.getByRole('textbox'), 'Hello')
    const sendBtn = screen.getByRole('button', { name: '发送' })
    expect(sendBtn).not.toBeDisabled()
  })

  it('calls onSend with text when submit button is clicked', async () => {
    const user = userEvent.setup()
    const onSend = vi.fn()
    renderChatInput({ onSend })

    await user.type(screen.getByRole('textbox'), 'Hello world')
    await user.click(screen.getByRole('button', { name: '发送' }))

    expect(onSend).toHaveBeenCalledWith('Hello world', [])
  })

  it('calls onSend with text when Enter is pressed', async () => {
    const user = userEvent.setup()
    const onSend = vi.fn()
    renderChatInput({ onSend })

    const textarea = screen.getByRole('textbox')
    await user.type(textarea, 'Hello{Enter}')

    // userEvent.type does NOT fire keyDown for Enter the same way real input does;
    // use keyboard instead
    await user.keyboard('Test')
    await user.keyboard('{Enter}')

    expect(onSend).toHaveBeenCalledWith('Test', [])
  })

  it('Shift+Enter inserts newline instead of submitting', async () => {
    const user = userEvent.setup()
    const onSend = vi.fn()
    renderChatInput({ onSend })

    const textarea = screen.getByRole('textbox')
    await user.click(textarea)
    await user.type(textarea, 'Line1{Shift>}{Enter}{/Shift}Line2')

    // onSend should NOT have been called (Enter was Shift+Enter)
    expect(onSend).not.toHaveBeenCalled()
    expect(textarea).toHaveValue('Line1\nLine2')
  })

  it('shows stop button when isStreaming is true', () => {
    renderChatInput({ isStreaming: true })
    expect(screen.getByRole('button', { name: '停止生成' })).toBeInTheDocument()
  })

  it('send button is disabled when disabled prop is true', async () => {
    const user = userEvent.setup()
    renderChatInput({ disabled: true })

    await user.type(screen.getByRole('textbox'), 'Hello')
    expect(screen.getByRole('button', { name: '发送' })).toBeDisabled()
  })

  it('clears textarea after sending', async () => {
    const user = userEvent.setup()
    const onSend = vi.fn()
    renderChatInput({ onSend })

    const textarea = screen.getByRole('textbox')
    await user.type(textarea, 'Hello')
    await user.click(screen.getByRole('button', { name: '发送' }))

    expect(textarea).toHaveValue('')
  })
})
