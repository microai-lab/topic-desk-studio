/** Visible media/mute control stays independent of tab activation and dragging. */
import { Volume2, VolumeX } from 'lucide-react'
import type { Locale } from './i18n'

/** A muted tab keeps its restore control even after playback has ended. */
export function TabMediaButton({ playing, muted, locale, onToggle }: {
  readonly playing: boolean
  readonly muted: boolean
  readonly locale: Locale
  readonly onToggle: () => void
}) {
  if (!playing && !muted) return null
  const label = muted
    ? locale === 'zh' ? '取消网页静音' : 'Unmute tab'
    : locale === 'zh' ? '正在播放媒体，点击静音' : 'Media playing — mute tab'
  const Glyph = muted ? VolumeX : Volume2
  return <button type="button" className={`browser-tab-media${muted ? ' is-muted' : ''}`}
    title={label} aria-label={label} aria-pressed={muted}
    onPointerDown={event => event.stopPropagation()}
    onClick={event => { event.stopPropagation(); onToggle() }}>
    <Glyph aria-hidden="true" />
  </button>
}
