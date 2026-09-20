import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { Shirt } from 'lucide-react'
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

  const inputCls =
    'h-10 w-full rounded-lg border border-border bg-surface px-3.5 text-sm text-content placeholder:text-faint transition-colors focus:border-primary focus:outline-none'

  return (
    <div className="flex min-h-full items-center justify-center bg-bg px-4 py-8">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex flex-col items-center">
          <div className="mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-primary shadow-card">
            <Shirt size={22} className="text-white" />
          </div>
          <h1 className="text-2xl font-semibold text-content">创建账号</h1>
          <p className="mt-1 text-sm text-muted">注册后自动创建你的工作室</p>
        </div>

        <form
          onSubmit={handleSubmit}
          className="rounded-card border border-border bg-surface p-6 shadow-card"
        >
          <div className="space-y-3.5">
            <div>
              <label className="mb-1.5 block text-sm text-muted">用户名</label>
              <input
                className={inputCls}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="至少 3 个字符"
                autoComplete="username"
                required
              />
            </div>
            <div>
              <label className="mb-1.5 block text-sm text-muted">
                邮箱<span className="ml-1 text-faint">（选填）</span>
              </label>
              <input
                type="email"
                className={inputCls}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@example.com"
                autoComplete="email"
              />
            </div>
            <div>
              <label className="mb-1.5 block text-sm text-muted">密码</label>
              <input
                type="password"
                className={inputCls}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="至少 6 位"
                autoComplete="new-password"
                required
              />
            </div>

            <div className="border-t border-border pt-3.5">
              <p className="mb-2 text-xs text-faint">
                组织信息（首次注册将自动创建）
              </p>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="mb-1.5 block text-sm text-muted">
                    工作室
                  </label>
                  <input
                    className={inputCls}
                    value={orgName}
                    onChange={(e) => setOrgName(e.target.value)}
                    placeholder="工作室名称"
                  />
                </div>
                <div>
                  <label className="mb-1.5 block text-sm text-muted">
                    部门
                  </label>
                  <input
                    className={inputCls}
                    value={deptName}
                    onChange={(e) => setDeptName(e.target.value)}
                    placeholder="部门名称"
                  />
                </div>
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
            {submitting ? '注册中…' : '注 册'}
          </button>

          <p className="mt-4 text-center text-sm text-muted">
            已有账号？{' '}
            <Link to="/login" className="text-primary-light hover:underline">
              返回登录
            </Link>
          </p>
        </form>
      </div>
    </div>
  )
}
