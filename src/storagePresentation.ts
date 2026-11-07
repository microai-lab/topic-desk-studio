/** Pure selection rules for destructive local-data maintenance. */
import type { ClearDataSelection } from './types'
import type { Messages } from './i18n'

/** Short labels and local consequences replace the duplicated all-category warning paragraph. */
export function clearDataOptions(m: Messages): readonly { key: keyof ClearDataSelection; label: string; hint?: string }[] {
  return [
    { key: 'collectedData', label: m.storageClearCollected, hint: m.storageClearCollectedHint },
    { key: 'browserHistory', label: m.storageClearHistory },
    { key: 'downloadRecords', label: m.storageClearDownloads, hint: m.storageClearDownloadsHint },
    { key: 'websiteLogins', label: m.storageClearLogins, hint: m.storageClearLoginsHint },
    { key: 'localBackups', label: m.storageClearBackups, hint: m.storageClearBackupsHint },
  ]
}

/** Start with nothing selected; opening the dialog grants no deletion authority. */
export const EMPTY_CLEAR_SELECTION: ClearDataSelection = { collectedData: false, browserHistory: false, downloadRecords: false, websiteLogins: false, localBackups: false }

/** Require at least one explicit category before invoking the native command. */
export function hasClearSelection(selection: ClearDataSelection): boolean {
  return selection.collectedData || selection.browserHistory || selection.downloadRecords || selection.websiteLogins || selection.localBackups
}
