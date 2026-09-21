import { expect, type Page } from '@playwright/test'

/**
 * 公共测试辅助：通过 UI 完成 demo 账号登录。
 *
 * demo 种子账号（见 web-client/src/mocks/data.ts）：
 *   username: designer
 *   password: designer123
 *
 * 登录成功后等待 Chat 页完成会话加载（header 显示种子会话标题），
 * 确保后续操作时输入框已可用。
 */
export async function login(page: Page): Promise<void> {
  await page.goto('/login')
  await page.locator('input[autocomplete="username"]').fill('designer')
  await page.locator('input[autocomplete="current-password"]').fill('designer123')
  await page.getByRole('button', { name: /^登\s*录$/ }).click()
  await page.waitForURL('/')
  await expect(page.locator('header h1')).toHaveText('欢迎使用 Fashion AI', {
    timeout: 15_000,
  })
}

/** 发送一条消息（输入并点击发送按钮） */
export async function sendMessage(page: Page, text: string): Promise<void> {
  await page.locator('textarea').fill(text)
  await page.getByRole('button', { name: '发送' }).click()
}
