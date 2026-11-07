/** Selection validation covers every combination before destructive confirmation. */
import { expect, it } from 'vitest'
import { clearDataOptions, EMPTY_CLEAR_SELECTION, hasClearSelection } from './storagePresentation'
import { messages } from './i18n'

it('keeps the five selection keys intact with short labels and separate relevant consequences', () => {
  for (const locale of ['zh', 'en'] as const) {
    const options = clearDataOptions(messages[locale])
    expect(options.map(option => option.key)).toEqual(['collectedData', 'browserHistory', 'downloadRecords', 'websiteLogins', 'localBackups'])
    expect(options.find(option => option.key === 'websiteLogins')?.hint).toBe(messages[locale].storageClearLoginsHint)
    expect(options.find(option => option.key === 'downloadRecords')?.hint).toBe(messages[locale].storageClearDownloadsHint)
    expect(options.find(option => option.key === 'browserHistory')?.hint).toBeUndefined()
  }
})

it('rejects the initial empty selection and permits every nonempty combination', () => {
  expect(hasClearSelection(EMPTY_CLEAR_SELECTION)).toBe(false)
  for (let mask = 0; mask < 32; mask += 1) {
    expect(hasClearSelection({ collectedData: Boolean(mask & 1), browserHistory: Boolean(mask & 2), downloadRecords: Boolean(mask & 4), websiteLogins: Boolean(mask & 8), localBackups: Boolean(mask & 16) })).toBe(mask !== 0)
  }
})
