/** Regression coverage for bilingual regional disable controls and empty/busy states. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { SourceRegionActions } from './SourceRegionActions'
import type { SourceConfiguration } from './types'

const source: SourceConfiguration = {
  code: 'test', displayName: 'Test', homeUrl: '', endpointUrl: '', region: 'domestic',
  category: 'technology', parserType: 'rss', proxyMode: 'auto', enabled: true, builtIn: false, parserConfig: {},
}

describe('regional source actions', () => {
  it('labels both regions in Chinese and English', () => {
    for (const locale of ['zh', 'en'] as const) {
      const html = renderToStaticMarkup(<SourceRegionActions sources={[source, { ...source, region: 'international' }]} locale={locale} busy={false} onSetEnabled={() => {}} />)
      expect(html).toContain(locale === 'zh' ? '国内数据源</button>' : 'Domestic sources</button>')
      expect(html).toContain(locale === 'zh' ? '国际数据源</button>' : 'International sources</button>')
      expect(html).toContain('lucide-map-pin')
      expect(html).toContain('lucide-globe')
      expect(html.match(/stroke-width="1.75"/g)).toHaveLength(2)
      expect(html.match(/aria-hidden="true"/g)).toHaveLength(2)
      for (const hint of locale === 'zh' ? ['关闭所有国内数据源', '关闭所有国际数据源'] : ['Disable all domestic sources', 'Disable all international sources']) {
        expect(html).toContain(`title="${hint}"`)
        expect(html).toContain(`aria-label="${hint}"`)
      }
    }
  })

  it('closes mixed regions and reopens fully disabled regions, reflecting saved state', () => {
    const onSetEnabled = vi.fn()
    for (const locale of ['zh', 'en'] as const) {
      for (const region of ['domestic', 'international'] as const) {
        const index = region === 'domestic' ? 0 : 1
        const closed = [{ ...source, region, enabled: false }]
        const button = SourceRegionActions({ sources: closed, locale, busy: false, onSetEnabled }).props.children[index]
        expect(button.props['aria-pressed']).toBe(false)
        expect(button.props.title).toContain(locale === 'zh' ? '打开所有' : 'Enable all')
        button.props.onClick()
        expect(onSetEnabled).toHaveBeenLastCalledWith(region, true)
        const mixed = [...closed, { ...source, code: 'second', region }]
        const mixedButton = SourceRegionActions({ sources: mixed, locale, busy: false, onSetEnabled }).props.children[index]
        expect(mixedButton.props['aria-pressed']).toBe(true)
        mixedButton.props.onClick()
        expect(onSetEnabled).toHaveBeenLastCalledWith(region, false)
      }
    }
  })

  it('dispatches only enabled regions and suppresses empty or busy actions', () => {
    const onSetEnabled = vi.fn()
    const buttons = (sources: SourceConfiguration[], busy = false) => SourceRegionActions({ sources, locale: 'en', busy, onSetEnabled }).props.children
    const domestic = buttons([source])
    domestic[0].props.onClick()
    domestic[1].props.onClick()
    expect(onSetEnabled).toHaveBeenCalledExactlyOnceWith('domestic', false)
    buttons([{ ...source, region: 'international' }])[1].props.onClick()
    expect(onSetEnabled).toHaveBeenLastCalledWith('international', false)
    onSetEnabled.mockClear()
    for (const controls of [buttons([]), buttons([source], true)]) {
      for (const button of controls) {
        expect(button.props.disabled).toBe(true)
        button.props.onClick()
      }
    }
    expect(onSetEnabled).not.toHaveBeenCalled()
  })
})
