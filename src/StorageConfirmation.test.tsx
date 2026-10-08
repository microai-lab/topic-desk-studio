/** Storage confirmation renders all localized consequences before any operation runs. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { messages } from './i18n'
import { StorageConfirmation, storageConfirmation } from './StorageConfirmation'
import type { StorageAction } from './StorageConfirmation'

describe('storage confirmation', () => {
  for (const locale of ['zh', 'en'] as const) {
    for (const action of ['optimize', 'backup', 'restore'] as const) {
      it(`requires a choice before ${action} in ${locale}`, () => {
        const execute = vi.fn()
        const m = messages[locale]
        const html = renderToStaticMarkup(<StorageConfirmation action={action} m={m} onCancel={vi.fn()} onConfirm={execute} />)
        expect(html).toContain('<dialog')
        expect(html).toContain('aria-describedby=')
        expect(html).toContain(m.sourceCancel)
        expect(html).toContain(storageConfirmation(action, m).description)
        expect(execute).not.toHaveBeenCalled()
      })
    }
  }
  it('marks replacement as destructive and explains backup retention', () => {
    for (const action of ['optimize', 'backup', 'restore'] as StorageAction[]) {
      expect(storageConfirmation(action, messages.zh).dangerous).toBe(action === 'restore')
    }
    expect(storageConfirmation('backup', messages.zh).description).toContain('3')
    expect(storageConfirmation('restore', messages.zh).description).toContain('覆盖')
  })
})
