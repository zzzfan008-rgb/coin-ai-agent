import { expect, test } from '@playwright/test'

test.describe('认证：登录 / 注册', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/login')
  })

  test('登录页展示必要的页面元素', async ({ page }) => {
    await expect(
      page.getByRole('heading', { name: 'Fashion AI Platform' }),
    ).toBeVisible()
    await expect(page.locator('input[autocomplete="username"]')).toBeVisible()
    await expect(
      page.locator('input[autocomplete="current-password"]'),
    ).toBeVisible()
    await expect(
      page.getByRole('button', { name: /^登\s*录$/ }),
    ).toBeVisible()
    // 跳转注册页的链接
    await expect(page.getByRole('link', { name: '立即注册' })).toBeVisible()
  })

  test('输入正确账号密码后登录成功并跳转到对话页', async ({ page }) => {
    await page.locator('input[autocomplete="username"]').fill('designer')
    await page.locator('input[autocomplete="current-password"]').fill(
      'designer123',
    )
    await page.getByRole('button', { name: /^登\s*录$/ }).click()

    await page.waitForURL('/')
    await expect(page.locator('header h1')).toHaveText('欢迎使用 Fashion AI')
  })

  test('登录成功后 JWT token 存入 localStorage', async ({ page }) => {
    await page.locator('input[autocomplete="username"]').fill('designer')
    await page.locator('input[autocomplete="current-password"]').fill(
      'designer123',
    )
    await page.getByRole('button', { name: /^登\s*录$/ }).click()
    await page.waitForURL('/')

    const token = await page.evaluate(() => {
      const t = localStorage.getItem('fai_token')
      if (!t) return null
      const payload = JSON.parse(atob(t.split('.')[1]))
      return { token: t, payload }
    })
    expect(token).toBeTruthy()
    expect(token!.token.split('.')).toHaveLength(3)
    expect(token!.payload.sub).toBe('u-demo-0001')
    expect(token!.payload.mock).toBe(true)
  })

  test('错误密码登录显示错误提示且不跳转', async ({ page }) => {
    await page.locator('input[autocomplete="username"]').fill('designer')
    await page.locator('input[autocomplete="current-password"]').fill(
      'wrong-password',
    )
    await page.getByRole('button', { name: /^登\s*录$/ }).click()

    await expect(page.getByText('用户名或密码错误')).toBeVisible()
    await expect(page).toHaveURL(/\/login$/)
  })

  test('注册页对用户名和密码长度做表单验证', async ({ page }) => {
    await page.goto('/register')
    await expect(page.getByRole('heading', { name: '创建账号' })).toBeVisible()
    await expect(page.locator('input[type="email"]')).toBeVisible()

    // 用户名过短
    await page.locator('input[placeholder="至少 3 个字符"]').fill('ab')
    await page.locator('input[placeholder="至少 6 位"]').fill('abc123')
    await page.getByRole('button', { name: /^注\s*册$/ }).click()
    await expect(page.getByText('用户名至少 3 个字符')).toBeVisible()

    // 修正用户名后，密码过短
    await page.locator('input[placeholder="至少 3 个字符"]').fill('abcd')
    await page.locator('input[placeholder="至少 6 位"]').fill('12345')
    await page.getByRole('button', { name: /^注\s*册$/ }).click()
    await expect(page.getByText('密码至少 6 位')).toBeVisible()
    await expect(page).toHaveURL(/\/register$/)
  })

  test('填写合法信息注册成功并自动登录进入对话页', async ({ page }) => {
    await page.goto('/register')
    const username = `newuser_${Date.now().toString().slice(-6)}`
    await page.locator('input[placeholder="至少 3 个字符"]').fill(username)
    await page.locator('input[type="email"]').fill(`${username}@example.com`)
    await page.locator('input[placeholder="至少 6 位"]').fill('abc123')
    await page.getByRole('button', { name: /^注\s*册$/ }).click()

    await page.waitForURL('/')
    const token = await page.evaluate(() => localStorage.getItem('fai_token'))
    expect(token).toBeTruthy()
    // 新用户无会话，useChat 会自动创建一个
    await expect(page.locator('header h1')).toHaveText('新会话', {
      timeout: 15_000,
    })
  })
})
