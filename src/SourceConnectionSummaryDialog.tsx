/** Completed source diagnostics use a focused, dismissible summary instead of a persistent settings row. */
import { useEffect, useId, useRef } from 'react'
import type { Locale } from './i18n'
import type { SourceConnectionBatchProgress } from './sourceConnectionPresentation'

/** Keep every outcome distinct; empty feeds and browser checks are never counted as passed. */
export function sourceConnectionSummary(progress: SourceConnectionBatchProgress, locale: Locale) {
  const zh = locale === 'zh'
  return {
    title: zh ? '连通性测试完成' : 'Connectivity test complete',
    description: zh ? `已完成 ${progress.completed}/${progress.total} 个数据来源` : `${progress.completed}/${progress.total} sources tested`,
    closeLabel: zh ? '知道了' : 'Done',
    items: [
      { key: 'success', label: zh ? '正常' : 'Passed', count: progress.success },
      { key: 'failed', label: zh ? '失败' : 'Failed', count: progress.failed },
      { key: 'empty', label: zh ? '无数据' : 'No items', count: progress.empty },
      { key: 'manual', label: zh ? '需网页测试' : 'Browser check', count: progress.manual },
    ],
  }
}

/** Native modality traps focus; cleanup prefers the previous control and otherwise the batch trigger. */
export function activateSourceConnectionSummary(
  dialog: Pick<HTMLDialogElement, 'open' | 'showModal' | 'close'> | null,
  done: Pick<HTMLElement, 'focus'> | null,
  previous: Pick<HTMLElement, 'isConnected' | 'focus'> | null,
  fallback?: () => Pick<HTMLElement, 'focus'> | null,
): () => void {
  if (dialog === null) return () => {}
  dialog.showModal()
  done?.focus()
  return () => {
    if (dialog.open) dialog.close()
    if (previous?.isConnected) previous.focus()
    else fallback?.()?.focus()
  }
}

/** Dismissal only hides the completed summary; it never restarts collection or testing. */
export function SourceConnectionSummaryDialog({ progress, locale, onClose }: {
  readonly progress: SourceConnectionBatchProgress
  readonly locale: Locale
  readonly onClose: () => void
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  const done = useRef<HTMLButtonElement>(null)
  const id = useId()
  const summary = sourceConnectionSummary(progress, locale)
  useEffect(() => {
    // Disabling the batch trigger can move focus to body before the test finishes.
    const previous = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null
    return activateSourceConnectionSummary(dialog.current, done.current, previous, () => document.querySelector<HTMLButtonElement>('.source-batch-test'))
  }, [])
  return <dialog ref={dialog} className="storage-confirmation source-connection-summary-dialog" aria-labelledby={`${id}-title`} aria-describedby={`${id}-description`}
    onCancel={event => { event.preventDefault(); onClose() }}>
    <h2 id={`${id}-title`}>{summary.title}</h2>
    <p id={`${id}-description`}>{summary.description}</p>
    <dl className="source-connection-summary-grid">
      {summary.items.map(item => <div className={`source-connection-summary-stat ${item.key}`} key={item.key}>
        <dt>{item.label}</dt><dd>{item.count}</dd>
      </div>)}
    </dl>
    <div className="storage-confirmation-actions">
      <button ref={done} type="button" className="primary" onClick={onClose}>{summary.closeLabel}</button>
    </div>
  </dialog>
}
