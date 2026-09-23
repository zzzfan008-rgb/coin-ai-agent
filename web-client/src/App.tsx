import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom'
import React, { type ReactNode } from 'react'
import { AuthProvider, useAuth } from './hooks/useAuth'
import Login from './pages/Login'
import Register from './pages/Register'
import Chat from './pages/Chat'
import ProjectDetail from './pages/ProjectDetail'
import KnowledgeAdmin from './pages/KnowledgeAdmin'
import McpAdmin from './pages/McpAdmin'
import { AppLayout } from './components/Layout'

function RequireAuth({ children }: { children: ReactNode }) {
  const { isAuthenticated } = useAuth()
  if (!isAuthenticated) return <Navigate to="/login" replace />
  return <>{children}</>
}

function RequireAdmin({ children }: { children: ReactNode }) {
  const { isAuthenticated, user } = useAuth()
  if (!isAuthenticated) return <Navigate to="/login" replace />
  if (user?.role !== 'admin') return <Navigate to="/" replace />
  return <>{children}</>
}

class ErrorBoundary extends React.Component<
  { children: ReactNode },
  { hasError: boolean; error: string }
> {
  constructor(props: { children: ReactNode }) {
    super(props)
    this.state = { hasError: false, error: '' }
  }
  static getDerivedStateFromError(e: Error) {
    return { hasError: true, error: e.message }
  }
  componentDidCatch(e: Error, info: React.ErrorInfo) {
    console.error('[ErrorBoundary]', e, info)
  }
  render() {
    if (this.state.hasError) {
      return (
        <div className="flex flex-col items-center justify-center min-h-screen text-center p-8">
          <p className="text-sm font-semibold text-red-500">页面崩溃</p>
          <p className="mt-2 text-xs text-gray-500 max-w-md">{this.state.error}</p>
          <button
            className="mt-4 px-4 py-2 bg-primary text-white text-sm rounded-lg"
            onClick={() => location.reload()}
          >
            刷新页面
          </button>
        </div>
      )
    }
    return this.props.children
  }
}

export default function App() {
  return (
    <AuthProvider>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<Login />} />
          <Route path="/register" element={<Register />} />
          <Route
            element={
              <RequireAuth>
                <AppLayout />
              </RequireAuth>
            }
          >
            <Route path="/" element={<ErrorBoundary><Chat /></ErrorBoundary>} />
            <Route path="/sessions/:id" element={<ErrorBoundary><Chat /></ErrorBoundary>} />
            <Route path="/projects/:id" element={<ErrorBoundary><ProjectDetail /></ErrorBoundary>} />
            {/* MCP 页：普通用户可管理自己被授权的开关；增删仅 admin 可见 */}
            <Route path="/mcp" element={<ErrorBoundary><McpAdmin /></ErrorBoundary>} />
            {/* 知识库：仅管理员 */}
            <Route
              path="/knowledge"
              element={
                <RequireAdmin>
                  <ErrorBoundary><KnowledgeAdmin /></ErrorBoundary>
                </RequireAdmin>
              }
            />
          </Route>
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </BrowserRouter>
    </AuthProvider>
  )
}
