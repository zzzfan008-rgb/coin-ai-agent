import { expect, test } from '@playwright/test'
import { login } from './helpers'

test.describe('侧边栏：会话列表', () => {
  test.beforeEach(async ({ page }) => {
    await login(page)
  })

  test('会话列表存在并展示种子会话与当前用户', async ({ page }) => {
    await expect(
      page.locator('button[title="欢迎使用 Fashion AI"]'),
    ).toBeVisible()
    await expect(page.getByText('示例设计师')).toBeVisible()
    await expect(
      page.getByRole('button', { name: '新建会话' }),
    ).toBeVisible()
  })

  test('点击“新建会话”后创建并切换到空的新会话', async ({ page }) => {
    await page.getByRole('button', { name: '新建会话' }).click()

    // 当前会话标题切换，消息区显示空态引导
    await expect(page.locator('header h1')).toHaveText('新会话')
    await expect(page.getByText('开始一个新会话')).toBeVisible()
    // 侧边栏新增一条会话
    await expect(page.locator('button[title="新会话"]')).toBeVisible()
    await expect(
      page.locator('button[title="欢迎使用 Fashion AI"]'),
    ).toBeVisible()
  })

  test('可以在会话之间切换并加载对应消息', async ({ page }) => {
    // 先新建一个会话
    await page.getByRole('button', { name: '新建会话' }).click()
    await expect(page.locator('header h1')).toHaveText('新会话')

    // 切回种子会话
    const seedButton = page.locator(
      'button[title="欢迎使用 Fashion AI"]',
    )
    await seedButton.click()

    await expect(page.locator('header h1')).toHaveText('欢迎使用 Fashion AI')
    // 种子会话呈现激活样式
    await expect(seedButton).toHaveClass(/bg-primary\/15/)
    // 种子会话的欢迎消息被加载
    await expect(page.getByText('Fashion AI 设计助手')).toBeVisible()
  })
})
