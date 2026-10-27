/** Preset controls show the saved choice and preserve explicit saving in both locales. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { ProxyAddressField } from './ProxyAddressField'
import { messages } from './i18n'
import { PROXY_PRESETS } from './networkPresentation'

describe('proxy address field', () => {
  for (const locale of ['zh', 'en'] as const) {
    for (const value of [...PROXY_PRESETS, '', 'http://127.0.0.1:8080']) {
      it(`renders both presets and the saved ${value || 'direct'} route in ${locale}`, () => {
        const change = vi.fn()
        const html = renderToStaticMarkup(<ProxyAddressField value={value} m={messages[locale]} onChange={change} />)
        expect(html).toContain(messages[locale].labelProxy)
        expect(html.match(/<input /g)).toHaveLength(1)
        expect(html).not.toContain('<select')
        expect(html).toContain('role="combobox"')
        expect(html).toContain('aria-expanded="false"')
        expect(html).toContain(`value="${value}"`)
        expect(html).toContain(`aria-label="${messages[locale].labelProxyPreset}"`)
        expect(change).not.toHaveBeenCalled()
      })
    }
  }
})
