/** Tab media affordance: activity, persistent mute, and isolated click handling. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { TabMediaButton } from './TabMediaButton'

describe('TabMediaButton', () => {
  it('does not occupy quiet unmuted tabs', () => {
    expect(TabMediaButton({ playing: false, muted: false, locale: 'zh', onToggle: vi.fn() })).toBeNull()
  })
  for (const locale of ['zh', 'en'] as const) {
    it(`reflects playing and muted states in ${locale}`, () => {
      const playing = renderToStaticMarkup(<TabMediaButton playing muted={false} locale={locale} onToggle={vi.fn()} />)
      const muted = renderToStaticMarkup(<TabMediaButton playing={false} muted locale={locale} onToggle={vi.fn()} />)
      expect(playing).toContain('lucide-volume-2')
      expect(playing).toContain('aria-pressed="false"')
      expect(muted).toContain('lucide-volume-x')
      expect(muted).toContain('aria-pressed="true"')
      expect(muted).toContain(locale === 'zh' ? '取消网页静音' : 'Unmute tab')
    })
  }
  it('toggles mute without activating or dragging another tab', () => {
    const toggle = vi.fn(), stop = vi.fn()
    const button = TabMediaButton({ playing: true, muted: false, locale: 'zh', onToggle: toggle })!
    button.props.onPointerDown({ stopPropagation: stop })
    button.props.onClick({ stopPropagation: stop })
    expect(toggle).toHaveBeenCalledTimes(1)
    expect(stop).toHaveBeenCalledTimes(2)
    expect(button.props.type).toBe('button')
  })
})
