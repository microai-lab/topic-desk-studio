/** Confirmation controls must share an explicit cross-platform base instead of native button defaults. */
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

describe('shared confirmation controls', () => {
  it('provides the same complete baseline to maintenance, source reset and connectivity dialogs', () => {
    const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    const base = css.match(/\.storage-confirmation-actions button\s*\{([^}]+)\}/)![1]!
    // Without these declarations, native cancel buttons differ from the styled confirm/summary controls.
    for (const property of ['border', 'border-radius', 'background', 'color', 'font', 'min-height', 'min-width', 'cursor']) {
      expect(base).toMatch(new RegExp(`(?:^|;)\\s*${property}:`))
    }
    expect(css).not.toMatch(/\.source-connection-summary-dialog \.storage-confirmation-actions button/)
    expect(css).toContain('.storage-confirmation-actions button:focus-visible')
    expect(css).toContain('.storage-confirmation-actions button:disabled')
  })
})
