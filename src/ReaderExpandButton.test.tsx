/** Regression coverage for the reader's dynamic expand/restore affordance. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { ReaderExpandButton } from './ReaderExpandButton'

describe('ReaderExpandButton', () => {
  for (const locale of ['zh', 'en'] as const) {
    it(`changes icon, action label and pressed state in ${locale}`, () => {
      const compact = renderToStaticMarkup(<ReaderExpandButton expanded={false} locale={locale} onToggle={vi.fn()} />)
      const full = renderToStaticMarkup(<ReaderExpandButton expanded locale={locale} onToggle={vi.fn()} />)
      expect(compact).toContain('lucide-panel-left-close')
      expect(compact).not.toContain('lucide-panel-left-open')
      expect(full).toContain('lucide-panel-left-open')
      expect(full).not.toContain('lucide-panel-left-close')
      expect(compact).toContain('aria-pressed="false"')
      expect(full).toContain('aria-pressed="true"')
      expect(compact).toContain(`aria-label="${locale === 'zh' ? '收起选题' : 'Hide topics'}"`)
      expect(full).toContain(`aria-label="${locale === 'zh' ? '展开选题' : 'Show topics'}"`)
    })
  }
  it('keeps the restore icon text-free while retaining accessible labels', () => {
    const html = renderToStaticMarkup(<ReaderExpandButton expanded locale="zh" onToggle={vi.fn()} />)
    expect(html).not.toContain('<span>')
    expect(html).toContain('title="展开选题"')
    expect(html).toContain('aria-label="展开选题"')
    expect(html).toContain('aria-controls="topic-list-pane"')
    expect(html).toContain('aria-expanded="false"')
    expect(html).toContain('data-topic-toggle="restore"')
    expect(renderToStaticMarkup(<ReaderExpandButton expanded={false} locale="en" onToggle={vi.fn()} />))
      .toContain('aria-expanded="true"')
  })
  it('invokes the same toggle in both states without submitting a form', () => {
    const toggle = vi.fn()
    for (const expanded of [false, true]) {
      const button = ReaderExpandButton({ expanded, locale: 'zh', onToggle: toggle })
      expect(button.props.type).toBe('button')
      button.props.onClick()
    }
    expect(toggle).toHaveBeenCalledTimes(2)
  })
})
