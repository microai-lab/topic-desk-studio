/** Source diagnostic controls remain accessible, localized, and isolated from row dragging. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { SourceConnectionBatchTestButton, SourceConnectionTestButton, SourceConnectionTestResultView } from './SourceConnectionTest'
import type { SourceConnectionTestState } from './sourceConnectionPresentation'
import type { SourceConnectionTestResult } from './types'

const result: SourceConnectionTestResult = { code: 'example-feed', status: 'success', elapsedMs: 320, itemCount: 8, error: null, route: 'direct' }

describe('batch connection test control', () => {
  for (const locale of ['zh', 'en'] as const) {
    it(`offers a batch covering disabled sources and announces progress in ${locale}`, () => {
      const idle = renderToStaticMarkup(<SourceConnectionBatchTestButton locale={locale} running={false} completed={0} total={4} onTest={vi.fn()} />)
      const running = renderToStaticMarkup(<SourceConnectionBatchTestButton locale={locale} running completed={2} total={4} onTest={vi.fn()} />)
      expect(idle.replace(/<[^>]*>/g, '').trim()).toBe(locale === 'zh' ? '连通性测试' : 'Test connectivity')
      expect(idle).toContain(locale === 'zh' ? '重新自动判断所有来源的连接方式，包含已停用来源' : 'Recheck routes automatically for all sources, including disabled ones')
      expect(idle).not.toContain('disabled=""')
      expect(idle).toContain('lucide-plug-zap')
      expect(running).toContain(locale === 'zh' ? '测试中 2/4' : 'Testing 2/4')
      expect(running).toContain('disabled=""')
      expect(running).toContain('aria-busy="true"')
      expect(running).toContain('lucide-loader-circle')
    })
  }
  it('runs once when available and blocks duplicate or globally disabled requests', () => {
    const test = vi.fn()
    const ready = SourceConnectionBatchTestButton({ locale: 'zh', running: false, completed: 0, total: 1, onTest: test })
    ready.props.onClick()
    expect(ready.props.type).toBe('button')
    expect(test).toHaveBeenCalledTimes(1)
    const running = SourceConnectionBatchTestButton({ locale: 'zh', running: true, completed: 0, total: 1, onTest: test })
    const disabled = SourceConnectionBatchTestButton({ locale: 'zh', running: false, disabled: true, completed: 0, total: 0, onTest: test })
    running.props.onClick(); disabled.props.onClick()
    expect(running.props.disabled).toBe(true)
    expect(disabled.props.disabled).toBe(true)
    expect(test).toHaveBeenCalledTimes(1)
  })
})

describe('source connection controls', () => {
  for (const locale of ['zh', 'en'] as const) {
    it(`offers a saved-configuration test and reflects its pending state in ${locale}`, () => {
      const idle = renderToStaticMarkup(<SourceConnectionTestButton locale={locale} onTest={vi.fn()} />)
      const pending = renderToStaticMarkup(<SourceConnectionTestButton state={{ key: 'saved', phase: 'testing' }} locale={locale} onTest={vi.fn()} />)
      expect(idle).toContain(`aria-label="${locale === 'zh' ? '测试连通性' : 'Test connection'}"`)
      expect(idle).toContain(locale === 'zh' ? '本次测试重新自动判断直连或已保存的代理，不修改采集设置' : 'Recheck Direct or the saved proxy for this test without changing collection settings')
      expect(idle).toContain('source-icon-action source-connection-button')
      expect(idle).toContain('lucide-plug-zap')
      expect(idle).not.toContain('disabled=""')
      expect(pending).toContain('disabled=""')
      expect(pending).toContain('aria-busy="true"')
      expect(pending).toContain('lucide-loader-circle')
      expect(pending).toContain(locale === 'zh' ? '测试中…' : 'Testing…')
      // Labels stay accessible in attributes while the visible button contains only its glyph.
      expect(idle.replace(/<[^>]*>/g, '').trim()).toBe('')
      expect(pending.replace(/<[^>]*>/g, '').trim()).toBe('')
    })
    it(`distinguishes queued work from an active request in ${locale}`, () => {
      const state: SourceConnectionTestState = { key: 'saved', phase: 'queued' }
      const button = renderToStaticMarkup(<SourceConnectionTestButton state={state} locale={locale} onTest={vi.fn()} />)
      expect(button).toContain(`aria-label="${locale === 'zh' ? '等待测试' : 'Queued'}"`)
      expect(button).toContain('disabled=""')
      expect(button).toContain('aria-busy="true"')
      expect(button).toContain('lucide-clock-3')
      expect(button).not.toContain('lucide-loader-circle')
      expect(button.replace(/<[^>]*>/g, '').trim()).toBe('')
      const diagnostic = renderToStaticMarkup(<SourceConnectionTestResultView state={state} locale={locale} />)
      expect(diagnostic).toContain('lucide-clock-3')
      expect(diagnostic).toContain(locale === 'zh' ? '等待测试' : 'Queued')
    })
    it(`shows the current result with its count and route in ${locale}`, () => {
      const html = renderToStaticMarkup(<SourceConnectionTestResultView state={{ key: 'saved', phase: 'done', result }} locale={locale} />)
      expect(html).toContain('role="status"')
      expect(html).toContain('source-connection-result success')
      expect(html).toContain(locale === 'zh' ? '8 条' : '8 items')
      expect(html).toContain(locale === 'zh' ? '直连' : 'Direct')
      expect(html).toContain('320 ms')
    })
  }
  it('does not start row dragging or submit a form and ignores duplicate pending clicks', () => {
    const test = vi.fn(), stop = vi.fn()
    const button = SourceConnectionTestButton({ locale: 'zh', onTest: test })
    button.props.onPointerDown({ stopPropagation: stop })
    button.props.onClick({ stopPropagation: stop })
    expect(button.props.type).toBe('button')
    expect(test).toHaveBeenCalledTimes(1)
    expect(stop).toHaveBeenCalledTimes(2)
    const pending = SourceConnectionTestButton({ state: { key: 'saved', phase: 'testing' }, locale: 'zh', onTest: test })
    pending.props.onClick({ stopPropagation: stop })
    expect(test).toHaveBeenCalledTimes(1)
  })
  it('does not invoke a request while globally disabled or waiting in the batch queue', () => {
    const test = vi.fn(), stop = vi.fn()
    const disabled = SourceConnectionTestButton({ locale: 'zh', disabled: true, onTest: test })
    const queued = SourceConnectionTestButton({ state: { key: 'saved', phase: 'queued' }, locale: 'zh', onTest: test })
    for (const button of [disabled, queued]) {
      expect(button.props.disabled).toBe(true)
      button.props.onClick({ stopPropagation: stop })
    }
    expect(stop).toHaveBeenCalledTimes(2)
    expect(test).not.toHaveBeenCalled()
  })
  it('renders complete escaped failure reasons instead of hiding them behind a tooltip', () => {
    const message = `HTTP 403: <script>alert('blocked')</script> ${'long diagnostic reason '.repeat(20)}`
    const state: SourceConnectionTestState = { key: 'saved', phase: 'error', error: message }
    const html = renderToStaticMarkup(<SourceConnectionTestResultView state={state} locale="zh" />)
    expect(html).toContain('source-connection-error')
    expect(html).toContain('source-connection-result failed')
    expect(html).toContain('&lt;script&gt;')
    expect(html).not.toContain('<script>')
    expect(html.match(/long diagnostic reason/g)).toHaveLength(40)
  })
  it('does not reserve space before testing', () => {
    expect(SourceConnectionTestResultView({ locale: 'zh' })).toBeNull()
  })
})
