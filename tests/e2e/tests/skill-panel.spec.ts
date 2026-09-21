import { expect, test } from '@playwright/test'
import { login } from './helpers'

test.describe('Skill 面板', () => {
  test.beforeEach(async ({ page }) => {
    // viewport 1440 ≥ xl(1280)，Skill 面板常驻可见
    await login(page)
  })

  test('Skill 面板存在，且 3 个 Skill 均可见', async ({ page }) => {
    await expect(
      page.getByRole('heading', { name: 'Skills' }),
    ).toBeVisible()

    await expect(
      page.getByRole('button', { name: /面料查询/ }),
    ).toBeVisible()
    await expect(
      page.getByRole('button', { name: /色彩搭配/ }),
    ).toBeVisible()
    await expect(
      page.getByRole('button', { name: /款式灵感/ }),
    ).toBeVisible()

    // 描述文案
    await expect(page.getByText(/根据面料名称查询/)).toBeVisible()
    await expect(page.getByText(/基于主色推荐搭配色/)).toBeVisible()
    await expect(page.getByText(/基于关键词生成款式创意/)).toBeVisible()
  })

  test('点击 Skill 后呈现选中状态', async ({ page }) => {
    const card = page.getByRole('button', { name: /面料查询/ })
    await card.click()

    // 选中样式：主色边框 + 主色浅底
    await expect(card).toHaveClass(/border-primary/)
    await expect(card).toHaveClass(/bg-primary\/10/)
    // 顶栏出现已启用计数
    await expect(page.getByText('1 个 Skill 已启用')).toBeVisible()

    // 再选第二个 Skill
    await page.getByRole('button', { name: /色彩搭配/ }).click()
    await expect(page.getByText('2 个 Skill 已启用')).toBeVisible()
  })

  test('再次点击已选 Skill 可取消选中', async ({ page }) => {
    const card = page.getByRole('button', { name: /面料查询/ })
    await card.click()
    await expect(card).toHaveClass(/bg-primary\/10/)

    await card.click()
    await expect(card).not.toHaveClass(/bg-primary\/10/)
    await expect(page.getByText('1 个 Skill 已启用')).toHaveCount(0)
  })
})
