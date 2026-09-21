import { expect, test } from '@playwright/test'
import { login, sendMessage } from './helpers'

test.describe('对话流程（SSE 流式）', () => {
  test.beforeEach(async ({ page }) => {
    await login(page)
  })

  test('发送消息后出现用户气泡', async ({ page }) => {
    const text = '你好，帮我看看这件衣服'
    await sendMessage(page, text)
    // exact 匹配，避免命中 AI 回复中引用的同一段文字
    await expect(page.getByText(text, { exact: true })).toBeVisible()
  })

  test('AI 回复通过 SSE 返回并最终完整渲染', async ({ page }) => {
    const text = '你好'
    await sendMessage(page, text)

    // 等待流式回复出现（mock 无 skill 时的固定前缀）
    await expect(
      page.getByText(`收到你的问题：「${text}」`),
    ).toBeVisible({ timeout: 20_000 })

    // 流式结束：“停止生成”按钮消失，输入区恢复发送按钮
    await expect(
      page.getByRole('button', { name: '停止生成' }),
    ).toHaveCount(0)
    await expect(page.locator('.md-body')).toHaveCount(2)
  })

  test('Markdown 渲染：加粗、表格与代码块高亮', async ({ page }) => {
    // 欢迎语中的 **加粗** 被渲染为 <strong>
    const welcome = page.locator('.md-body').first()
    await expect(welcome.locator('strong').first()).toContainText(
      'Fashion AI 设计助手',
    )

    // 启用 fabric-query skill，使回复包含表格与 json 代码块
    await page.getByRole('button', { name: /面料查询/ }).first().click()
    await expect(page.getByText('1 个 Skill 已启用')).toBeVisible()

    await sendMessage(page, '真丝双绉')

    await expect(
      page.locator('h2', { hasText: '基本属性' }),
    ).toBeVisible({ timeout: 20_000 })
    await expect(page.locator('table').first()).toBeVisible()
    // SyntaxHighlighter 渲染的 <pre>，文本经分词但 textContent 完整
    await expect(
      page.locator('pre').filter({ hasText: 'breathable' }),
    ).toBeVisible()
  })

  test('打字机效果：流式过程中回复内容逐步增长', async ({ page }) => {
    await sendMessage(page, '请详细介绍一下你们能做什么，越详细越好')

    const reply = page.locator('.md-body').last()
    await expect(reply).toContainText('收到你的问题')

    let prev = (await reply.textContent()) ?? ''
    let grew = false
    // 在流式窗口内连续采样，观察到长度增长即通过
    for (let i = 0; i < 12; i += 1) {
      await page.waitForTimeout(120)
      const cur = (await reply.textContent()) ?? ''
      if (cur.length > prev.length) {
        grew = true
        break
      }
      prev = cur
    }
    expect(grew, '流式输出过程中回复内容应当逐步增长').toBe(true)
  })
})
