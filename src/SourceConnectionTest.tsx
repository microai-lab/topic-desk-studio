/** Source test controls and inline outcomes stay independent of row dragging and collection health. */
import { CheckCircle2, CircleAlert, Clock3, LoaderCircle, PlugZap } from 'lucide-react'
import type { Locale } from './i18n'
import { sourceConnectionPresentation } from './sourceConnectionPresentation'
import type { SourceConnectionTestState } from './sourceConnectionPresentation'
/** Each batch diagnoses routes afresh for every saved source, including disabled rows. */
export function SourceConnectionBatchTestButton({ locale, running, disabled = false, completed, total, onTest }: {
  readonly locale: Locale
  readonly running: boolean
  readonly disabled?: boolean | undefined
  readonly completed: number
  readonly total: number
  readonly onTest: () => void
}) {
  const unavailable = running || disabled
  const label = running ? `${locale === 'zh' ? '测试中' : 'Testing'} ${completed}/${total}` : (locale === 'zh' ? '连通性测试' : 'Test connectivity')
  const Glyph = running ? LoaderCircle : PlugZap
  return <button type="button" className={`source-batch-test${running ? ' is-testing' : ''}`} disabled={unavailable} aria-busy={running}
    title={locale === 'zh' ? '重新自动判断所有来源的连接方式，包含已停用来源' : 'Recheck routes automatically for all sources, including disabled ones'}
    onClick={() => { if (!unavailable) onTest() }}>
    <Glyph aria-hidden="true" />{label}
  </button>
}

/** Pending requests are disabled and cannot accidentally begin a source reorder. */
export function SourceConnectionTestButton({ state, locale, disabled = false, onTest }: {
  readonly state?: SourceConnectionTestState | undefined
  readonly locale: Locale
  readonly disabled?: boolean | undefined
  readonly onTest: () => void
}) {
  const pending = state?.phase === 'testing'
  const queued = state?.phase === 'queued'
  const unavailable = disabled || pending || queued
  const label = queued ? (locale === 'zh' ? '等待测试' : 'Queued')
    : pending ? (locale === 'zh' ? '测试中…' : 'Testing…') : (locale === 'zh' ? '测试连通性' : 'Test connection')
  const help = locale === 'zh' ? '本次测试重新自动判断直连或已保存的代理，不修改采集设置' : 'Recheck Direct or the saved proxy for this test without changing collection settings'
  const Glyph = queued ? Clock3 : pending ? LoaderCircle : PlugZap
  return <button type="button" className={`source-icon-action source-connection-button${pending ? ' is-testing' : ''}`} disabled={unavailable}
    aria-label={label} aria-busy={pending || queued} title={pending || queued ? label : help}
    onPointerDown={event => event.stopPropagation()}
    onClick={event => { event.stopPropagation(); if (!unavailable) onTest() }}>
    <Glyph aria-hidden="true" />
  </button>
}

/** Error detail wraps in the row so long native explanations remain available without hovering. */
export function SourceConnectionTestResultView({ state, locale }: {
  readonly state?: SourceConnectionTestState | undefined
  readonly locale: Locale
}) {
  const content = sourceConnectionPresentation(state, locale)
  if (content === undefined) return null
  const Glyph = state?.phase === 'queued' ? Clock3 : content.tone === 'success' ? CheckCircle2 : content.tone === 'testing' ? LoaderCircle : CircleAlert
  return <span className={`source-connection-result ${content.tone}`} role="status">
    <span className="source-connection-summary"><Glyph aria-hidden="true" /><strong>{content.label}</strong>{content.detail ? <span> · {content.detail}</span> : null}</span>
    {content.error ? <span className="source-connection-error" title={content.error}>{content.error}</span> : null}

  </span>
}
