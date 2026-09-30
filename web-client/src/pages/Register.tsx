import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { Shirt, Sparkles } from 'lucide-react'
import { useAuth } from '../hooks/useAuth'

export default function Register() {
  const { register } = useAuth()
  const navigate = useNavigate()

  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [orgName, setOrgName] = useState('我的工作室')
  const [deptName, setDeptName] = useState('设计部')
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (submitting) return
    setError(null)

    if (username.trim().length < 3) {
      setError('用户名至少 3 个字符')
      return
    }
    if (password.length < 6) {
      setError('密码至少 6 位')
      return
    }

    setSubmitting(true)
    try {
      await register({
        username: username.trim(),
        email: email.trim() || undefined,
        password,
        org_name: orgName.trim() || '我的工作室',
        dept_name: deptName.trim() || '设计部',
        display_name: username.trim(),
      })
      navigate('/', { replace: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : '注册失败')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="brand-gradient flex min-h-full items-center justify-center px-4 py-8">
      {/* 背景装饰 */}
      <div className="pointer-events-none fixed inset-0 overflow-hidden">
        <div className="absolute left-1/4 top-1/3 h-64 w-64 rounded-full bg-primary/10 blur-3xl" />
        <div className="absolute bottom-1/3 right-1/4 h-80 w-80 rounded-full bg-fabric-purple/8 blur-3xl" />
        <div className="absolute right-1/3 top-1/4 h-48 w-48 rounded-full bg-info/6 blur-3xl" />
      </div>

      <div className="relative w-full max-w-sm">
        {/* Logo 区 */}
        <div className="mb-8 flex flex-col items-center">
          <div className="logo-glow mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-primary to-fabric-purple shadow-glow-lg">
            <Shirt size={26} className="text-white" />
          </div>
          <h1 className="text-2xl font-bold text-content">创建账号</h1>
          <p className="mt-1.5 text-sm text-muted">开启你的智能设计之旅</p>
          <div className="mt-2 flex items-center gap-1.5 text-[11px] text-faint">
            <span className="h-px w-6 bg-border" />
            <span>免费注册</span>
            <span className="h-px w-6 bg-border" />
          </div>
        </div>

        {/* 注册卡片 */}
        <div className="auth-card glass rounded-card-lg border border-border/50 p-6 shadow-glow">
          <form onSubmit={handleSubmit}>
            <div className="space-y-3.5">
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
                  placeholder="至少 3 个字符"
                  autoComplete="username"
                  required
                />
              </div>

              {/* 邮箱 */}
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-sm font-medium text-muted">
                  <span className="h-1 w-1 rounded-full bg-faint" />
                  邮箱<span className="ml-1 text-faint">（选填）</span>
                </label>
                <input
                  type="email"
                  className="h-11 w-full rounded-xl border border-border/70 bg-surface-elevated/50 px-4 text-sm text-content placeholder:text-faint/60 transition-all focus:border-primary focus:bg-surface-elevated focus:outline-none focus:ring-2 focus:ring-primary/20"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="you@example.com"
                  autoComplete="email"
                />
              </div>

              {/* 密码 */}
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-sm font-medium text-muted">
                  <span className="h-1 w-1 rounded-full bg-warning" />
                  密码
                </label>
                <input
                  type="password"
                  className="h-11 w-full rounded-xl border border-border/70 bg-surface-elevated/50 px-4 text-sm text-content placeholder:text-faint/60 transition-all focus:border-primary focus:bg-surface-elevated focus:outline-none focus:ring-2 focus:ring-primary/20"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="至少 6 位"
                  autoComplete="new-password"
                  required
                />
              </div>

              {/* 组织信息 */}
              <div className="rounded-xl border border-border/40 bg-surface-elevated/30 p-3.5">
                <p className="mb-2.5 flex items-center gap-1.5 text-xs text-faint">
                  <span className="h-0.5 w-4 rounded-full bg-gradient-to-r from-primary to-fabric-purple" />
                  组织信息（首次注册将自动创建）
                </p>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="mb-1 block text-xs text-faint">工作室</label>
                    <input
                      className="h-9 w-full rounded-lg border border-border/50 bg-surface px-3 text-xs text-content placeholder:text-faint/60 transition-all focus:border-primary focus:outline-none"
                      value={orgName}
                      onChange={(e) => setOrgName(e.target.value)}
                      placeholder="工作室名称"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs text-faint">部门</label>
                    <input
                      className="h-9 w-full rounded-lg border border-border/50 bg-surface px-3 text-xs text-content placeholder:text-faint/60 transition-all focus:border-primary focus:outline-none"
                      value={deptName}
                      onChange={(e) => setDeptName(e.target.value)}
                      placeholder="部门名称"
                    />
                  </div>
                </div>
              </div>
            </div>

            {error && (
              <div className="mt-4 rounded-xl border border-error/30 bg-error/10 px-4 py-3 text-sm text-error">
                {error}
              </div>
            )}

            {/* 注册按钮 */}
            <button
              type="submit"
              disabled={submitting}
              className="btn-glow mt-5 flex h-11 w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-primary to-fabric-purple text-sm font-semibold text-white shadow-glow transition-all"
            >
              {submitting ? (
                <>
                  <span className="h-4 w-4 animate-spin rounded-full border-2 border-white/30 border-t-white" />
                  注册中…
                </>
              ) : (
                <>
                  <Sparkles size={15} className="text-white/90" />
                  注 册
                </>
              )}
            </button>

            {/* 返回登录 */}
            <Link
              to="/login"
              className="mt-3 flex h-10 w-full items-center justify-center rounded-xl border border-border/50 text-xs text-muted transition-all hover:border-primary/40 hover:text-primary-light"
            >
              已有账号？返回登录
            </Link>
          </form>
        </div>

        {/* 底部 */}
        <p className="mt-6 text-center text-xs text-faint/60">
          注册即表示您同意我们的服务条款和隐私政策
        </p>
      </div>
    </div>
  )
}
