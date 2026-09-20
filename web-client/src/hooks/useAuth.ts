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
  getToken,
  login as apiLogin,
  register as apiRegister,
  saveAuth,
  type AuthUser,
  type LoginRequest,
  type RegisterRequest,
} from '../api/client'

interface AuthContextValue {
  user: AuthUser | null
  token: string | null
  isAuthenticated: boolean
  login: (req: LoginRequest) => Promise<void>
  register: (req: RegisterRequest) => Promise<void>
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<AuthUser | null>(() => getStoredUser())
  const [token, setToken] = useState<string | null>(() => getToken())

  const login = useCallback(async (req: LoginRequest) => {
    const res = await apiLogin(req)
    const { token: jwt, ...u } = res
    saveAuth(jwt, u)
    setToken(jwt)
    setUser(u)
  }, [])

  const register = useCallback(async (req: RegisterRequest) => {
    const res = await apiRegister(req)
    const { token: jwt, ...u } = res
    saveAuth(jwt, u)
    setToken(jwt)
    setUser(u)
  }, [])

  const logout = useCallback(() => {
    clearAuth()
    setToken(null)
    setUser(null)
  }, [])

  const value = useMemo<AuthContextValue>(
    () => ({
      user,
      token,
      isAuthenticated: Boolean(token && user),
      login,
      register,
      logout,
    }),
    [user, token, login, register, logout],
  )

  return createElement(AuthContext.Provider, { value }, children)
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth 必须在 <AuthProvider> 内使用')
  return ctx
}
