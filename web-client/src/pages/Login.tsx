import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { Shirt, Eye, EyeOff, Sparkles } from 'lucide-react'
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

  return (
    <div className="brand-gradient flex min-h-full items-center justify-center px-4 py-8">
      {/* 背景装饰：浮动光球 */}
      <div className="pointer-events-none fixed inset-0 overflow-hidden">
        <div className="absolute left-1/4 top-1/4 h-64 w-64 rounded-full bg-primary/10 blur-3xl" />
        <div className="absolute bottom-1/4 right-1/4 h-80 w-80 rounded-full bg-fabric-purple/8 blur-3xl" />
        <div className="absolute right-1/3 top-1/2 h-48 w-48 rounded-full bg-info/6 blur-3xl" />
      </div>

      <div className="relative w-full max-w-sm">
        {/* Logo 区 */}
        <div className="mb-8 flex flex-col items-center">
          <div className="logo-glow mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-primary to-fabric-purple shadow-glow-lg">
            <Shirt size={26} className="text-white" />
          </div>
          <h1 className="text-2xl font-bold text-content">
            Coin-AI
          </h1>
          <p className="mt-1.5 text-sm text-muted">智能服装设计工作台</p>
          <div className="mt-2 flex items-center gap-1.5 text-[11px] text-faint">
            <span className="h-px w-6 bg-border" />
            <span>欢迎回来</span>
            <span className="h-px w-6 bg-border" />
          </div>
        </div>

        {/* 登录卡片：玻璃态 + 渐变边框 */}
        <div className="auth-card glass rounded-card-lg border border-border/50 p-6 shadow-glow">
          <form onSubmit={handleSubmit}>
            <div className="space-y-4">
              {/* 用户名 */}
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-sm font-medium text-muted">
                  <span className="h-1 w-1 rounded-full bg-primary" />
                  用户名
                </label>
                <input
                  className="h-11 w-full rounded-xl border border-border/70 bg-surface-elevated/50 px-4 text-sm text-content placeholder:text-faint/60 transition-all focus:border-primary focus:bg-surface-elevated focus:outline-none focus:ring-2 focus:ring-primary/20"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder="输入用户名"
                  autoComplete="username"
                  required
                />
              </div>

              {/* 密码 */}
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-sm font-medium text-muted">
                  <span className="h-1 w-1 rounded-full bg-primary" />
                  密码
                </label>
                <div className="relative">
                  <input
                    type={showPassword ? 'text' : 'password'}
                    className="h-11 w-full rounded-xl border border-border/70 bg-surface-elevated/50 px-4 pr-11 text-sm text-content placeholder:text-faint/60 transition-all focus:border-primary focus:bg-surface-elevated focus:outline-none focus:ring-2 focus:ring-primary/20"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="输入密码"
                    autoComplete="current-password"
                    required
                  />
                  <button
                    type="button"
                    onClick={() => setShowPassword((v) => !v)}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-faint transition-colors hover:text-muted"
                    title={showPassword ? '隐藏密码' : '显示密码'}
                  >
                    {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                </div>
              </div>
            </div>

            {error && (
              <div className="mt-4 rounded-xl border border-error/30 bg-error/10 px-4 py-3 text-sm text-error">
                {error}
              </div>
            )}

            {/* 登录按钮 */}
            <button
              type="submit"
              disabled={submitting}
              className="btn-glow mt-6 flex h-11 w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-primary to-fabric-purple text-sm font-semibold text-white shadow-glow transition-all"
            >
              {submitting ? (
                <>
                  <span className="h-4 w-4 animate-spin rounded-full border-2 border-white/30 border-t-white" />
                  登录中…
                </>
              ) : (
                <>
                  <Sparkles size={15} className="text-white/90" />
                  登 录
                </>
              )}
            </button>

            {/* 演示账号 */}
            <button
              type="button"
              onClick={fillDemo}
              className="mt-2.5 w-full rounded-xl border border-border/50 px-4 py-2.5 text-xs text-muted transition-all hover:border-primary/40 hover:text-primary-light"
            >
              填充演示账号（designer / designer123）
            </button>
          </form>

          <div className="mt-5 flex items-center gap-3">
            <span className="h-px flex-1 bg-border/50" />
            <span className="text-xs text-faint">还没有账号？</span>
            <span className="h-px flex-1 bg-border/50" />
          </div>

          <Link
            to="/register"
            className="btn-glow mt-2.5 flex h-11 w-full items-center justify-center gap-2 rounded-xl border border-primary/30 bg-primary/10 text-sm font-medium text-primary-light transition-all hover:bg-primary/20"
          >
            立即注册
          </Link>
        </div>

        {/* 底部 */}
        <p className="mt-6 text-center text-xs text-faint/60">
          登录即表示您同意我们的服务条款和隐私政策
        </p>
      </div>
    </div>
  )
}
