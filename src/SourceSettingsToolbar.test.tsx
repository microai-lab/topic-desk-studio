/** Verify action hierarchy and bilingual maintenance disclosure without invoking native operations. */
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it, vi } from 'vitest'
import { SourceSettingsToolbar } from './SourceSettingsToolbar'
import { messages } from './i18n'

it('keeps primary and regional actions visible and maintenance inside a closed disclosure', () => {
  for (const locale of ['zh', 'en'] as const) {
    const run = vi.fn()
    const html = renderToStaticMarkup(<SourceSettingsToolbar m={messages[locale]}
      primary={<button onClick={run}>primary-fixture</button>}
      regional={<button onClick={run}>regional-fixture</button>}
      maintenance={<button onClick={run}>maintenance-fixture</button>} />)
    expect(html.indexOf('primary-fixture')).toBeLessThan(html.indexOf('<details'))
    expect(html.indexOf('regional-fixture')).toBeLessThan(html.indexOf('<details'))
    expect(html.indexOf('maintenance-fixture')).toBeGreaterThan(html.indexOf('<details'))
    expect(html).not.toContain('open=""')
    expect(html).toContain(messages[locale].sourceMoreActions)
    expect(html).toContain(messages[locale].sourceBulkActions)
    expect(run).not.toHaveBeenCalled()
  }
})
