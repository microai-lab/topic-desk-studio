/** Deterministic coverage for source test identity, native boundaries, and diagnostic outcomes. */
import { describe, expect, it, vi } from 'vitest'
import { matchingSourceConnectionState, runSourceConnectionBatch, runSourceConnectionTest, sourceConnectionKey, sourceConnectionPresentation, sourceHealthTone } from './sourceConnectionPresentation'
import type { SourceConnectionTestState } from './sourceConnectionPresentation'
import type { SourceConfiguration, SourceConnectionTestResult } from './types'

const source: SourceConfiguration = {
  code: 'example-feed', displayName: 'Example', homeUrl: 'https://example.com', endpointUrl: 'https://example.com/feed',
  parserType: 'rss', parserConfig: {}, proxyMode: 'auto', region: 'international', category: 'technology', enabled: true, builtIn: false,
}
const result: SourceConnectionTestResult = { code: source.code, status: 'success', elapsedMs: 1200, itemCount: 4, error: null, route: 'proxy' }

describe('source connection batch queue', () => {
  it('tests disabled sources too, queues every unique source once, and separates all outcomes', async () => {
    const sources = ['passed', 'empty', 'blocked', 'browser', 'rejected'].map((code, index) => ({ ...source, code, enabled: index === 0 }))
    const updates: [string, SourceConnectionTestState][] = []
    const request = vi.fn(async (code: string) => {
      if (code === 'rejected') throw new Error('timed out')
      const status = { passed: 'success', empty: 'empty', blocked: 'failed', browser: 'manual' }[code] as SourceConnectionTestResult['status']
      return { ...result, code, status, itemCount: status === 'success' ? 4 : 0 }
    })
    const report = vi.fn()
    const summary = await runSourceConnectionBatch([...sources, sources[0]!], 2, request, (code, state) => updates.push([code, state]), report)
    expect(request.mock.calls.map(([code]) => code).sort()).toEqual(sources.map(item => item.code).sort())
    expect(updates.slice(0, sources.length).map(([code, state]) => [code, state.phase])).toEqual(sources.map(item => [item.code, 'queued']))
    expect(summary).toEqual({ total: 5, completed: 5, success: 1, empty: 1, failed: 2, manual: 1 })
    expect(report.mock.calls.map(([progress]) => progress.completed)).toEqual([0, 1, 2, 3, 4, 5])
    expect(updates.filter(([, state]) => state.phase === 'error')).toEqual([['rejected', { key: sourceConnectionKey(sources[4]!, 2), phase: 'error', error: 'timed out' }]])
  })

  it('keeps at most three native requests active and continues the queue after a rejection', async () => {
    const sources = Array.from({ length: 7 }, (_, index) => ({ ...source, code: `source-${index}` }))
    const pending = new Map<string, { resolve: (result: SourceConnectionTestResult) => void; reject: (error: Error) => void }>()
    let active = 0, peak = 0
    const request = vi.fn((code: string) => {
      peak = Math.max(peak, ++active)
      return new Promise<SourceConnectionTestResult>((resolve, reject) => pending.set(code, { resolve, reject })).finally(() => { active -= 1 })
    })
    const batch = runSourceConnectionBatch(sources, 0, request, vi.fn(), vi.fn())
    expect(request).toHaveBeenCalledTimes(3)
    for (let index = 0; index < sources.length; index++) {
      const code = sources[index]!.code
      expect(pending.has(code)).toBe(true)
      if (index === 0) pending.get(code)!.reject(new Error('source unavailable'))
      else pending.get(code)!.resolve({ ...result, code })
      // Drain promise/finally and worker continuations without real timers.
      for (let step = 0; step < 8; step++) await Promise.resolve()
      expect(active).toBeLessThanOrEqual(3)
    }
    expect(await batch).toMatchObject({ total: 7, completed: 7, failed: 1, success: 6 })
    expect(peak).toBe(3)
    expect(active).toBe(0)
  })

  it('finishes empty selections without requests and counts invalid source identifiers as failures', async () => {
    const request = vi.fn(async () => result), report = vi.fn()
    expect(await runSourceConnectionBatch([], 0, request, vi.fn(), report)).toEqual({ total: 0, completed: 0, success: 0, empty: 0, failed: 0, manual: 0 })
    expect(request).not.toHaveBeenCalled()
    expect(report).toHaveBeenCalledTimes(1)
    expect(await runSourceConnectionBatch([{ ...source, code: 'BAD' }], 0, request, vi.fn(), vi.fn())).toMatchObject({ total: 1, completed: 1, failed: 1 })
    expect(request).not.toHaveBeenCalled()
  })

  it('snapshots queued configuration keys before the first request completes', async () => {
    const sources = Array.from({ length: 4 }, (_, index) => ({ ...source, code: `source-${index}` }))
    const originalKey = sourceConnectionKey(sources[3]!, 4)
    const update = vi.fn()
    await runSourceConnectionBatch(sources, 4, async code => {
      Object.assign(sources[3]!, { endpointUrl: 'https://example.com/changed' })
      return { ...result, code }
    }, update, vi.fn())
    expect(update).toHaveBeenCalledWith('source-3', { key: originalKey, phase: 'done', result: { ...result, code: 'source-3' } })
  })

})

describe('source connection test identity', () => {
  it('invalidates results for endpoint, parser, route, or saved network changes', () => {
    const key = sourceConnectionKey(source, 0)
    for (const changed of [
      { ...source, endpointUrl: 'https://example.com/new-feed' },
      { ...source, parserType: 'json' as const },
      { ...source, parserConfig: { titlePath: 'headline' } },
      { ...source, proxyMode: 'direct' as const },
      { ...source, code: 'another-feed' },
    ]) expect(sourceConnectionKey(changed, 0)).not.toBe(key)
    expect(sourceConnectionKey(source, 1)).not.toBe(key)
  })
  it('does not invalidate a diagnostic when a source is renamed or disabled', () => {
    expect(sourceConnectionKey({ ...source, displayName: 'Renamed', enabled: false }, 0)).toBe(sourceConnectionKey(source, 0))
    expect(sourceConnectionKey({ ...source, parserConfig: { titlePath: 'title', urlPath: 'url' } }, 0))
      .toBe(sourceConnectionKey({ ...source, parserConfig: { urlPath: 'url', titlePath: 'title' } }, 0))
  })
  it('hides a delayed response after source or network settings have changed', () => {
    const state: SourceConnectionTestState = { key: 'before-save', phase: 'done', result }
    expect(matchingSourceConnectionState(state, 'after-save')).toBeUndefined()
    expect(matchingSourceConnectionState(undefined, 'after-save')).toBeUndefined()
    expect(matchingSourceConnectionState(state, 'before-save')).toBe(state)
  })
})

describe('source connection request lifecycle', () => {
  it('announces pending state before requesting and preserves the returned diagnostic', async () => {
    const states: SourceConnectionTestState[] = []
    const request = vi.fn(async (code: string) => {
      expect(code).toBe(source.code)
      expect(states).toEqual([{ key: 'saved', phase: 'testing' }])
      return result
    })
    await runSourceConnectionTest(source.code, 'saved', request, state => states.push(state))
    expect(states).toEqual([{ key: 'saved', phase: 'testing' }, { key: 'saved', phase: 'done', result }])
  })
  it('keeps empty, manual, and failed results distinct from request rejection', async () => {
    for (const status of ['empty', 'manual', 'failed'] as const) {
      const update = vi.fn()
      const diagnostic = { ...result, status, itemCount: 0, error: status === 'failed' ? 'HTTP 403' : null }
      await runSourceConnectionTest(source.code, 'saved', async () => diagnostic, update)
      expect(update).toHaveBeenLastCalledWith({ key: 'saved', phase: 'done', result: diagnostic })
    }
  })
  it('makes native failures visible and keeps them retryable', async () => {
    const update = vi.fn()
    await runSourceConnectionTest(source.code, 'saved', () => Promise.reject(new Error('Request timed out')), update)
    expect(update).toHaveBeenLastCalledWith({ key: 'saved', phase: 'error', error: 'Request timed out' })
    await runSourceConnectionTest(source.code, 'saved', async () => result, update)
    expect(update).toHaveBeenLastCalledWith({ key: 'saved', phase: 'done', result })
  })
  it('rejects invalid identifiers and missing configuration identity without invoking native code', async () => {
    for (const [code, key] of [['', 'saved'], ['x', 'saved'], ['x'.repeat(49), 'saved'], ['https://example.com', 'saved'], ['UPPER', 'saved'], [source.code, ' ']]) {
      const request = vi.fn(), update = vi.fn()
      await runSourceConnectionTest(code!, key!, request, update)
      expect(request).not.toHaveBeenCalled()
      expect(update).toHaveBeenCalledExactlyOnceWith({ key, phase: 'error', error: 'invalid-source-test-request' })
    }
  })
  it('accepts code length boundaries and zero elapsed time or zero items', async () => {
    for (const code of ['ab', 'x'.repeat(48)]) {
      const diagnostic = { ...result, code, elapsedMs: 0, itemCount: 0, status: 'empty' as const }
      const update = vi.fn()
      await runSourceConnectionTest(code, 'saved', async () => diagnostic, update)
      expect(update).toHaveBeenLastCalledWith({ key: 'saved', phase: 'done', result: diagnostic })
    }
  })
  it('rejects mismatched source results and invalid native measurements', async () => {
    for (const diagnostic of [
      { ...result, code: 'another-feed' }, { ...result, elapsedMs: NaN }, { ...result, elapsedMs: -1 },
      { ...result, itemCount: -1 }, { ...result, itemCount: 1.5 },
    ]) {
      const update = vi.fn()
      await runSourceConnectionTest(source.code, 'saved', async () => diagnostic, update)
      expect(update).toHaveBeenLastCalledWith({ key: 'saved', phase: 'error', error: 'invalid-source-test-result' })
    }
  })
  it('accepts optional fallback metadata and preserves older native results without it', async () => {
    for (const diagnostic of [result, { ...result, fallbackFrom: null }, { ...result, fallbackFrom: 'direct' as const }]) {
      const update = vi.fn()
      await runSourceConnectionTest(source.code, 'saved', async () => diagnostic, update)
      expect(update).toHaveBeenLastCalledWith({ key: 'saved', phase: 'done', result: diagnostic })
    }
  })
  it('rejects invalid final routes or fallback routes from the native boundary', async () => {
    for (const malformed of [
      { ...result, route: 'auto' }, { ...result, route: undefined },
      { ...result, fallbackFrom: 'auto' }, { ...result, fallbackFrom: false },
    ]) {
      const update = vi.fn()
      await runSourceConnectionTest(source.code, 'saved', async () => malformed as unknown as SourceConnectionTestResult, update)
      expect(update).toHaveBeenLastCalledWith({ key: 'saved', phase: 'error', error: 'invalid-source-test-result' })
    }
  })
})

describe('source connection diagnostic copy', () => {
  for (const locale of ['zh', 'en'] as const) {
    it(`shows counts, duration, and actual route in ${locale}`, () => {
      const success = sourceConnectionPresentation({ key: 'saved', phase: 'done', result }, locale)!
      expect(success).toMatchObject({ tone: 'success', error: null })
      expect(success.detail).toContain(locale === 'zh' ? '4 条' : '4 items')
      expect(success.detail).toContain(locale === 'zh' ? '1.2 秒' : '1.2 s')
      expect(success.detail).toContain(locale === 'zh' ? '代理' : 'Proxy')
      const direct = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: { ...result, route: 'direct', elapsedMs: 9 } }, locale)!
      expect(direct.detail).toContain('9 ms')
      expect(direct.detail).toContain(locale === 'zh' ? '直连' : 'Direct')
    })
    it(`shows either automatic route change after success or failure in ${locale}`, () => {
      for (const status of ['success', 'failed'] as const) {
        const switchedToProxy = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: { ...result, status, fallbackFrom: 'direct' } }, locale)!
        const switchedToDirect = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: { ...result, status, route: 'direct', fallbackFrom: 'proxy' } }, locale)!
        expect(switchedToProxy.detail).toContain(locale === 'zh' ? '自动切换：直连 → 代理' : 'Automatically switched: Direct → Proxy')
        expect(switchedToDirect.detail).toContain(locale === 'zh' ? '自动切换：代理 → 直连' : 'Automatically switched: Proxy → Direct')
        expect(switchedToProxy.tone).toBe(status === 'success' ? 'success' : 'failed')
        expect(switchedToDirect.tone).toBe(status === 'success' ? 'success' : 'failed')
        expect(switchedToProxy.detail).toContain(locale === 'zh' ? '1.2 秒' : '1.2 s')
      }
    })
    it(`does not invent a route change for absent, null, or unchanged route metadata in ${locale}`, () => {
      for (const diagnostic of [result, { ...result, fallbackFrom: null }, { ...result, fallbackFrom: 'proxy' as const }, { ...result, route: null, fallbackFrom: 'direct' as const }]) {
        const content = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: diagnostic }, locale)!
        expect(content.detail).not.toContain('→')
        expect(content.detail).not.toContain(locale === 'zh' ? '自动切换' : 'Automatically switched')
      }
    })
    it(`never reports empty feeds or browser-only sources as healthy in ${locale}`, () => {
      const empty = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: { ...result, status: 'empty', itemCount: 0 } }, locale)!
      const manual = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: { ...result, status: 'manual', route: null, elapsedMs: 0, itemCount: 0, error: 'Open the site in the reader.' } }, locale)!
      expect(empty.tone).toBe('warning')
      expect(empty.label).toBe(locale === 'zh' ? '连接正常，暂无数据' : 'Connected, no items')
      expect(manual).toMatchObject({ tone: 'warning', detail: '', error: 'Open the site in the reader.' })
      expect(manual.label).toBe(locale === 'zh' ? '需在网页中测试' : 'Browser check required')
    })
    it(`keeps empty results and browser-only outcomes distinct when fallback metadata exists in ${locale}`, () => {
      const empty = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: { ...result, status: 'empty', itemCount: 0, fallbackFrom: 'direct' } }, locale)!
      const manual = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: { ...result, status: 'manual', fallbackFrom: 'direct' } }, locale)!
      expect(empty.tone).toBe('warning')
      expect(empty.detail).toContain(locale === 'zh' ? '自动切换：直连 → 代理' : 'Automatically switched: Direct → Proxy')
      expect(empty.label).toBe(locale === 'zh' ? '连接正常，暂无数据' : 'Connected, no items')
      expect(manual.tone).toBe('warning')
      expect(manual.detail).toBe('')
      expect(manual.label).toBe(locale === 'zh' ? '需在网页中测试' : 'Browser check required')
    })
    it(`keeps errors visible and localizes invalid requests in ${locale}`, () => {
      const failed = sourceConnectionPresentation({ key: 'saved', phase: 'done', result: { ...result, status: 'failed', error: 'HTTP 403' } }, locale)!
      expect(failed).toMatchObject({ tone: 'failed', error: 'HTTP 403' })
      expect(sourceConnectionPresentation({ key: 'saved', phase: 'error', error: 'invalid-source-test-request' }, locale)?.error)
        .toBe(locale === 'zh' ? '数据源测试参数无效' : 'Invalid source test request.')
      expect(sourceConnectionPresentation({ key: 'saved', phase: 'testing' }, locale)?.tone).toBe('testing')
      expect(sourceConnectionPresentation(undefined, locale)).toBeUndefined()
    })
  }
})


describe('source health indicator', () => {
  it('turns a previously failed collection green after a successful test', () => {
    expect(sourceHealthTone({ key: 'current', phase: 'done', result }, 'failed')).toBe('healthy')
  })

  it('shows failed diagnostics in red and never marks empty or browser-only results green', () => {
    for (const status of ['failed', 'empty', 'manual'] as const) {
      expect(sourceHealthTone({ key: 'current', phase: 'done', result: { ...result, status } }, 'succeeded'))
        .toBe(status === 'failed' ? 'failed' : '')
    }
    expect(sourceHealthTone({ key: 'current', phase: 'error', error: 'request failed' }, 'succeeded')).toBe('failed')
  })

  it('keeps collection health before completion and ignores stale configuration results', () => {
    for (const state of [undefined, { key: 'current', phase: 'queued' }, { key: 'current', phase: 'testing' }] as const) {
      expect(sourceHealthTone(state, 'failed')).toBe('failed')
      expect(sourceHealthTone(state, 'succeeded')).toBe('healthy')
      expect(sourceHealthTone(state, null)).toBe('')
    }
    const stale: SourceConnectionTestState = { key: 'old', phase: 'done', result }
    expect(sourceHealthTone(matchingSourceConnectionState(stale, 'current'), 'failed')).toBe('failed')
  })
})
