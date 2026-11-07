/** Disabled settings rows must be recoverable without appearing as enabled discovery choices. */
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { SourceSettingsGroups } from './SourceSettingsGroups'
import { messages } from './i18n'
import type { SourceConfiguration } from './types'

const source: SourceConfiguration = { code: 'enabled', displayName: 'Enabled', homeUrl: '', endpointUrl: '', region: 'domestic', category: 'general', parserType: 'rss', parserConfig: {}, proxyMode: 'direct', builtIn: false, enabled: true }

it('renders enabled rows above a closed disabled group with bilingual labels', () => {
  for (const locale of ['zh', 'en'] as const) {
    const html = renderToStaticMarkup(<SourceSettingsGroups sources={[{ ...source, code: 'disabled', enabled: false }, source]} busy={false} m={messages[locale]} renderSource={item => <span key={item.code}>{item.code}</span>} />)
    expect(html.indexOf('<span>enabled</span>')).toBeLessThan(html.indexOf('<details'))
    expect(html.indexOf('<details')).toBeLessThan(html.indexOf('<span>disabled</span>'))
    expect(html).not.toContain('open=""')
    expect(html).toContain(`${messages[locale].sourceDisabledGroup} (1)`)
  }
})

it('omits the disabled group when every source is enabled', () => {
  const html = renderToStaticMarkup(<SourceSettingsGroups sources={[source]} busy={false} m={messages.en} renderSource={item => <span key={item.code}>{item.code}</span>} />)
  expect(html).not.toContain('<details')
})

it('shows an empty-state explanation and zero overview counts when every source is disabled', () => {
  const html = renderToStaticMarkup(<SourceSettingsGroups sources={[{ ...source, enabled: false }]} busy={true} m={messages.zh} renderSource={item => <span key={item.code}>{item.code}</span>} />)
  expect(html).toContain(messages.zh.sourceNoEnabled)
  expect(html.match(/<dd>0<\/dd>/g)).toHaveLength(3)
  expect(html).toContain('aria-busy="true"')
  expect(html).toContain('<details')
})
