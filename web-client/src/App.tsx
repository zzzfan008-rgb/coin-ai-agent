import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom'
import type { ReactNode } from 'react'
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
            <Route path="/" element={<Chat />} />
            <Route path="/sessions/:id" element={<Chat />} />
            <Route path="/projects/:id" element={<ProjectDetail />} />
            {/* MCP 页：普通用户可管理自己被授权的开关；增删仅 admin 可见 */}
            <Route path="/mcp" element={<McpAdmin />} />
            {/* 知识库：仅管理员 */}
            <Route
              path="/knowledge"
              element={
                <RequireAdmin>
                  <KnowledgeAdmin />
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
