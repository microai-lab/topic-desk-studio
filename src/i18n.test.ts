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

/** Compact sidebar actions stay legible inside the narrow navigation footer. */
describe('sidebar toggle labels', () => {
  it('uses short Chinese labels in both directions', () => {
    expect(messages.zh.sidebarCollapse).toBe('收起')
    expect(messages.zh.sidebarExpand).toBe('展开')
  })
})

/** Proxy settings omit the retired routing explanation in both locales. */
describe('proxy settings copy', () => {
  it('removes the obsolete routing help text', () => {
    for (const locale of ['zh', 'en'] as const) {
      expect(messages[locale]).not.toHaveProperty('proxyHelp')
    }
  })
})

/** Search is scoped to topic titles; placeholder copy must describe that scope. */
describe('topic search copy', () => {
  it('names the searched content in both locales', () => {
    expect(messages.zh.filterSearchPlaceholder).toBe('搜索选题标题')
    expect(messages.en.filterSearchPlaceholder).toBe('Search topic titles')
  })
})
