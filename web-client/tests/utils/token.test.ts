/**
 * Token / AuthUser 工具单元测试
 *
 * 测试点：
 * 1. saveAuth — 仅存用户信息（AuthUser），不存 token（token 由 HttpOnly cookie 管理）
 * 2. getStoredUser — 正确解析/返回用户对象
 * 3. clearAuth — 清理用户 key
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { saveAuth, getStoredUser, clearAuth, type AuthUser } from '../../src/api/client'

const USER_KEY = 'fai_user'

const fakeUser: AuthUser = {
  user_id: 'u-123',
  org_id: 'org-fake',
  dept_id: 'dept-fake',
  username: 'designer',
  display_name: '设计师小明',
  role: 'designer',
}

describe('Token 管理（HttpOnly Cookie 场景）', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  describe('saveAuth — 用户信息存储', () => {
    it('saveAuth 只存用户信息，不存 token（HttpOnly cookie 策略）', () => {
      saveAuth('jwt-from-server', fakeUser)

      // 用户信息应正确序列化
      expect(getStoredUser()).toMatchObject(fakeUser)
      // token key 不应出现在 localStorage（token 走 HttpOnly cookie）
      expect(localStorage.getItem('fai_token')).toBeNull()
    })

    it('saveAuth 覆盖旧用户信息', () => {
      saveAuth('jwt-1', fakeUser)
      const updated: AuthUser = { ...fakeUser, display_name: '新名字' }
      saveAuth('jwt-2', updated)

      const stored = getStoredUser()
      expect(stored?.display_name).toBe('新名字')
      expect(stored?.username).toBe('designer')
    })

    it('saveAuth 多次调用不累积 key（只保留最新用户）', () => {
      saveAuth('jwt-1', fakeUser)
      saveAuth('jwt-2', { ...fakeUser, user_id: 'u-456' })

      const keys = Array.from({ length: localStorage.length }, (_, i) => localStorage.key(i))
      expect(keys).toEqual(['fai_user'])
    })
  })

  describe('getStoredUser', () => {
    it('无用户时返回 null', () => {
      expect(getStoredUser()).toBeNull()
    })

    it('有效 JSON 用户数据正确解析', () => {
      localStorage.setItem(USER_KEY, JSON.stringify(fakeUser))
      expect(getStoredUser()).toEqual(fakeUser)
    })

    it('损坏的 JSON 返回 null（不抛异常）', () => {
      localStorage.setItem(USER_KEY, '{ broken json }')
      expect(getStoredUser()).toBeNull()
    })
  })

  describe('clearAuth — 清理', () => {
    it('clearAuth 清理用户 key', () => {
      localStorage.setItem(USER_KEY, JSON.stringify(fakeUser))

      clearAuth()

      expect(localStorage.getItem(USER_KEY)).toBeNull()
      expect(getStoredUser()).toBeNull()
    })

    it('clearAuth 对不存在的 key 不抛异常', () => {
      expect(() => clearAuth()).not.toThrow()
    })

    it('clearAuth 不影响其他 localStorage key', () => {
      localStorage.setItem('other_key', 'some_value')
      localStorage.setItem(USER_KEY, JSON.stringify(fakeUser))

      clearAuth()

      expect(localStorage.getItem('other_key')).toBe('some_value')
    })
  })
})
