/** Shared topic-pane collapse/restore control for list and reader headers. */
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react'
import type { Locale } from './i18n'

/** Keep the icon-only restore control accessible when the topic pane is out of view. */
export function ReaderExpandButton({ expanded, locale, onToggle }: {
  readonly expanded: boolean
  readonly locale: Locale
  readonly onToggle: () => void
}) {
  const label = expanded
    ? locale === 'zh' ? '展开选题' : 'Show topics'
    : locale === 'zh' ? '收起选题' : 'Hide topics'
  const Glyph = expanded ? PanelLeftOpen : PanelLeftClose
  return <button className="browser-icon-button reader-expand" type="button"
    title={label} aria-label={label} aria-pressed={expanded}
    aria-controls="topic-list-pane" aria-expanded={!expanded}
    data-topic-toggle={expanded ? 'restore' : 'collapse'} onClick={onToggle}>
    <Glyph aria-hidden="true" />
  </button>
}
