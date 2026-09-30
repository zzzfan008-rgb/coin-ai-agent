/**
 * LoginForm 组件测试
 *
 * 测试点：
 * 1. 渲染：用户名、密码输入框，登录按钮
 * 2. 表单提交：调用 onLogin 并传正确参数
 * 3. 错误提示：登录失败时显示错误信息
 * 4. 演示账号填充
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { BrowserRouter } from 'react-router-dom'
import Login from '../../src/pages/Login'

// ── Mock ─────────────────────────────────────────────────────────────────────

const mockLogin = vi.fn()
const mockNavigate = vi.fn()

vi.mock('../../src/hooks/useAuth', () => ({
  useAuth: () => ({ login: mockLogin }),
}))

vi.mock('react-router-dom', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-router-dom')>()
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  }
})

// ── Helpers ─────────────────────────────────────────────────────────────────

function renderLogin() {
  return render(
    <BrowserRouter>
      <Login />
    </BrowserRouter>,
  )
}

// ── Tests ───────────────────────────────────────────────────────────────────

describe('LoginForm 渲染', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockLogin.mockResolvedValue(undefined)
  })

  it('渲染用户名和密码输入框', () => {
    renderLogin()
    expect(screen.getByPlaceholderText('输入用户名')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('输入密码')).toBeInTheDocument()
  })

  it('渲染登录按钮', () => {
    renderLogin()
    expect(screen.getByRole('button', { name: '登 录' })).toBeInTheDocument()
  })

  it('初始状态下登录按钮可点击（不自动禁用）', () => {
    renderLogin()
    expect(screen.getByRole('button', { name: '登 录' })).not.toBeDisabled()
  })
})

describe('LoginForm 提交逻辑', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockLogin.mockResolvedValue(undefined)
  })

  it('填写并提交表单，调用 login 和 navigate', async () => {
    const user = userEvent.setup()
    renderLogin()

    await user.type(screen.getByPlaceholderText('输入用户名'), 'alice')
    await user.type(screen.getByPlaceholderText('输入密码'), 'secret123')
    await user.click(screen.getByRole('button', { name: '登 录' }))

    expect(mockLogin).toHaveBeenCalledWith({
      username: 'alice',
      password: 'secret123',
    })
    expect(mockNavigate).toHaveBeenCalledWith('/', { replace: true })
  })

  it('输入为空时 login 不被调用（浏览器原生验证或表单约束）', async () => {
    const user = userEvent.setup()
    renderLogin()

    await user.click(screen.getByRole('button', { name: '登 录' }))

    expect(mockLogin).not.toHaveBeenCalled()
  })

  it('登录失败时显示错误信息', async () => {
    const user = userEvent.setup()
    mockLogin.mockRejectedValue(new Error('用户名或密码错误'))

    renderLogin()

    await user.type(screen.getByPlaceholderText('输入用户名'), 'baduser')
    await user.type(screen.getByPlaceholderText('输入密码'), 'badpass')
    await user.click(screen.getByRole('button', { name: '登 录' }))

    await screen.findByText((_, el) => el?.textContent === '用户名或密码错误')
  })
})

describe('LoginForm 演示账号填充', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockLogin.mockResolvedValue(undefined)
  })

  it('点击填充演示账号按钮，字段自动填充 designer/designer123', async () => {
    const user = userEvent.setup()
    renderLogin()

    await user.click(screen.getByRole('button', { name: /填充演示账号/ }))

    const usernameInput = screen.getByPlaceholderText(
      '输入用户名',
    ) as HTMLInputElement
    const passwordInput = screen.getByPlaceholderText(
      '输入密码',
    ) as HTMLInputElement

    expect(usernameInput.value).toBe('designer')
    expect(passwordInput.value).toBe('designer123')
  })
})

describe('LoginForm 密码可见性切换', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockLogin.mockResolvedValue(undefined)
  })

  it('默认密码字段类型为 password', () => {
    renderLogin()
    const pw = screen.getByPlaceholderText('输入密码') as HTMLInputElement
    expect(pw.type).toBe('password')
  })

  it('点击显示密码切换为 text 类型', async () => {
    const user = userEvent.setup()
    renderLogin()

    await user.click(screen.getByTitle('显示密码'))

    const pw = screen.getByPlaceholderText('输入密码') as HTMLInputElement
    expect(pw.type).toBe('text')
  })

  it('再次点击切换回 password 类型', async () => {
    const user = userEvent.setup()
    renderLogin()

    await user.click(screen.getByTitle('显示密码'))
    await user.click(screen.getByTitle('隐藏密码'))

    const pw = screen.getByPlaceholderText('输入密码') as HTMLInputElement
    expect(pw.type).toBe('password')
  })
})
