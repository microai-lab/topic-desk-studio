/** Regression tests for browser chrome state, trusted domains and bounded controls. */

import { describe, expect, it, vi } from 'vitest'
import { browserTabClosePlan, clampDeviceDimension, isXiaohongshuAddress, nextZoom, observeReaderRecovery, pageTranslationRequest, parseDownloadDetail, runBrowserCookieClear, runBrowserNavigation, runBrowserTabClose } from './browserPresentation'

describe('runBrowserTabClose', () => {
  it('removes each tab only after native close finishes and ignores duplicate ids', async () => {
    const events: string[] = []
    await runBrowserTabClose(['first', 'second', 'first'], async (id) => {
      events.push(`native:${id}`)
      await Promise.resolve()
      events.push(`finished:${id}`)
    }, (id) => events.push(`removed:${id}`), vi.fn())
    expect(events).toEqual(['native:first', 'finished:first', 'removed:first', 'native:second', 'finished:second', 'removed:second'])
  })

  it('keeps a failed close visible and continues closing the remaining tabs', async () => {
    const error = new Error('native close failed')
    const removed = vi.fn()
    const failed = vi.fn()
    await runBrowserTabClose(['playing', 'other'], async (id) => {
      if (id === 'playing') throw error
    }, removed, failed)
    expect(removed).toHaveBeenCalledExactlyOnceWith('other')
    expect(failed).toHaveBeenCalledExactlyOnceWith('playing', error)
  })

  it('does nothing for an empty selection', async () => {
    const close = vi.fn()
    const removed = vi.fn()
    const failed = vi.fn()
    await runBrowserTabClose([], close, removed, failed)
    expect(close).not.toHaveBeenCalled()
    expect(removed).not.toHaveBeenCalled()
    expect(failed).not.toHaveBeenCalled()
  })
})

describe('runBrowserNavigation', () => {
  it('keeps a completion received before command resolution instead of restarting loading', async () => {
    let loading = false
    const events: string[] = []
    await runBrowserNavigation(async () => { events.push('native-finished'); loading = false },
      () => { events.push('start'); loading = true }, () => { events.push('failure') })
    expect(events).toEqual(['start', 'native-finished'])
    expect(loading).toBe(false)
  })
  it('clears optimistic loading when navigation fails and preserves the error', async () => {
    let loading = false
    const error = new Error('invalid URL')
    await expect(runBrowserNavigation(async () => { throw error },
      () => { loading = true }, () => { loading = false })).rejects.toBe(error)
    expect(loading).toBe(false)
  })
})

describe('parseDownloadDetail', () => {
  it('preserves trusted states and native paths', () => {
    expect(parseDownloadDetail('complete\n/Users/test/Downloads/report.pdf')).toEqual({
      state: 'complete',
      path: '/Users/test/Downloads/report.pdf',
    })
    expect(parseDownloadDetail('downloading')).toEqual({ state: 'downloading', path: '' })
  })

  it('fails closed for malformed or unknown states', () => {
    expect(parseDownloadDetail('')).toEqual({ state: 'failed', path: '' })
    expect(parseDownloadDetail('ready\n/tmp/file')).toEqual({ state: 'failed', path: '/tmp/file' })
  })
})

describe('isXiaohongshuAddress', () => {
  it('accepts the site and subdomains but rejects look-alikes and invalid input', () => {
    expect(isXiaohongshuAddress('https://www.xiaohongshu.com/explore')).toBe(true)
    expect(isXiaohongshuAddress('https://xiaohongshu.com/')).toBe(true)
    expect(isXiaohongshuAddress('https://xiaohongshu.com.evil.example/')).toBe(false)
    expect(isXiaohongshuAddress('not a url')).toBe(false)
  })
})

describe('bounded browser controls', () => {
  const levels = [0.25, 0.5, 1, 1.5, 3]

  it('selects adjacent zoom levels without crossing the supported range', () => {
    expect(nextZoom(1, 1, levels)).toBe(1.5)
    expect(nextZoom(1, -1, levels)).toBe(0.5)
    expect(nextZoom(3, 1, levels)).toBeUndefined()
    expect(nextZoom(0.25, -1, levels)).toBeUndefined()
  })

  it('clamps preview dimensions and handles invalid numbers', () => {
    expect(clampDeviceDimension(120, 240)).toBe(240)
    expect(clampDeviceDimension(390, 240)).toBe(390)
    expect(clampDeviceDimension(9_000, 200)).toBe(2560)
    expect(clampDeviceDimension(Number.NaN, 200)).toBe(200)
  })
})

describe('pageTranslationRequest', () => {
  it('toggles an active translation back to the original, including pending or failed paragraphs', () => {
    const settings = { searchEngine: 'bing', zoom: 1, rememberHistory: true, translationLanguage: 'zh-CN' } as const
    expect(pageTranslationRequest(settings, true)).toEqual({ kind: 'restorePageTranslation' })
    expect(pageTranslationRequest(settings, false)).toEqual({ kind: 'translatePage', targetLanguage: 'zh-CN' })
  })
  it('translates immediately with the language saved in browser settings', () => {
    expect(pageTranslationRequest({
      searchEngine: 'bing',
      zoom: 1,
      rememberHistory: true,
      translationLanguage: 'en',
    })).toEqual({ kind: 'translatePage', targetLanguage: 'en' })
  })
})

describe('reader recovery after idle', () => {
  it('resynchronizes unchanged geometry on focus, pageshow and visibility, then cleans up', () => {
    const host = new EventTarget()
    const page = Object.assign(new EventTarget(), { hidden: false })
    const recover = vi.fn()
    const stop = observeReaderRecovery(host as Window, page as unknown as Document, recover)
    host.dispatchEvent(new Event('focus'))
    host.dispatchEvent(new Event('pageshow'))
    page.dispatchEvent(new Event('visibilitychange'))
    expect(recover).toHaveBeenCalledTimes(3)
    page.hidden = true
    host.dispatchEvent(new Event('focus'))
    page.dispatchEvent(new Event('visibilitychange'))
    expect(recover).toHaveBeenCalledTimes(3)
    page.hidden = false
    page.dispatchEvent(new Event('visibilitychange'))
    expect(recover).toHaveBeenCalledTimes(4)
    stop()
    host.dispatchEvent(new Event('focus'))
    host.dispatchEvent(new Event('pageshow'))
    page.dispatchEvent(new Event('visibilitychange'))
    expect(recover).toHaveBeenCalledTimes(4)
  })
})

describe('clearing remembered browser logins', () => {
  it('clears before resetting tabs and reopening a fresh native session', async () => {
    const calls: string[] = []
    await runBrowserCookieClear(async () => { calls.push('clear') }, () => { calls.push('reset') }, async () => { calls.push('reopen') })
    expect(calls).toEqual(['clear', 'reset', 'reopen'])
  })
  it('keeps current tabs when clearing fails and reports a failed reload after a successful clear', async () => {
    const reset = vi.fn()
    const reopen = vi.fn(async () => {})
    await expect(runBrowserCookieClear(async () => { throw new Error('clear failed') }, reset, reopen)).rejects.toThrow('clear failed')
    expect(reset).not.toHaveBeenCalled(); expect(reopen).not.toHaveBeenCalled()
    await expect(runBrowserCookieClear(async () => {}, reset, async () => { throw new Error('reload failed') })).rejects.toThrow('reload failed')
    expect(reset).toHaveBeenCalledTimes(1)
  })
})


describe('workspace after closing browser tabs', () => {
  it('restores topics when the final tab closes, including a collapsed-topic workspace', () => {
    expect(browserTabClosePlan([{ id: 'last' }], 'last', 'last')).toEqual({
      remaining: [], activeId: '', restoreTopics: true,
    })
  })

  it('keeps reading mode for remaining tabs and selects the nearest tab', () => {
    const tabs = [{ id: 'first' }, { id: 'middle' }, { id: 'last' }]
    expect(browserTabClosePlan(tabs, 'middle', 'middle')).toEqual({
      remaining: [tabs[0], tabs[2]], activeId: 'last', restoreTopics: false,
    })
    expect(browserTabClosePlan(tabs, 'last', 'last')?.activeId).toBe('middle')
    expect(browserTabClosePlan(tabs, 'first', 'last')?.activeId).toBe('first')
  })

  it('restores topics only after all native closes succeed and ignores stale completions', async () => {
    let tabs = [{ id: 'first' }, { id: 'last' }]
    let activeId = 'first'
    const restoration: boolean[] = []
    await runBrowserTabClose(['first', 'last'], async () => {}, id => {
      const plan = browserTabClosePlan(tabs, activeId, id)!
      tabs = plan.remaining
      activeId = plan.activeId
      restoration.push(plan.restoreTopics)
    }, vi.fn())
    expect(restoration).toEqual([false, true])
    expect(browserTabClosePlan([{ id: 'new' }], 'new', 'last')).toBeUndefined()
    expect(browserTabClosePlan([], '', 'last')).toBeUndefined()
  })

  it('retains the collapsed reader if its last native tab fails to close', async () => {
    const closed = vi.fn()
    await runBrowserTabClose(['last'], async () => { throw new Error('native failure') }, closed, vi.fn())
    expect(closed).not.toHaveBeenCalled()
  })
})
