/** Verify the flat source action row stays below counts and never invokes operations during rendering. */
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it, vi } from 'vitest'
import { SourceSettingsToolbar } from './SourceSettingsToolbar'
import { messages } from './i18n'

it('renders regional and maintenance actions directly without a heading or dropdown', () => {
  for (const locale of ['zh', 'en'] as const) {
    const run = vi.fn()
    const html = renderToStaticMarkup(<SourceSettingsToolbar m={messages[locale]}
      overview={<div>overview-fixture</div>}
      primary={<button onClick={run}>primary-fixture</button>}
      regional={<button onClick={run}>regional-fixture</button>}
      maintenance={<button onClick={run}>maintenance-fixture</button>}
      restore={<button onClick={run}>restore-fixture</button>} />)
    expect(html.indexOf('primary-fixture')).toBeLessThan(html.indexOf('overview-fixture'))
    expect(html.indexOf('overview-fixture')).toBeLessThan(html.indexOf('source-management-toolbar'))
    expect(html.indexOf('regional-fixture')).toBeGreaterThan(html.indexOf('source-management-toolbar'))
    expect(html.indexOf('maintenance-fixture')).toBeGreaterThan(html.indexOf('source-management-toolbar'))
    expect(html.indexOf('regional-fixture')).toBeGreaterThan(html.indexOf('maintenance-fixture'))
    expect(html.indexOf('restore-fixture')).toBeGreaterThan(html.indexOf('regional-fixture'))
    expect(html).not.toContain('<details')
    expect(html).not.toContain('<summary')
    expect(html).not.toContain(locale === 'zh' ? '批量管理' : 'Bulk actions')
    expect(html).not.toContain(locale === 'zh' ? '更多操作' : 'More actions')
    expect(run).not.toHaveBeenCalled()
  }
})
