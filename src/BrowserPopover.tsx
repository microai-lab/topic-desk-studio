/** Isolated local popup UI. It receives display data and emits only session-bound UI events. */
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { ArrowDownToLine, ChevronDown, ExternalLink, File, FolderOpen, History, Languages, Minus, Plus, Printer, Search, Settings2, Trash2, X, ZoomIn } from 'lucide-react'
import type { PopoverData, PopoverItem } from './nativeBrowserMenu'

/** Render with the application theme; the native host reveals only committed, measured content. */
export function BrowserPopover() {
  const [data, setData] = useState<PopoverData>()
  const [error, setError] = useState('')
  const body = useRef<HTMLDivElement>(null)
  const send = (kind: string, id: string | null = null) => {
    if (data) void invoke('browser_popover_event', { token: data.token, kind, id }).catch(reason => setError(String(reason)))
  }
  useEffect(() => {
    let live = true
    const stop = listen<PopoverData>('browser-popover-data', ({ payload }) => { if (live) setData(payload) })
    void stop.then(() => invoke<PopoverData | null>('browser_popover_data')).then(value => { if (live && value) setData(value) }).catch(reason => { if (live) setError(String(reason)) })
    return () => { live = false; void stop.then(unlisten => unlisten()) }
  }, [])
  useLayoutEffect(() => {
    if (!data || !body.current) return
    document.documentElement.dataset.theme = data.theme
    const element = body.current
    const sync = () => void invoke('browser_popover_ready', { token: data.token, height: element.scrollHeight }).catch(reason => setError(String(reason)))
    sync()
    const resize = new ResizeObserver(sync)
    resize.observe(element)
    return () => resize.disconnect()
  }, [data])
  if (!data) return null
  return <div className="browser-popover-surface" ref={body} style={{ maxHeight: data.height }} onPointerEnter={() => send('enter')} onPointerLeave={event => { if (event.pointerType !== 'touch') send('leave') }} onKeyDown={event => { if (event.key === 'Escape') send('close') }}>
    <header><strong>{data.title}</strong><button type="button" aria-label="Close" onClick={() => send('close')}><X aria-hidden="true" /></button></header>
    <div className="popover-items"><BrowserPopoverItems items={data.items} onAction={id => send('action', id)} /></div>
    {error ? <p role="alert">{error}</p> : null}
  </div>
}

/** Pure menu rows keep untrusted download names as text and expose only known action IDs. */
export function BrowserPopoverItems({ items, onAction }: { items: PopoverItem[]; onAction: (id: string) => void }) {
  const icon = (text: string) => {
    if (/查找|Find/.test(text)) return Search
    if (/缩放|Zoom|%/.test(text)) return ZoomIn
    if (/翻译|原文|original|translat/i.test(text)) return Languages
    if (/打印|Print/.test(text)) return Printer
    if (/文件夹|folder/i.test(text)) return FolderOpen
    if (/默认浏览器|default browser/i.test(text)) return ExternalLink
    if (/设置|settings/i.test(text)) return Settings2
    if (/清除|Clear/.test(text)) return Trash2
    if (/历史|History/.test(text)) return History
    if (/下载|Download/.test(text)) return ArrowDownToLine
    return File
  }
  const row = (item: PopoverItem, index: number) => {
    if (item.separator) return <hr key={`separator-${index}`} />
    const Glyph = icon(item.text)
    if (item.layout === 'zoom' && item.children.length === 3) return <div className="popover-zoom-row" key={item.id}>
      <span><ZoomIn aria-hidden="true" />{item.text}</span>
      <div>{item.children.map((control, index) => {
        const label = index === 1 ? (/缩放/.test(item.text) ? '重置缩放为 100%' : 'Reset zoom to 100%') : control.text
        return <button key={control.id} type="button" title={label} aria-label={label} disabled={!control.enabled} onClick={() => onAction(control.id)}>
          {index === 0 ? <Minus aria-hidden="true" /> : index === 2 ? <Plus aria-hidden="true" /> : control.text}
        </button>
      })}</div>
    </div>
    if (item.children.length) return <details key={`${item.id}-${index}`} className="popover-group">
      <summary><Glyph aria-hidden="true" /><span title={item.text}>{item.text}</span><ChevronDown aria-hidden="true" /></summary>
      <div>{item.children.map(row)}</div>
    </details>
    return <button className="popover-item" type="button" key={`${item.id}-${index}`} disabled={!item.enabled} onClick={() => onAction(item.id)}>
      <Glyph aria-hidden="true" /><span title={item.text}>{item.text}</span>
    </button>
  }
  return <>{items.map(row)}</>
}
