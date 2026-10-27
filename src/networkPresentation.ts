/** Proxy presets and draft selection stay separate from native network access. */
export const DEFAULT_PROXY_URL = 'http://127.0.0.1:7897'
export const PROXY_PRESETS = [DEFAULT_PROXY_URL, 'http://127.0.0.1:1082'] as const

/** Recognize saved presets without replacing custom endpoints or an explicit direct route. */
export function proxyPresetSelection(address: string): string {
  const normalized = address.trim().replace(/\/$/, '')
  if (!normalized) return 'direct'
  return PROXY_PRESETS.find(preset => preset === normalized) ?? 'custom'
}

/** Only known selections can replace the editable draft; saving remains explicit. */
export function selectProxyPreset(selection: string, current: string): string {
  if (selection === 'direct') return ''
  return PROXY_PRESETS.find(preset => preset === selection) ?? current
}

/** Keyboard navigation changes only the highlighted choice; Enter commits it.
 * Escape/Tab close without replacing a custom draft or trapping focus. */
export function proxyComboboxKey(state: { open: boolean; active: number }, key: string, count: number): { open: boolean; active: number; selected?: number } | undefined {
  if (!Number.isInteger(count) || count <= 0) return undefined
  if (key === 'Escape' || key === 'Tab') return state.open ? { open: false, active: -1 } : undefined
  if (key === 'Enter') return state.open && state.active >= 0 && state.active < count
    ? { open: false, active: -1, selected: state.active } : undefined
  if (key === 'ArrowDown') return { open: true, active: state.open ? (state.active + 1) % count : 0 }
  if (key === 'ArrowUp') return { open: true, active: !state.open || state.active <= 0 ? count - 1 : state.active - 1 }
  return undefined
}
