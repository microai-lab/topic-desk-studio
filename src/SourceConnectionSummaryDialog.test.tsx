/** Completed diagnostics preserve all counts and native dialog focus behavior in both locales. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { activateSourceConnectionSummary, SourceConnectionSummaryDialog, sourceConnectionSummary } from './SourceConnectionSummaryDialog'
import type { SourceConnectionBatchProgress } from './sourceConnectionPresentation'

const progress: SourceConnectionBatchProgress = { total: 48, completed: 48, success: 14, failed: 33, empty: 0, manual: 1 }
describe('completed connection test summary', () => {
  for (const locale of ['zh', 'en'] as const) {
    it(`renders the exact 48-source outcome in ${locale} without triggering dismissal`, () => {
      const close = vi.fn()
      const summary = sourceConnectionSummary(progress, locale)
      const html = renderToStaticMarkup(<SourceConnectionSummaryDialog progress={progress} locale={locale} onClose={close} />)
      expect(html).toContain('<dialog')
      expect(html).toContain('aria-labelledby=')
      expect(html).toContain('aria-describedby=')
      expect(html).toContain(locale === 'zh' ? '连通性测试完成' : 'Connectivity test complete')
      expect(html).toContain('48/48')
      expect(html).toContain(locale === 'zh' ? '知道了' : 'Done')
      expect(html.match(/<button\b/g)).toHaveLength(1)
      expect(summary.items.map(item => [item.key, item.count])).toEqual([['success', 14], ['failed', 33], ['empty', 0], ['manual', 1]])
      for (const item of summary.items) expect(html).toContain(`<dt>${item.label}</dt><dd>${item.count}</dd>`)
      expect(close).not.toHaveBeenCalled()
    })
    it(`retains zero and distinct non-success categories in ${locale}`, () => {
      const summary = sourceConnectionSummary({ total: 4, completed: 4, success: 0, failed: 0, empty: 3, manual: 1 }, locale)
      expect(summary.items.map(item => item.count)).toEqual([0, 0, 3, 1])
      expect(summary.items.map(item => item.label)).toEqual(locale === 'zh' ? ['正常', '失败', '无数据', '需网页测试'] : ['Passed', 'Failed', 'No items', 'Browser check'])
      expect(sourceConnectionSummary({ total: 0, completed: 0, success: 0, failed: 0, empty: 0, manual: 0 }, locale).description).toContain('0/0')
    })
  }
  it('does not infer a completion count from category totals', () => {
    expect(sourceConnectionSummary({ ...progress, total: 49 }, 'zh').description).toContain('48/49')
  })
})

describe('connection summary native modality', () => {
  it('opens modally, focuses Done, then closes and restores the previous control on cleanup', () => {
    const events: string[] = []
    const dialog = {
      open: false,
      showModal: vi.fn(() => { dialog.open = true; events.push('modal') }),
      close: vi.fn(() => { dialog.open = false; events.push('close') }),
    }
    const done = { focus: vi.fn(() => events.push('focus-done')) }
    const previous = { isConnected: true, focus: vi.fn(() => events.push('restore-focus')) }
    const cleanup = activateSourceConnectionSummary(dialog, done, previous)
    expect(events).toEqual(['modal', 'focus-done'])
    expect(previous.focus).not.toHaveBeenCalled()
    cleanup()
    expect(events).toEqual(['modal', 'focus-done', 'close', 'restore-focus'])
  })
  it('does not focus detached controls or re-close an already dismissed dialog', () => {
    const dialog = { open: true, showModal: vi.fn(), close: vi.fn() }
    const previous = { isConnected: false, focus: vi.fn() }
    const cleanup = activateSourceConnectionSummary(dialog, null, previous)
    dialog.open = false
    cleanup()
    expect(dialog.close).not.toHaveBeenCalled()
    expect(previous.focus).not.toHaveBeenCalled()
  })
  it('falls back to the batch trigger when no previous field remains, while respecting connected fields', () => {
    for (const connected of [false, true]) {
      const dialog = { open: true, showModal: vi.fn(), close: vi.fn() }
      const previous = { isConnected: connected, focus: vi.fn() }
      const batch = { focus: vi.fn() }, fallback = vi.fn(() => batch)
      activateSourceConnectionSummary(dialog, null, previous, fallback)()
      expect(previous.focus).toHaveBeenCalledTimes(connected ? 1 : 0)
      expect(batch.focus).toHaveBeenCalledTimes(connected ? 0 : 1)
      expect(fallback).toHaveBeenCalledTimes(connected ? 0 : 1)
    }
    const batch = { focus: vi.fn() }
    activateSourceConnectionSummary({ open: true, showModal: vi.fn(), close: vi.fn() }, null, null, () => batch)()
    expect(batch.focus).toHaveBeenCalledTimes(1)
  })
  it('does nothing when the dialog has not been attached', () => {
    const done = { focus: vi.fn() }, previous = { isConnected: true, focus: vi.fn() }
    activateSourceConnectionSummary(null, done, previous)()
    expect(done.focus).not.toHaveBeenCalled()
    expect(previous.focus).not.toHaveBeenCalled()
  })
})
