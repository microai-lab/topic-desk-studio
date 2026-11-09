/** Local child-WebView popups preserve the live article and keep all handlers in trusted main chrome. */
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'

/** Shared menu descriptions never send executable callbacks to the popover. */
export type BrowserPopupItem = { item: 'Separator' } | { text: string; enabled?: boolean; action?: () => void; items?: BrowserPopupItem[]; layout?: 'zoom'; keepOpen?: boolean }
/** Serializable row data rendered as text inside the trusted local popup. */
export interface PopoverItem { id: string; text: string; enabled: boolean; separator: boolean; children: PopoverItem[]; layout?: string; keepOpen?: boolean }
/** Logical window coordinates and a session token shared with the native popup host. */
export interface PopoverData { token: string; title: string; theme: 'light' | 'dark'; items: PopoverItem[]; x: number; y: number; width: number; height: number }

/** Generate session-local action IDs; display data cannot execute or name arbitrary native commands. */
export function serializePopoverItems(items: BrowserPopupItem[], handlers: Map<string, () => void>): PopoverItem[] {
  return items.map(item => {
    const id = `item-${crypto.randomUUID()}`
    if ('item' in item) return { id: '', text: '', enabled: false, separator: true, children: [] }
    if (item.action && (item.enabled !== false || item.keepOpen)) handlers.set(id, item.action)
    return { id, text: item.text, enabled: item.enabled !== false, separator: false, children: serializePopoverItems(item.items ?? [], handlers), layout: item.layout ?? '', keepOpen: item.keepOpen ?? false }
  })
}

let active: { data: PopoverData; title: string; token: string; handlers: Map<string, () => void>; unlisten: UnlistenFn; visibility: (open: boolean) => void; ready: boolean } | undefined
let hovered = true
let dismissTimer: ReturnType<typeof setTimeout> | undefined
/** Both the toolbar trigger and child WebView cancel the same travel-gap dismissal timer. */
export function keepBrowserPopup(): void { hovered = true; clearTimeout(dismissTimer); dismissTimer = undefined }
/** Delay briefly so moving from a toolbar button into its popup does not dismiss it. */
export function leaveBrowserPopup(): void {
  hovered = false
  if (!dismissTimer && active?.ready) dismissTimer = setTimeout(() => { dismissTimer = undefined; void closeBrowserPopup() }, 200)
}
/** Close only the current session; late messages from an older popup cannot close a replacement. */
export async function closeBrowserPopup(): Promise<void> {
  keepBrowserPopup()
  const old = active
  active = undefined
  if (!old) return
  old.unlisten(); old.visibility(false)
  await invoke('browser_popover_event', { token: old.token, kind: 'close', id: null })
}

/** Create or reuse the local overlay without hiding, resizing, or snapshotting article content. */
async function openPopup(items: BrowserPopupItem[], anchor: { x: number; y: number }, title: string, visibility: (open: boolean) => void): Promise<void> {
  if (active?.title === title) { await closeBrowserPopup(); return }
  await closeBrowserPopup()
  const token = crypto.randomUUID()
  const handlers = new Map<string, () => void>()
  const width = Math.min(items.some(item => !('item' in item) && item.layout === 'zoom') ? 240 : 360, Math.max(200, window.innerWidth - 16))
  const data: PopoverData = {
    token, title, items: serializePopoverItems(items, handlers),
    theme: document.documentElement.dataset.theme === 'dark' || window.matchMedia('(prefers-color-scheme: dark)').matches && document.documentElement.dataset.theme !== 'light' ? 'dark' : 'light',
    x: Math.max(0, Math.min(anchor.x, window.innerWidth - width - 8)), y: Math.max(0, Math.min(anchor.y, window.innerHeight - 48)), width,
    height: Math.max(48, Math.min(900, window.innerHeight - anchor.y - 8)),
  }
  const unlisten = await listen<{ token: string; kind: string; id?: string }>('browser-popover-event', ({ payload }) => {
    if (payload.token !== active?.token) return
    if (payload.kind === 'ready') { active.ready = true; if (!hovered) leaveBrowserPopup() }
    else if (payload.kind === 'enter') keepBrowserPopup()
    else if (payload.kind === 'leave') leaveBrowserPopup()
    else if (payload.kind === 'action' || payload.kind === 'close') {
      const handler = payload.kind === 'action' ? handlers.get(payload.id ?? '') : undefined
      const find = (items: PopoverItem[]): PopoverItem | undefined => { for (const item of items) { if (item.id === payload.id) return item; const child = find(item.children); if (child) return child } return undefined }
      if (payload.kind === 'action' && find(active.data.items)?.keepOpen) { keepBrowserPopup(); handler?.(); return }
      const old = active
      active = undefined; keepBrowserPopup(); old?.unlisten(); visibility(false)
      handler?.()
    }
  })
  hovered = true
  active = { data, title, token, handlers, unlisten, visibility, ready: false }
  visibility(true)
  try { await invoke('browser_popover_open', { data }) }
  catch (error) { await closeBrowserPopup(); throw error }
}

let opening: Promise<unknown> = Promise.resolve()
/** Serialize fast clicks so child creation and listener replacement cannot race each other. */
export function popupBrowserMenu(items: BrowserPopupItem[], anchor: { x: number; y: number }, title: string, visibility: (open: boolean) => void): Promise<void> {
  const next = opening.then(() => openPopup(items, anchor, title, visibility))
  opening = next.catch(() => {})
  return next
}

/** Apply a successfully saved zoom value without closing or recreating the current popup. */
export async function updateBrowserPopupZoom(value: number): Promise<void> {
  if (!active) return
  const row = active.data.items.find(item => item.layout === 'zoom')
  if (!row || row.children.length !== 3) return
  row.children[0]!.enabled = row.enabled && value > .25
  row.children[1]!.text = `${Math.round(value * 100)}%`
  row.children[2]!.enabled = row.enabled && value < 3
  await invoke('browser_popover_update', { data: active.data })
}
