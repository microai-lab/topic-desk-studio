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
      const html = renderToStaticMarkup(<SourceRegionActions sources={[]} locale={locale} busy={false} onDisable={() => {}} />)
      expect(html).toContain(locale === 'zh' ? '一键关闭国内数据源' : 'Disable domestic sources')
      expect(html).toContain(locale === 'zh' ? '一键关闭国外数据源' : 'Disable international sources')
    }
  })

  it('dispatches only enabled regions and suppresses empty or busy actions', () => {
    const onDisable = vi.fn()
    const buttons = (sources: SourceConfiguration[], busy = false) => SourceRegionActions({ sources, locale: 'en', busy, onDisable }).props.children
    const domestic = buttons([source])
    domestic[0].props.onClick()
    domestic[1].props.onClick()
    expect(onDisable).toHaveBeenCalledExactlyOnceWith('domestic')
    buttons([{ ...source, region: 'international' }])[1].props.onClick()
    expect(onDisable).toHaveBeenLastCalledWith('international')
    onDisable.mockClear()
    for (const controls of [buttons([]), buttons([{ ...source, enabled: false }]), buttons([source], true)]) {
      for (const button of controls) {
        expect(button.props.disabled).toBe(true)
        button.props.onClick()
      }
    }
    expect(onDisable).not.toHaveBeenCalled()
  })
})
