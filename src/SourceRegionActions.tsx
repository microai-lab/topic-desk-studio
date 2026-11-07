/** Regional source switches reflect saved configuration and never enable sources implicitly. */
import { messages, type Locale } from './i18n'
import type { SourceConfiguration, SourceRegion } from './types'

/** Render region-specific bulk disable actions, suppressing empty and busy operations. */
export function SourceRegionActions({ sources, locale, busy, onDisable }: {
  sources: readonly SourceConfiguration[]; locale: Locale; busy: boolean; onDisable: (region: SourceRegion) => void
}) {
  const m = messages[locale]
  return <>{(['domestic', 'international'] as const).map(region => {
    const disabled = busy || !sources.some(source => source.region === region && source.enabled)
    return <button key={region} type="button" disabled={disabled} onClick={() => { if (!disabled) onDisable(region) }}>
      {region === 'domestic' ? m.sourceDisableDomestic : m.sourceDisableInternational}
    </button>
  })}</>
}
