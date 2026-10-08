/** Regression coverage for the single-line sidebar title and separate version. */
import { renderToStaticMarkup } from 'react-dom/server'
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { Brand } from './Brand'

describe('Brand', () => {
  it('groups STUDIO after Topic Desk on one title line, above the version', () => {
    const html = renderToStaticMarkup(<Brand version="v0.3.0" />)
    expect(html).toContain('<span class="brand-title"><strong>Topic Desk</strong><small>STUDIO</small></span><span class="brand-version">v0.3.0</span>')
    expect(html).not.toContain('<br')
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    const titleRule = styles.match(/\.brand-title\s*\{([^}]+)\}/)?.[1]
    expect(titleRule).toContain('display: flex')
    expect(titleRule).toContain('white-space: nowrap')
  })
  it('preserves the release version and escapes any supplied label', () => {
    expect(renderToStaticMarkup(<Brand version="v0.3.0" />)).toContain('v0.3.0</span>')
    const html = renderToStaticMarkup(<Brand version="<script>" />)
    expect(html).toContain('&lt;script&gt;')
    expect(html).not.toContain('<script>')
  })
})
