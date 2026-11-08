/** Keep source counts above a flat, wrapping row of regional and maintenance actions. */
import type { ReactNode } from 'react'
import type { Messages } from './i18n'

/** Layout slots preserve native action handlers while reducing toolbar crowding. */
export function SourceSettingsToolbar({ m, overview, primary, regional, maintenance, restore }: {
  m: Messages; overview: ReactNode; primary: ReactNode; regional: ReactNode; maintenance: ReactNode; restore: ReactNode
}) {
  return <>
    <header className="settings-page-header source-settings-header">
      <div><h2>{m.tabSources}</h2><p>{m.settingsSourcesDesc}</p></div>
      <div className="source-batch-actions source-primary-actions">{primary}</div>
    </header>
    {overview}
    <div className="source-management-toolbar source-batch-actions">
      {maintenance}
      {regional}
      {restore}
    </div>
  </>
}
