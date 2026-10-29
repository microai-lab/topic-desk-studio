/** Storage confirmation renders all localized consequences before any operation runs. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { messages } from './i18n'
import { StorageConfirmation, storageConfirmation } from './StorageConfirmation'
import type { StorageAction } from './StorageConfirmation'

describe('storage confirmation', () => {
  for (const locale of ['zh', 'en'] as const) {
    for (const action of ['optimize', 'backup', 'restore', 'restoreSources'] as const) {
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
  it('explains exactly what restoring source defaults resets and preserves', () => {
    for (const locale of ['zh', 'en'] as const) {
      const m = messages[locale]
      const content = storageConfirmation('restoreSources', m)
      expect(content.title).toBe(m.sourceRestoreDefaults)
      expect(content.description).toBe(m.sourceRestoreConfirm)
      expect(content.description).not.toBe(m.storageRestoreConfirm)
      expect(content.dangerous).toBe(true)
    }
    expect(messages.zh.sourceRestoreConfirm).toContain('自定义来源和历史话题不会删除')
  })

  it('marks replacement as destructive and explains backup retention', () => {
    for (const action of ['optimize', 'backup', 'restore'] as StorageAction[]) {
      expect(storageConfirmation(action, messages.zh).dangerous).toBe(action === 'restore')
    }
    expect(storageConfirmation('backup', messages.zh).description).toContain('3')
    expect(storageConfirmation('restore', messages.zh).description).toContain('覆盖')
  })
})
