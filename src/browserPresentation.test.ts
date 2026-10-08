/** Regression tests for browser chrome state, trusted domains and bounded controls. */

import { describe, expect, it } from 'vitest'
import { clampDeviceDimension, isXiaohongshuAddress, nextZoom, pageTranslationRequest, parseDownloadDetail, runBrowserNavigation } from './browserPresentation'

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
  it('translates immediately with the language saved in browser settings', () => {
    expect(pageTranslationRequest({
      searchEngine: 'bing',
      zoom: 1,
      rememberHistory: true,
      translationLanguage: 'en',
    })).toEqual({ kind: 'translatePage', targetLanguage: 'en' })
  })
})
