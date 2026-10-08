/** Reader sizing toggle reflects the current pane state, not a fixed maximize glyph. */
import { Maximize2, Minimize2 } from 'lucide-react'
import type { Locale } from './i18n'

/** Expose both the current pressed state and the action available next. */
export function ReaderExpandButton({ expanded, locale, onToggle }: {
  readonly expanded: boolean
  readonly locale: Locale
  readonly onToggle: () => void
}) {
  const label = expanded
    ? locale === 'zh' ? '恢复分栏' : 'Split view'
    : locale === 'zh' ? '放大' : 'Expand'
  const Glyph = expanded ? Minimize2 : Maximize2
  return <button className="browser-icon-button reader-expand" type="button"
    title={label} aria-label={label} aria-pressed={expanded} onClick={onToggle}>
    <Glyph aria-hidden="true" />
  </button>
}
