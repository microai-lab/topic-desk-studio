/** Regression coverage for the reader's dynamic expand/restore affordance. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { ReaderExpandButton } from './ReaderExpandButton'

describe('ReaderExpandButton', () => {
  for (const locale of ['zh', 'en'] as const) {
    it(`changes icon, action label and pressed state in ${locale}`, () => {
      const compact = renderToStaticMarkup(<ReaderExpandButton expanded={false} locale={locale} onToggle={vi.fn()} />)
      const full = renderToStaticMarkup(<ReaderExpandButton expanded locale={locale} onToggle={vi.fn()} />)
      expect(compact).toContain('lucide-maximize')
      expect(compact).not.toContain('lucide-minimize')
      expect(full).toContain('lucide-minimize')
      expect(full).not.toContain('lucide-maximize')
      expect(compact).toContain('aria-pressed="false"')
      expect(full).toContain('aria-pressed="true"')
      expect(compact).toContain(`aria-label="${locale === 'zh' ? '放大' : 'Expand'}"`)
      expect(full).toContain(`aria-label="${locale === 'zh' ? '恢复分栏' : 'Split view'}"`)
    })
  }
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
