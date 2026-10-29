/** Read-only source test identity, request lifecycle, and localized result presentation. */
import type { Locale } from './i18n'
import type { SourceConfiguration, SourceConnectionTestResult } from './types'

/** A configuration fingerprint keeps results from an old URL, parser, or network route hidden. */
export type SourceConnectionTestState = { readonly key: string } & (
  { readonly phase: 'queued' }
  | { readonly phase: 'testing' }
  | { readonly phase: 'done'; readonly result: SourceConnectionTestResult }
  | { readonly phase: 'error'; readonly error: string }
)

/** A completed current-configuration diagnostic overrides older collection health only in the UI.
 * Empty and browser-only results must not appear healthy; pending work retains collection health. */
export function sourceHealthTone(state: SourceConnectionTestState | undefined, collectionStatus?: string | null): 'healthy' | 'failed' | '' {
  if (state?.phase === 'done') {
    return state.result.status === 'success' ? 'healthy' : state.result.status === 'failed' ? 'failed' : ''
  }
  if (state?.phase === 'error') return 'failed'
  return collectionStatus === 'succeeded' ? 'healthy' : collectionStatus === 'failed' ? 'failed' : ''
}

/** Batch counters distinguish unavailable, empty, and browser-only sources from success. */
export interface SourceConnectionBatchProgress {
  readonly total: number
  readonly completed: number
  readonly success: number
  readonly empty: number
  readonly failed: number
  readonly manual: number
}

/** Match the native diagnostic worker cap; the frontend calls only typed commands. */
export const SOURCE_CONNECTION_TEST_CONCURRENCY = 3

/** Enable/disable and display names do not alter the endpoint being diagnosed. */
export function sourceConnectionKey(source: SourceConfiguration, networkRevision: number): string {
  const parser = Object.entries(source.parserConfig)
    .filter(([, value]) => value !== undefined)
    .sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)
  return JSON.stringify([source.code, source.endpointUrl, source.parserType, parser, source.proxyMode, networkRevision])
}

/** A late response must never describe newly saved source or proxy settings. */
export function matchingSourceConnectionState(state: SourceConnectionTestState | undefined, key: string): SourceConnectionTestState | undefined {
  return state?.key === key ? state : undefined
}

/** Keep validation and rejection behavior testable without starting a WebView or network request. */
export async function runSourceConnectionTest(code: string, key: string, request: (code: string) => Promise<SourceConnectionTestResult>, update: (state: SourceConnectionTestState) => void): Promise<void> {
  if (!/^[a-z0-9-]{2,48}$/.test(code) || !key.trim()) {
    update({ key, phase: 'error', error: 'invalid-source-test-request' })
    return
  }
  update({ key, phase: 'testing' })
  try {
    const result = await request(code)
    if (result.code !== code || !['success', 'empty', 'failed', 'manual'].includes(result.status)
      || !Number.isFinite(result.elapsedMs) || result.elapsedMs < 0
      || !Number.isSafeInteger(result.itemCount) || result.itemCount < 0
      || ![null, 'direct', 'proxy'].includes(result.route)
      || (result.fallbackFrom !== undefined && ![null, 'direct', 'proxy'].includes(result.fallbackFrom))) {
      throw new Error('invalid-source-test-result')
    }
    update({ key, phase: 'done', result })
  } catch (reason) {
    update({ key, phase: 'error', error: reason instanceof Error ? reason.message : String(reason) })
  }
}

/** Snapshot each source once, including disabled sources. Rejections finish only
 * their own job; at most three native commands run while the rest stay queued. */
export async function runSourceConnectionBatch(
  sources: readonly SourceConfiguration[],
  networkRevision: number,
  request: (code: string) => Promise<SourceConnectionTestResult>,
  update: (code: string, state: SourceConnectionTestState) => void,
  report: (progress: SourceConnectionBatchProgress) => void,
): Promise<SourceConnectionBatchProgress> {
  const seen = new Set<string>()
  const jobs = sources.filter(source => {
    if (seen.has(source.code)) return false
    seen.add(source.code)
    return true
  }).map(source => ({ code: source.code, key: sourceConnectionKey(source, networkRevision) }))
  let progress: SourceConnectionBatchProgress = { total: jobs.length, completed: 0, success: 0, empty: 0, failed: 0, manual: 0 }
  report(progress)
  for (const job of jobs) update(job.code, { key: job.key, phase: 'queued' })
  let cursor = 0
  await Promise.all(Array.from({ length: Math.min(SOURCE_CONNECTION_TEST_CONCURRENCY, jobs.length) }, async () => {
    while (cursor < jobs.length) {
      const job = jobs[cursor++]!
      let outcome: SourceConnectionTestResult['status'] = 'failed'
      await runSourceConnectionTest(job.code, job.key, request, state => {
        if (state.phase === 'done') outcome = state.result.status
        update(job.code, state)
      })
      progress = { ...progress, completed: progress.completed + 1, [outcome]: progress[outcome] + 1 }
      report(progress)
    }
  }))
  return progress
}

/** Only current test state is presented; collection history remains a separate indicator. */
export function sourceConnectionPresentation(state: SourceConnectionTestState | undefined, locale: Locale): {
  readonly tone: 'queued' | 'testing' | 'success' | 'warning' | 'failed'
  readonly label: string
  readonly detail: string
  readonly error: string | null
} | undefined {
  if (state === undefined) return undefined
  const zh = locale === 'zh'
  if (state.phase === 'queued') return { tone: 'queued', label: zh ? '等待测试' : 'Queued', detail: '', error: null }
  if (state.phase === 'testing') return { tone: 'testing', label: zh ? '测试中…' : 'Testing…', detail: '', error: null }
  if (state.phase === 'error') {
    const error = state.error === 'invalid-source-test-request' ? (zh ? '数据源测试参数无效' : 'Invalid source test request.')
      : state.error === 'invalid-source-test-result' ? (zh ? '数据源测试结果无效，请重试' : 'Invalid source test result. Please retry.')
      : state.error || (zh ? '无法完成测试，请重试' : 'Could not complete the test. Please retry.')
    return { tone: 'failed', label: zh ? '测试失败' : 'Test failed', detail: '', error }
  }
  const { result } = state
  const elapsed = result.elapsedMs < 1000 ? `${Math.round(result.elapsedMs)} ms` : `${(result.elapsedMs / 1000).toFixed(1)} ${zh ? '秒' : 's'}`
  const route = result.route === 'proxy' ? (zh ? '代理' : 'Proxy') : result.route === 'direct' ? (zh ? '直连' : 'Direct') : null
  // Only actual native route changes earn the fallback label; old results have no fallback metadata.
  const previousRoute = result.fallbackFrom === 'proxy' ? (zh ? '代理' : 'Proxy') : result.fallbackFrom === 'direct' ? (zh ? '直连' : 'Direct') : null
  const routeDetail = previousRoute && route && result.fallbackFrom !== result.route
    ? `${zh ? '自动切换：' : 'Automatically switched: '}${previousRoute} → ${route}` : route
  const count = zh ? `${result.itemCount} 条` : `${result.itemCount} ${result.itemCount === 1 ? 'item' : 'items'}`
  const detail = result.status === 'manual' ? '' : [result.status === 'success' || result.status === 'empty' ? count : null, elapsed, routeDetail].filter(Boolean).join(' · ')
  switch (result.status) {
    case 'success': return { tone: 'success', label: zh ? '测试正常' : 'Test passed', detail, error: null }
    case 'empty': return { tone: 'warning', label: zh ? '连接正常，暂无数据' : 'Connected, no items', detail, error: null }
    case 'manual': return { tone: 'warning', label: zh ? '需在网页中测试' : 'Browser check required', detail, error: result.error }
    case 'failed': return { tone: 'failed', label: zh ? '测试失败' : 'Test failed', detail, error: result.error || (zh ? '无法获取并解析数据源' : 'Could not fetch and parse the source.') }
  }
}
