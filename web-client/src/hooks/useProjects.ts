import { useCallback, useEffect, useState } from 'react'
import {
  archiveProject,
  createProject,
  deleteProject,
  listProjects,
  unarchiveProject,
  updateProject,
  type Project,
} from '../api/client'

export interface ProjectInput {
  name: string
  description?: string
  cover_color?: string
}

export function useProjects() {
  const [projects, setProjects] = useState<Project[]>([])
  const [archived, setArchived] = useState<Project[]>([])
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const [active, archivedRes] = await Promise.all([
        listProjects(false),
        listProjects(true),
      ])
      setProjects(active.projects)
      setArchived(archivedRes.projects)
      setError(null)
      return true
    } catch (e) {
      setError(e instanceof Error ? e.message : '项目加载失败')
      return false
    }
  }, [])

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const ok = await refresh()
      if (!cancelled && ok) setLoaded(true)
    })()
    return () => {
      cancelled = true
    }
  }, [refresh])

  const create = useCallback(
    async (input: ProjectInput) => {
      const p = await createProject(input)
      await refresh()
      return p
    },
    [refresh],
  )

  const update = useCallback(
    async (id: string, input: Partial<ProjectInput>) => {
      const p = await updateProject(id, input)
      await refresh()
      return p
    },
    [refresh],
  )

  const archive = useCallback(
    async (id: string) => {
      await archiveProject(id)
      await refresh()
    },
    [refresh],
  )

  const unarchive = useCallback(
    async (id: string) => {
      await unarchiveProject(id)
      await refresh()
    },
    [refresh],
  )

  const remove = useCallback(
    async (id: string) => {
      await deleteProject(id)
      await refresh()
    },
    [refresh],
  )

  return {
    projects,
    archived,
    loaded,
    error,
    refresh,
    create,
    update,
    archive,
    unarchive,
    remove,
  }
}
