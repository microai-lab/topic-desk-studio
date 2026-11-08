/** Enabled source rows stay visible; disabled rows remain available in a collapsed recovery group. */
import type { ReactNode } from 'react'
import type { SourceConfiguration } from './types'
import type { Messages } from './i18n'
import { sourceSettingsGroups, sourceSettingsOverview } from './sourceSettingsState'

/** Source counts appear before bulk management so users see the current scope first. */
export function SourceSettingsOverview({ sources, m }: { sources: readonly SourceConfiguration[]; m: Messages }) {
  const overview = sourceSettingsOverview(sources)
  return <dl className="source-overview">
    <div className="source-overview-active"><dt>{m.sourceEnabledGroup}</dt><dd>{overview.enabled}</dd></div>
    <div><dt>{m.regionDomestic}</dt><dd>{overview.domestic}</dd></div>
    <div><dt>{m.regionIntl}</dt><dd>{overview.international}</dd></div>
  </dl>
}

/** Partition display only: source switches and manual order stay authoritative in SQLite. */
export function SourceSettingsGroups({ sources, busy, m, renderSource }: {
  sources: readonly SourceConfiguration[]; busy: boolean; m: Messages; renderSource: (source: SourceConfiguration) => ReactNode
}) {
  const groups = sourceSettingsGroups(sources)
  return <div className="source-list" aria-busy={busy}>
    <div className="source-group-heading"><h3>{m.sourceEnabledGroup}</h3><span>{groups.enabled.length}</span></div>
    {groups.enabled.length === 0 ? <p className="source-group-empty">{m.sourceNoEnabled}</p> : null}
    {groups.enabled.map(renderSource)}
    {groups.disabled.length > 0 ? <details className="disabled-source-group">
      <summary>{m.sourceDisabledGroup} ({groups.disabled.length})</summary>
      <p>{m.sourceDisabledGroupHelp}</p>
      {groups.disabled.map(renderSource)}
    </details> : null}
  </div>
}
