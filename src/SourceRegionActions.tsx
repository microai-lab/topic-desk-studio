/** Regional source switches derive their next action from saved configuration. */
import { Globe2, MapPin } from 'lucide-react'
import { messages, type Locale } from './i18n'
import type { SourceConfiguration, SourceRegion } from './types'

/** Mixed regions close first; fully disabled regions open together on the next click. */
export function SourceRegionActions({ sources, locale, busy, onSetEnabled }: {
  sources: readonly SourceConfiguration[]; locale: Locale; busy: boolean; onSetEnabled: (region: SourceRegion, enabled: boolean) => void
}) {
  const m = messages[locale]
  return <>{(['domestic', 'international'] as const).map(region => {
    const regional = sources.filter(source => source.region === region)
    const enabled = regional.some(source => source.enabled)
    const disabled = busy || regional.length === 0
    const hint = region === 'domestic'
      ? (enabled ? m.sourceDisableDomesticHint : m.sourceEnableDomesticHint)
      : (enabled ? m.sourceDisableInternationalHint : m.sourceEnableInternationalHint)
    const Icon = region === 'domestic' ? MapPin : Globe2
    return <button key={region} className="source-region-action" type="button" title={hint} aria-label={hint} aria-pressed={enabled} disabled={disabled} onClick={() => { if (!disabled) onSetEnabled(region, !enabled) }}>
      <Icon size={14} strokeWidth={1.75} aria-hidden="true" />
      {region === 'domestic' ? m.sourceDisableDomestic : m.sourceDisableInternational}
    </button>
  })}</>
}
