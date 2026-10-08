/** Pure browser-chrome helpers kept outside React for deterministic regression tests. */

import type { BrowserAction, BrowserSettings } from './api'

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

/** Build a direct page-translation request from the saved browser preference. */
export function pageTranslationRequest(settings: BrowserSettings): BrowserAction {
  return { kind: 'translatePage', targetLanguage: settings.translationLanguage }
}

/** Mark loading before dispatch so a fast native completion cannot be overwritten afterward. */
export async function runBrowserNavigation(request: () => Promise<void>, started: () => void, failed: () => void): Promise<void> {
  started()
  try { await request() }
  catch (error) { failed(); throw error }
}
