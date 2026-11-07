/** Storage confirmation renders all localized consequences before any operation runs. */
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { messages } from './i18n'
import { StorageConfirmation, storageConfirmation } from './StorageConfirmation'
import type { StorageAction } from './StorageConfirmation'

describe('storage confirmation', () => {
  for (const locale of ['zh', 'en'] as const) {
    for (const action of ['optimize', 'backup', 'restore', 'restoreSources', 'clear'] as const) {
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

  it('keeps the destructive warning brief and moves category consequences to their own hints', () => {
    expect(storageConfirmation('clear', messages.zh).dangerous).toBe(true)
    expect(messages.zh.storageClearConfirm).toContain('无法撤销')
    expect(messages.en.storageClearConfirm).toContain('cannot be undone')
    expect(messages.zh.storageClearConfirm).not.toContain('退出所有网站')
    expect(messages.zh.storageClearLoginsHint).toContain('退出所有网站')
    expect(messages.zh.storageClearBackupsHint).toContain('无法恢复')
    expect(messages.en.storageClearCollectedHint).toContain('future runs')
  })

  it('starts with five unchecked categories and confirmation disabled in both languages', () => {
    for (const locale of ['zh', 'en'] as const) {
      const m = messages[locale]
      const html = renderToStaticMarkup(<StorageConfirmation action="clear" m={m} onCancel={() => {}} onConfirm={() => {}} />)
      expect(html.match(/type="checkbox"/g)).toHaveLength(5)
      expect(html).not.toContain('checked=""')
      expect(html).toContain('disabled=""')
      for (const label of [m.storageClearSelect, m.storageClearCollected, m.storageClearHistory, m.storageClearDownloads, m.storageClearLogins, m.storageClearBackups, m.storageClearSelected]) expect(html).toContain(label)
      for (const hint of [m.storageClearCollectedHint, m.storageClearDownloadsHint, m.storageClearLoginsHint, m.storageClearBackupsHint]) expect(html).toContain(hint)
      expect(html.match(/class="storage-clear-option"/g)).toHaveLength(5)
      expect(html).toContain('websiteLogins-hint')
    }
  })
})
