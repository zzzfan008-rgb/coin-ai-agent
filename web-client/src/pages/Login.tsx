import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { Shirt, Eye, EyeOff } from 'lucide-react'
import { useAuth } from '../hooks/useAuth'

export default function Login() {
  const { login } = useAuth()
  const navigate = useNavigate()

  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (submitting) return
    setError(null)
    setSubmitting(true)
    try {
      await login({ username: username.trim(), password })
      navigate('/', { replace: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : '登录失败')
    } finally {
      setSubmitting(false)
    }
  }

  const fillDemo = () => {
    setUsername('designer')
    setPassword('designer123')
    setError(null)
  }

  const inputCls =
    'h-10 w-full rounded-lg border border-border bg-surface px-3.5 text-sm text-content placeholder:text-faint transition-colors focus:border-primary focus:outline-none'

  return (
    <div className="flex min-h-full items-center justify-center bg-bg px-4">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex flex-col items-center">
          <div className="mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-primary shadow-card">
            <Shirt size={22} className="text-white" />
          </div>
          <h1 className="text-2xl font-semibold text-content">
            Fashion AI Platform
          </h1>
          <p className="mt-1 text-sm text-muted">登录以进入智能设计工作台</p>
        </div>

        <form
          onSubmit={handleSubmit}
          className="rounded-card border border-border bg-surface p-6 shadow-card"
        >
          <div className="space-y-4">
            <div>
              <label className="mb-1.5 block text-sm text-muted">用户名</label>
              <input
                className={inputCls}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="请输入用户名"
                autoComplete="username"
                required
              />
            </div>
            <div>
              <label className="mb-1.5 block text-sm text-muted">密码</label>
              <div className="relative">
                <input
                  type={showPassword ? 'text' : 'password'}
                  className={`${inputCls} pr-10`}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="请输入密码"
                  autoComplete="current-password"
                  required
                />
                <button
                  type="button"
                  onClick={() => setShowPassword((v) => !v)}
                  className="absolute right-2.5 top-1/2 -translate-y-1/2 text-faint hover:text-muted"
                  title={showPassword ? '隐藏密码' : '显示密码'}
                >
                  {showPassword ? <EyeOff size={17} /> : <Eye size={17} />}
                </button>
              </div>
            </div>
          </div>

          {error && (
            <p className="mt-3 rounded-md bg-error/10 px-3 py-2 text-xs text-error">
              {error}
            </p>
          )}

          <button
            type="submit"
            disabled={submitting}
            className="mt-5 h-11 w-full rounded-lg bg-primary text-sm font-medium text-white transition-colors hover:bg-primary-hover disabled:cursor-not-allowed disabled:opacity-50"
          >
            {submitting ? '登录中…' : '登 录'}
          </button>

          <button
            type="button"
            onClick={fillDemo}
            className="mt-2 w-full rounded-lg border border-border px-3 py-2 text-xs text-muted transition-colors hover:border-primary hover:text-primary-light"
          >
            填充演示账号（designer / designer123）
          </button>

          <p className="mt-4 text-center text-sm text-muted">
            还没有账号？{' '}
            <Link to="/register" className="text-primary-light hover:underline">
              立即注册
            </Link>
          </p>
        </form>
      </div>
    </div>
  )
}
