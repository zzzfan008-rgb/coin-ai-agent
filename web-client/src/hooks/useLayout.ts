import { createContext, useContext } from 'react'

interface LayoutContextValue {
  openSidebar: () => void
  refreshAll: () => Promise<void>
  setCurrentSessionId: (id?: string) => void
}

const LayoutContext = createContext<LayoutContextValue | null>(null)

export function useLayout(): LayoutContextValue {
  const ctx = useContext(LayoutContext)
  if (!ctx) throw new Error('useLayout 必须在 AppLayout 内使用')
  return ctx
}

export { LayoutContext }
