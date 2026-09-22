import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { BrowserRouter } from 'react-router-dom'
import Login from '../pages/Login'

// ── Mock ─────────────────────────────────────────────────────────────────────

const mockLogin = vi.fn()
const mockNavigate = vi.fn()

vi.mock('../hooks/useAuth', () => ({
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

describe('Login', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders username and password inputs', () => {
    renderLogin()
    expect(screen.getByPlaceholderText('请输入用户名')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('请输入密码')).toBeInTheDocument()
  })

  it('renders submit button', () => {
    renderLogin()
    expect(screen.getByRole('button', { name: '登 录' })).toBeInTheDocument()
  })

  it('shows error message when login fails', async () => {
    const user = userEvent.setup()
    mockLogin.mockRejectedValue(new Error('用户名或密码错误'))

    renderLogin()

    await user.type(screen.getByPlaceholderText('请输入用户名'), 'baduser')
    await user.type(screen.getByPlaceholderText('请输入密码'), 'wrongpass')
    await user.click(screen.getByRole('button', { name: '登 录' }))

    // Wait for error to appear
    await screen.findByText((_, el) =>
      el?.textContent === '用户名或密码错误',
    )
  })

  it('calls login and navigates to home on success', async () => {
    const user = userEvent.setup()
    mockLogin.mockResolvedValue(undefined)

    renderLogin()

    await user.type(screen.getByPlaceholderText('请输入用户名'), 'designer')
    await user.type(screen.getByPlaceholderText('请输入密码'), 'designer123')
    await user.click(screen.getByRole('button', { name: '登 录' }))

    expect(mockLogin).toHaveBeenCalledWith({
      username: 'designer',
      password: 'designer123',
    })
    expect(mockNavigate).toHaveBeenCalledWith('/', { replace: true })
  })

  it('fillDemo button populates fields with demo credentials', async () => {
    const user = userEvent.setup()
    renderLogin()

    await user.click(screen.getByRole('button', { name: /填充演示账号/ }))

    expect(
      (screen.getByPlaceholderText('请输入用户名') as HTMLInputElement).value,
    ).toBe('designer')
    expect(
      (screen.getByPlaceholderText('请输入密码') as HTMLInputElement).value,
    ).toBe('designer123')
  })

  it('password field toggles visibility', async () => {
    const user = userEvent.setup()
    renderLogin()

    const passwordInput = screen.getByPlaceholderText(
      '请输入密码',
    ) as HTMLInputElement

    // Initially hidden (password type)
    expect(passwordInput.type).toBe('password')

    const toggleBtn = screen.getByTitle('显示密码')
    await user.click(toggleBtn)
    expect(passwordInput.type).toBe('text')

    const hideBtn = screen.getByTitle('隐藏密码')
    await user.click(hideBtn)
    expect(passwordInput.type).toBe('password')
  })
})
