/** Keep frequent source actions prominent and file/reset maintenance in a separate disclosure. */
import type { ReactNode } from 'react'
import { ChevronDown } from 'lucide-react'
import type { Messages } from './i18n'

/** Layout slots preserve native action handlers while reducing toolbar crowding. */
export function SourceSettingsToolbar({ m, primary, regional, maintenance }: {
  m: Messages; primary: ReactNode; regional: ReactNode; maintenance: ReactNode
}) {
  return <>
    <header className="settings-page-header source-settings-header">
      <div><h2>{m.tabSources}</h2><p>{m.settingsSourcesDesc}</p></div>
      <div className="source-batch-actions source-primary-actions">{primary}</div>
    </header>
    <div className="source-management-toolbar">
      <div className="source-batch-actions source-regional-actions"><span className="source-toolbar-label">{m.sourceBulkActions}</span>{regional}</div>
      <details className="source-maintenance-menu">
        <summary>{m.sourceMoreActions}<ChevronDown aria-hidden="true" /></summary>
        <div className="source-batch-actions source-maintenance-actions">{maintenance}</div>
      </details>
    </div>
  </>
}
