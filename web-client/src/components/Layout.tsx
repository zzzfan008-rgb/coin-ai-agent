import {
  useCallback,
  useEffect,
  useState,
} from 'react'
import { Outlet, useNavigate, useParams } from 'react-router-dom'
import { useAuth } from '../hooks/useAuth'
import { useProjects } from '../hooks/useProjects'
import {
  listSessions,
  updateSession,
  deleteSession,
  type Session,
} from '../api/client'
import { Sidebar } from './Sidebar'
import { ProjectFormModal } from './ProjectFormModal'
import { LayoutContext } from '../hooks/useLayout'

interface LayoutContextValue {
  openSidebar: () => void
  refreshAll: () => Promise<void>
  setCurrentSessionId: (id?: string) => void
}

export { LayoutContext }
export type { LayoutContextValue }

export function AppLayout() {
  const { user, logout } = useAuth()
  const projects = useProjects()
  const navigate = useNavigate()
  const params = useParams()

  const [drawerOpen, setDrawerOpen] = useState(false)
  const [showCreate, setShowCreate] = useState(false)
  const [sessions, setSessions] = useState<Session[]>([])
  const [archivedSessions, setArchivedSessions] = useState<Session[]>([])
  const [currentSessionId, setCurrentSessionId] = useState<string | undefined>(
    params.id,
  )

  const reloadSessions = useCallback(async () => {
    try {
      const [cur, archived] = await Promise.all([
        listSessions(),
        listSessions(true),
      ])
      setSessions(cur.sessions)
      setArchivedSessions(archived.sessions)
    } catch {
      // sidebar session refresh is best-effort
    }
  }, [])

  const handleArchiveSession = useCallback(
    async (id: string) => {
      try {
        await updateSession(id, { is_archived: true })
        await reloadSessions()
      } catch {
        // archive failure is surfaced by next reload
      }
    },
    [reloadSessions],
  )

  const handleDeleteSession = useCallback(
    async (id: string) => {
      try {
        await deleteSession(id)
        await reloadSessions()
      } catch {
        // delete failure is surfaced by next reload
      }
    },
    [reloadSessions],
  )

  const handleRestoreSession = useCallback(
    async (id: string) => {
      try {
        await updateSession(id, { is_archived: false })
        await reloadSessions()
      } catch {
        // restore failure is surfaced by next reload
      }
    },
    [reloadSessions],
  )

  useEffect(() => {
    void reloadSessions()
  }, [reloadSessions])

  // 路由变化时同步高亮的会话
  useEffect(() => {
    if (params.id) setCurrentSessionId(params.id)
  }, [params.id])

  const refreshAll = useCallback(async () => {
    await reloadSessions()
    await projects.refresh()
  }, [reloadSessions, projects])

  const value: LayoutContextValue = {
    openSidebar: () => setDrawerOpen(true),
    refreshAll,
    setCurrentSessionId,
  }

  const sidebar = (onClose?: () => void) => (
    <Sidebar
      sessions={sessions}
      projects={projects.projects}
      archivedProjects={projects.archived}
      currentSessionId={currentSessionId}
      currentProjectId={params.id}
      user={user}
      onNewSession={() => {
        setDrawerOpen(false)
        navigate('/?new=1')
      }}
      onCreateProject={() => setShowCreate(true)}
      onLogout={logout}
      onClose={onClose}
      archivedSessions={archivedSessions}
      onArchiveSession={handleArchiveSession}
      onDeleteSession={handleDeleteSession}
      onRestoreSession={handleRestoreSession}
    />
  )

  return (
    <LayoutContext.Provider value={value}>
      <div className="flex h-full overflow-hidden bg-bg">
        <div className="hidden shrink-0 lg:block">{sidebar()}</div>

        {drawerOpen && (
          <div className="fixed inset-0 z-40 lg:hidden">
            <div
              className="absolute inset-0 bg-black/60"
              onClick={() => setDrawerOpen(false)}
            />
            <div className="absolute left-0 top-0 h-full">
              {sidebar(() => setDrawerOpen(false))}
            </div>
          </div>
        )}

        <main className="flex min-w-0 flex-1 overflow-hidden">
          <Outlet />
        </main>

        {showCreate && (
          <ProjectFormModal
            onClose={() => setShowCreate(false)}
            onSubmit={async (input) => {
              const p = await projects.create(input)
              navigate(`/projects/${p.id}`)
            }}
          />
        )}
      </div>
    </LayoutContext.Provider>
  )
}
