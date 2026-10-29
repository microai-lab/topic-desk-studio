/** Pure browser-chrome helpers kept outside React for deterministic regression tests. */

import type { BrowserAction, BrowserSettings } from './api'

/** Apply an already-successful native close. Only removing the final tab restores
 * the topic list; stale close notifications cannot hide a newly opened reader. */
export function browserTabClosePlan<T extends { readonly id: string }>(tabs: readonly T[], activeId: string, closedId: string) {
  const index = tabs.findIndex(tab => tab.id === closedId)
  if (index < 0) return undefined
  const remaining = tabs.filter(tab => tab.id !== closedId)
  return {
    remaining,
    activeId: activeId === closedId ? remaining[Math.min(index, remaining.length - 1)]?.id ?? '' : activeId,
    restoreTopics: remaining.length === 0,
  }
}

export type DownloadState = 'downloading' | 'complete' | 'cancelled' | 'failed'

/** Decode the native `state\npath` representation without trusting unknown states. */
export function parseDownloadDetail(detail: string): { state: DownloadState; path: string } {
  const newline = detail.indexOf('\n')
  const rawState = newline < 0 ? detail : detail.slice(0, newline)
  const state: DownloadState = ['downloading', 'complete', 'cancelled', 'failed'].includes(rawState)
    ? rawState as DownloadState
    : 'failed'
  return { state, path: newline < 0 ? '' : detail.slice(newline + 1) }
}

/** Match only the real Xiaohongshu domain, never a look-alike hostname. */
export function isXiaohongshuAddress(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase()
    return host === 'xiaohongshu.com' || host.endsWith('.xiaohongshu.com')
  } catch {
    return false
  }
}

/** Choose the next supported browser zoom while clamping at both ends. */
export function nextZoom(current: number, direction: number, levels: readonly number[]): number | undefined {
  return direction > 0
    ? levels.find((level) => level > current)
    : [...levels].reverse().find((level) => level < current)
}

/** Keep device-preview dimensions inside the native browser's supported range. */
export function clampDeviceDimension(value: number, minimum: number): number {
  if (!Number.isFinite(value)) return minimum
  return Math.max(minimum, Math.min(2560, value))
}

/** The same button restores an active translation or starts the saved target language. */
export function pageTranslationRequest(settings: BrowserSettings, translated = false): BrowserAction {
  return translated ? { kind: 'restorePageTranslation' } : { kind: 'translatePage', targetLanguage: settings.translationLanguage }
}

/** Mark loading before dispatch so a fast native completion cannot be overwritten afterward. */
export async function runBrowserNavigation(request: () => Promise<void>, started: () => void, failed: () => void): Promise<void> {
  started()
  try { await request() }
  catch (error) { failed(); throw error }
}

/** Keep failed native closes visible and retryable, while allowing other tabs
 * in a bulk close to finish. Removing the chrome first can orphan playing media. */
export async function runBrowserTabClose(ids: readonly string[], close: (id: string) => Promise<void>, closed: (id: string) => void, failed: (id: string, error: unknown) => void): Promise<void> {
  for (const id of new Set(ids)) {
    try { await close(id) }
    catch (error) { failed(id, error); continue }
    closed(id)
  }
}

/** Reapply native bounds after backgrounding/sleep, even when DOM size is unchanged.
 * Recovery uses events only; it never starts a frontend polling timer. */
export function observeReaderRecovery(host: Window, page: Document, recover: () => void): () => void {
  const resume = () => { if (!page.hidden) recover() }
  host.addEventListener('focus', resume)
  host.addEventListener('pageshow', resume)
  page.addEventListener('visibilitychange', resume)
  return () => {
    host.removeEventListener('focus', resume)
    host.removeEventListener('pageshow', resume)
    page.removeEventListener('visibilitychange', resume)
  }
}

/** Recreate temporary native views only after clearing succeeds. This also
 * prevents saved-cookie snapshots from outliving the cleared browser stores. */
export async function runBrowserCookieClear(clear: () => Promise<unknown>, reset: () => void, reopen: () => Promise<void>): Promise<void> {
  await clear()
  reset()
  await reopen()
}
