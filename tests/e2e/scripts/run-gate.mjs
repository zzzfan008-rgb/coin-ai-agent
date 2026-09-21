#!/usr/bin/env node
/**
 * E2E 门禁执行器
 *
 * 1. 调用 Playwright（line 控制台输出 + json 结果文件）
 * 2. 汇总通过/失败统计、失败用例详情
 * 3. 生成 gate-report.md / gate-report.json
 * 4. 透传 Playwright 退出码（失败时非 0）
 *
 * 用法：npm run gate
 */
import { spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const e2eRoot = resolve(here, '..')
const resultsDir = resolve(e2eRoot, 'test-results')
const jsonPath = resolve(resultsDir, 'results.json')
const npx = process.platform === 'win32' ? 'npx.cmd' : 'npx'

mkdirSync(resultsDir, { recursive: true })

const run = spawnSync(
  npx,
  ['playwright', 'test', '--reporter=line,json'],
  {
    cwd: e2eRoot,
    stdio: 'inherit',
    env: { ...process.env, PLAYWRIGHT_JSON_OUTPUT_NAME: jsonPath },
  },
)

let result
try {
  result = JSON.parse(readFileSync(jsonPath, 'utf8'))
} catch (e) {
  console.error('无法读取 Playwright JSON 结果文件:', e.message)
  process.exit(run.status ?? 1)
}

// ── 展开 suites，收集 spec / test 信息 ─────────────────────────────────────

const STATUS_LABEL = {
  passed: '通过',
  failed: '失败',
  timedOut: '超时',
  skipped: '跳过',
  interrupted: '中断',
  expected: '通过',
  unexpected: '失败',
  flaky: '通过(重试)',
}

const cases = []
function walk(suites, file) {
  for (const suite of suites ?? []) {
    const currentFile = suite.file ? relative(suite.file) : file
    for (const spec of suite.specs ?? []) {
      // 一个 spec 通常只有一个 test；兼容多个
      for (const t of spec.tests ?? []) {
        const results = t.results ?? []
        const last = results[results.length - 1] ?? {}
        const flaky =
          results.length > 1 && results.some((r) => r.status === 'failed') &&
          last.status === 'passed'
        cases.push({
          file: currentFile,
          title: spec.title,
          line: spec.line,
          status: flaky ? 'flaky' : last.status ?? 'unknown',
          duration: results.reduce((s, r) => s + (r.duration ?? 0), 0),
          errors: results
            .filter((r) => r.error)
            .map((r) => cleanError(r.error)),
        })
      }
    }
    walk(suite.suites, currentFile)
  }
}
function relative(p) {
  return p.replace(/\\/g, '/').split('/tests/')[1] ?? p
}
function cleanError(error) {
  const message = (error.message ?? '').replace(/\u001b\[[0-9;]*m/g, '')
  const stack = (error.stack ?? '').replace(/\u001b\[[0-9;]*m/g, '')
  const location = error.location
  return { message: message.trim(), stack: stack.trim(), location }
}

walk(result.suites, null)

const total = cases.length
const failed = cases.filter((c) => ['failed', 'timedOut', 'interrupted'].includes(c.status)).length
const passed = cases.filter((c) => c.status === 'passed').length
const flaky = cases.filter((c) => c.status === 'flaky').length
const skipped = cases.filter((c) => c.status === 'skipped').length
const durationMs = cases.reduce((s, c) => s + c.duration, 0)
const gatePassed = failed === 0
const passRate = total ? `${Math.round(((passed + flaky) / total) * 100)}%` : '0%'

// ── 输出 JSON 汇总 ─────────────────────────────────────────────────────────
const summary = {
  generatedAt: new Date().toISOString(),
  gatePassed,
  stats: { total, passed, failed, flaky, skipped, passRate, durationMs },
  cases: cases.map((c) => ({
    file: c.file,
    title: c.title,
    status: c.status,
    duration: c.duration,
  })),
}
writeFileSync(resolve(e2eRoot, 'gate-report.json'), JSON.stringify(summary, null, 2))

// ── 输出 Markdown 门禁报告 ─────────────────────────────────────────────────
const icon = {
  passed: '✅',
  failed: '❌',
  timedOut: '⏱️',
  skipped: '⏭️',
  flaky: '⚠️',
}
const L = []
L.push('# E2E 门禁报告 — Phase 1A（T-009）')
L.push('')
L.push(`生成时间：${new Date().toLocaleString('zh-CN', { hour12: false })}`)
L.push(`前端环境：MSW mock 模式（http://localhost:5174）`)
L.push('')
L.push(`## 门禁结论：${gatePassed ? '✅ PASS' : '❌ FAIL'}`)
L.push('')
L.push('| 指标 | 值 |')
L.push('| --- | --- |')
L.push(`| 用例总数 | ${total} |`)
L.push(`| 通过 | ${passed} |`)
L.push(`| 失败 | ${failed} |`)
L.push(`| Flaky | ${flaky} |`)
L.push(`| 跳过 | ${skipped} |`)
L.push(`| 通过率 | ${passRate} |`)
L.push(`| 总耗时 | ${(durationMs / 1000).toFixed(1)}s |`)
L.push('')

const failedCases = cases.filter((c) => c.errors.length > 0)
if (failedCases.length) {
  L.push('## 失败用例详情')
  L.push('')
  for (const c of failedCases) {
    L.push(`### ❌ ${c.title}`)
    L.push('')
    L.push(`- 文件：\`tests/${c.file}\``)
    if (c.line) L.push(`- 起始行：${c.line}`)
    L.push(`- 状态：${STATUS_LABEL[c.status] ?? c.status}`)
    L.push('')
    for (const e of c.errors) {
      const lines = e.message.split('\n').slice(0, 12).join('\n')
      L.push('```')
      L.push(lines)
      L.push('```')
      L.push('')
    }
  }
} else {
  L.push('## 失败用例详情')
  L.push('')
  L.push('无失败用例。')
  L.push('')
}

L.push('## 用例清单')
L.push('')
const byFile = new Map()
for (const c of cases) {
  if (!byFile.has(c.file)) byFile.set(c.file, [])
  byFile.get(c.file).push(c)
}
for (const [file, list] of byFile) {
  L.push(`### ${file}`)
  L.push('')
  L.push('| 结果 | 用例 | 耗时 |')
  L.push('| --- | --- | --- |')
  for (const c of list) {
    L.push(
      `| ${icon[c.status] ?? ''} ${STATUS_LABEL[c.status] ?? c.status} | ${c.title} | ${(c.duration / 1000).toFixed(2)}s |`,
    )
  }
  L.push('')
}

writeFileSync(resolve(e2eRoot, 'gate-report.md'), L.join('\n'))

// ── 控制台简要输出 ─────────────────────────────────────────────────────────
console.log('')
console.log('──────── E2E 门禁 ────────')
console.log(
  `总数 ${total} | 通过 ${passed} | 失败 ${failed} | Flaky ${flaky} | 跳过 ${skipped} | 通过率 ${passRate}`,
)
console.log(`报告：gate-report.md / gate-report.json`)
console.log(`结论：${gatePassed ? 'PASS' : 'FAIL'}`)

process.exit(gatePassed ? 0 : run.status ?? 1)
