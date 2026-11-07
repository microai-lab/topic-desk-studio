/** Keep persisted source switches authoritative across settings reloads and editor saves. */
import type { SaveSourceConfiguration, SourceConfiguration } from './types'

/** Enabled sources retain manual ordering and are the only discovery choices. */
export function enabledSources(sources: readonly SourceConfiguration[]): SourceConfiguration[] {
  return sources.filter(source => source.enabled)
}

/** Group presentation without rewriting the user's persisted order or switches. */
export function sourceSettingsGroups(sources: readonly SourceConfiguration[]) {
  return { enabled: enabledSources(sources), disabled: sources.filter(source => !source.enabled) }
}

/** Overview counts describe enabled collection scope, excluding disabled regional sources. */
export function sourceSettingsOverview(sources: readonly SourceConfiguration[]) {
  const enabled = enabledSources(sources)
  return { enabled: enabled.length, domestic: enabled.filter(source => source.region === 'domestic').length,
    international: enabled.filter(source => source.region === 'international').length }
}

/** Discard reads started before a switch write or a newer panel opening. */
export function createSourceSettingsReader() {
  let revision = 0
  return {
    invalidate() { revision += 1 },
    async refresh(load: () => Promise<SourceConfiguration[]>, publish: (sources: SourceConfiguration[]) => void) {
      const request = ++revision
      const sources = await load()
      if (request === revision) publish(sources)
    },
  }
}

/** An editor opened before a toggle must never restore its stale enabled flag. */
export function sourceDraftForSave(draft: SaveSourceConfiguration, sources: readonly SourceConfiguration[]): SaveSourceConfiguration {
  const saved = sources.find(source => source.code === draft.code)
  return saved ? { ...draft, enabled: saved.enabled } : draft
}
