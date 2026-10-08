/** Localized model-save notices describe the outcome, not storage implementation. */
import { describe, expect, it } from 'vitest'
import { messages } from './i18n'

describe('model settings save notice', () => {
  it('shows a concise success message in Chinese and English', () => {
    expect(messages.zh.saveNotice).toBe('保存成功')
    expect(messages.en.saveNotice).toBe('Saved successfully')
  })
  it('does not expose database details in either locale', () => {
    for (const locale of ['zh', 'en'] as const) {
      expect(messages[locale].saveNotice).not.toMatch(/sqlite|database|数据库/i)
    }
  })
})
