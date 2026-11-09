/** Local popup IPC must preserve the article, fence stale events, and retain handlers in main chrome. */
import { afterEach, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { popupBrowserMenu, closeBrowserPopup, keepBrowserPopup, leaveBrowserPopup, updateBrowserPopupZoom, serializePopoverItems } from './nativeBrowserMenu'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))

function environment() {
  vi.stubGlobal('window', { innerWidth: 1180, innerHeight: 680, matchMedia: () => ({ matches: false }) })
  vi.stubGlobal('document', { documentElement: { dataset: { theme: 'light' } } })
}
afterEach(async () => { await closeBrowserPopup(); vi.unstubAllGlobals(); vi.clearAllMocks(); vi.useRealTimers() })

it('serializes safe display data with unique IDs, omitting disabled callbacks and executable code', () => {
  const handlers = new Map<string, () => void>()
  const action = vi.fn()
  const items = serializePopoverItems([{ text: 'Group', items: [{ text: '<script>title</script>', action }] }, { text: 'Disabled', enabled: false, action }, { item: 'Separator' }], handlers)
  expect(items[0]!.id).not.toBe(items[0]!.children[0]!.id)
  expect(handlers.size).toBe(1)
  expect(JSON.stringify(items)).not.toContain('action')
  expect(items[0]!.children[0]!.text).toBe('<script>title</script>')
})

it('opens an isolated view without hiding or resizing the article, and dispatches only current session actions', async () => {
  environment()
  const callback = vi.fn(), visibility = vi.fn()
  await popupBrowserMenu([{ text: 'Find', action: callback }], { x: 1100, y: 80 }, 'More', visibility)
  const request = vi.mocked(invoke).mock.calls.find(call => call[0] === 'browser_popover_open')![1] as any
  expect(request.data.x + request.data.width).toBeLessThanOrEqual(1180)
  expect(request.data.height).toBe(592)
  expect(visibility).toHaveBeenLastCalledWith(true)
  expect(invoke).not.toHaveBeenCalledWith('browser_control', expect.anything())
  const handler = vi.mocked(listen).mock.calls.at(-1)![1] as any
  handler({ payload: { token: 'stale', kind: 'action', id: request.data.items[0].id } })
  expect(callback).not.toHaveBeenCalled()
  handler({ payload: { token: request.data.token, kind: 'action', id: request.data.items[0].id } })
  expect(callback).toHaveBeenCalledOnce()
  expect(visibility).toHaveBeenLastCalledWith(false)
})

it('allows travel between trigger and popup, then dismisses on leave', async () => {
  environment(); vi.useFakeTimers()
  await popupBrowserMenu([], { x: 0, y: 80 }, 'Downloads', vi.fn())
  const open = vi.mocked(invoke).mock.calls.find(call => call[0] === 'browser_popover_open')![1] as any
  const handler = vi.mocked(listen).mock.calls.at(-1)![1] as any
  handler({ payload: { token: open.data.token, kind: 'ready' } })
  leaveBrowserPopup(); vi.advanceTimersByTime(100); keepBrowserPopup(); vi.advanceTimersByTime(200)
  expect(vi.mocked(invoke).mock.calls.filter(call => call[0] === 'browser_popover_event')).toHaveLength(0)
  leaveBrowserPopup(); await vi.advanceTimersByTimeAsync(201)
  expect(invoke).toHaveBeenLastCalledWith('browser_popover_event', expect.objectContaining({ kind: 'close' }))
})

it('restores visibility state and reports failures when creation fails', async () => {
  environment()
  vi.mocked(invoke).mockRejectedValueOnce(new Error('create failed'))
  const visibility = vi.fn()
  await expect(popupBrowserMenu([], { x: 0, y: 80 }, 'More', visibility)).rejects.toThrow('create failed')
  expect(visibility).toHaveBeenLastCalledWith(false)
})

it('keeps zoom controls open, updates their saved value and boundaries, and uses the compact menu width', async () => {
  environment()
  const action = vi.fn(), visibility = vi.fn()
  await popupBrowserMenu([{ text: 'Zoom', layout: 'zoom', items: [
    { text: 'Out', action, enabled: false, keepOpen: true },
    { text: '25%', action, keepOpen: true },
    { text: 'In', action, keepOpen: true },
  ] }], { x: 900, y: 80 }, 'More', visibility)
  const open = vi.mocked(invoke).mock.calls.find(call => call[0] === 'browser_popover_open')![1] as any
  expect(open.data.width).toBe(240)
  const handler = vi.mocked(listen).mock.calls.at(-1)![1] as any
  handler({ payload: { token: open.data.token, kind: 'action', id: open.data.items[0].children[2].id } })
  expect(action).toHaveBeenCalledOnce()
  expect(visibility).toHaveBeenLastCalledWith(true)
  await updateBrowserPopupZoom(3)
  const update = vi.mocked(invoke).mock.calls.find(call => call[0] === 'browser_popover_update')![1] as any
  expect(update.data.items[0].children.map((item: any) => item.enabled)).toEqual([true, true, false])
  expect(update.data.items[0].children[1].text).toBe('300%')
  await updateBrowserPopupZoom(.25)
  expect(update.data.items[0].children.map((item: any) => item.enabled)).toEqual([false, true, true])
})
