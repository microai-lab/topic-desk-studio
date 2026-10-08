/** Browser lifecycle requests must remain ordered even when a native call fails. */
import { afterEach, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { browserControl, browserRequest } from './api'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

afterEach(() => vi.mocked(invoke).mockReset())

it('keeps menus and navigation responsive while translation is pending', async () => {
  const native = vi.mocked(invoke)
  let finish!: (value: unknown) => void
  native.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve }))
  native.mockResolvedValue(undefined)
  const translation = browserControl({ kind: 'translatePage', targetLanguage: 'zh-CN' })
  await Promise.resolve()
  await browserControl({ kind: 'overlay', visible: true })
  await browserControl({ kind: 'library' })
  await browserRequest('reload', 'tab-1')
  expect(native).toHaveBeenCalledTimes(4)
  finish({ translatedCount: 1 })
  await translation
})

it('waits for creation before close and allows reopening after failure', async () => {
  let rejectCreation!: (error: Error) => void
  const native = vi.mocked(invoke)
  native.mockImplementationOnce(() => new Promise((_, reject) => { rejectCreation = reject }))
  native.mockResolvedValue(undefined)
  const opening = browserRequest('sync', 'tab-1', { x: 400, y: 100, width: 600, height: 700, viewportHeight: 800 }, 'https://example.com')
  const failed = expect(opening).rejects.toThrow('creation failed')
  const closing = browserRequest('close', 'tab-1')
  await Promise.resolve()
  expect(native).toHaveBeenCalledTimes(1)
  rejectCreation(new Error('creation failed'))
  await failed
  await closing
  await browserRequest('sync', 'tab-2', { x: 400, y: 100, width: 600, height: 700, viewportHeight: 800 }, 'https://example.org')
  expect(native.mock.calls.map((call) => (call[1] as { action: string }).action)).toEqual(['sync', 'close', 'sync'])
})

it('serializes typed browser controls behind lifecycle work', async () => {
  const native = vi.mocked(invoke)
  native.mockResolvedValueOnce(undefined)
  native.mockResolvedValueOnce({ translatedCount: 3, detectedLanguage: 'en' })

  await browserRequest('reload', 'tab-1')
  await expect(browserControl({ kind: 'translatePage', targetLanguage: 'zh-CN' })).resolves.toEqual({
    translatedCount: 3,
    detectedLanguage: 'en',
  })

  expect(native.mock.calls).toEqual([
    ['browser_request', { action: 'reload', tabId: 'tab-1', bounds: null, url: null }],
    ['browser_control', { request: { kind: 'translatePage', targetLanguage: 'zh-CN' } }],
  ])
})

it('reveals a completed download by record id without accepting a frontend path', async () => {
  const native = vi.mocked(invoke)
  native.mockResolvedValue(undefined)

  await browserControl({ kind: 'revealDownload', id: 42 })

  expect(native).toHaveBeenCalledWith('browser_control', {
    request: { kind: 'revealDownload', id: 42 },
  })
})
