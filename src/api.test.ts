/** Browser lifecycle requests must remain ordered even when a native call fails. */
import { expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { browserRequest } from './api'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

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
