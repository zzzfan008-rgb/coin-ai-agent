import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from 'react'
import {
  clearAuth,
  getStoredUser,
  login as apiLogin,
  logout as apiLogout,
  register as apiRegister,
  saveAuth,
  type AuthUser,
  type LoginRequest,
  type RegisterRequest,
} from '../api/client'

interface AuthContextValue {
  user: AuthUser | null
  isAuthenticated: boolean
  login: (req: LoginRequest) => Promise<void>
  register: (req: RegisterRequest) => Promise<void>
  logout: () => Promise<void>
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<AuthUser | null>(() => getStoredUser())

  const login = useCallback(async (req: LoginRequest) => {
    const res = await apiLogin(req)
    const { token: _jwt, ...u } = res
    saveAuth(_jwt, u)
    setUser(u)
  }, [])

  const register = useCallback(async (req: RegisterRequest) => {
    const res = await apiRegister(req)
    const { token: _jwt, ...u } = res
    saveAuth(_jwt, u)
    setUser(u)
  }, [])

  // Clear both the localStorage state AND the httpOnly cookie on the gateway.
  const logout = useCallback(async () => {
    try {
      await apiLogout()
    } catch {
      // Proceed with local cleanup even if the API call fails (e.g. token already expired).
    }
    clearAuth()
    setUser(null)
  }, [])

  const value = useMemo<AuthContextValue>(
    () => ({
      user,
      isAuthenticated: Boolean(user),
      login,
      register,
      logout,
    }),
    [user, login, register, logout],
  )

  return createElement(AuthContext.Provider, { value }, children)
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth 必须在 <AuthProvider> 内使用')
  return ctx
}
