/** Browser chrome modeled on the reference: omnibox, native content and local tools. */
import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { browserControl, browserRequest } from './api'
import type { BrowserLibrary, BrowserSettings, BrowserStatus } from './api'
import type { Locale } from './i18n'

type Panel = 'menu' | 'history' | 'downloads' | 'import' | 'clear' | 'settings' | null
export interface BrowserTab { id: string; url: string; title: string; topicId?: number }
const DEFAULT_SETTINGS: BrowserSettings = { searchEngine: 'bing', zoom: 1, rememberHistory: true }
const ZOOMS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3]

/** Small stroke icons keep browser controls distinct from the application navigation. */
function Icon({ name }: { name: 'back' | 'forward' | 'reload' | 'more' | 'expand' | 'close' | 'globe' | 'search' | 'device' }) {
  const paths = {
    back: 'M19 12H5m7-7-7 7 7 7', forward: 'M5 12h14m-7-7 7 7-7 7',
    reload: 'M20 4v6h-6M4 20v-6h6M5.4 8a7 7 0 0 1 11.5-3L20 10M4 14l3.1 5A7 7 0 0 0 18.6 16',
    more: 'M12 5h.01M12 12h.01M12 19h.01', expand: 'M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5',
    close: 'm6 6 12 12M6 18 18 6', globe: 'M3 12h18M12 3a18 18 0 0 1 0 18 18 18 0 0 1 0-18',
    search: 'm16 16 5 5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0', device: 'M3 4h13v12H3zM7 20h5M10 16v4M18 9h4v12h-4z',
  }
  return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={name === 'more' ? 3.5 : 1.6} strokeLinecap="round" strokeLinejoin="round"><path d={paths[name]} />{name === 'globe' ? <circle cx="12" cy="12" r="9" /> : null}</svg>
}

/** Host a native browser while React owns the address bar, overlays and layout. */
export function BrowserPane({ tabs, activeTabId, locale, expanded, closing, onActivate, onNewTab, onUpdateTab, onCloseTab, onExpand, onCollectXiaohongshu, onClose, onResize }: {
  tabs: BrowserTab[]; activeTabId: string; locale: Locale; expanded: boolean; closing?: boolean
  onActivate: (id: string) => void; onNewTab: () => void
  onUpdateTab: (id: string, update: Partial<Pick<BrowserTab, 'url' | 'title'>>) => void
  onCloseTab: (id: string) => void; onExpand: () => void
  onCollectXiaohongshu: (tabId: string) => Promise<string>
  onClose: () => void; onResize: (width: number) => void
}) {
  const viewport = useRef<HTMLDivElement>(null)
  const addressInput = useRef<HTMLInputElement>(null)
  const alive = useRef(true)
  const activeTabRef = useRef(activeTabId)
  const createdTabs = useRef(new Set<string>())
  const [statuses, setStatuses] = useState<Record<string, BrowserStatus>>({})
  const [address, setAddress] = useState('')
  const [panel, setPanel] = useState<Panel>(null)
  const [preview, setPreview] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [library, setLibrary] = useState<BrowserLibrary>({ history: [], downloads: [], settings: DEFAULT_SETTINGS })
  const [busy, setBusy] = useState(false)
  const [findOpen, setFindOpen] = useState(false)
  const [findText, setFindText] = useState('')
  const [findResult, setFindResult] = useState<boolean | null>(null)
  const [sensitive, setSensitive] = useState(false)
  const [device, setDevice] = useState(false)
  const [deviceWidth, setDeviceWidth] = useState(390)
  const [deviceHeight, setDeviceHeight] = useState(844)
  const [filter, setFilter] = useState('')
  const [importText, setImportText] = useState('')
  const [clear, setClear] = useState({ history: true, cookies: false, downloads: false })
  const zh = locale === 'zh'
  const t = (cn: string, en: string): string => zh ? cn : en
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0]
  const status = statuses[activeTabId] ?? { tabId: activeTabId, url: activeTab?.url ?? '', title: activeTab?.title ?? '', loading: false, canBack: false, canForward: false }
  activeTabRef.current = activeTabId
  const fail = useCallback((reason: unknown) => { if (alive.current) setError(String(reason)) }, [])
  const refreshLibrary = useCallback(async () => {
    const value = await browserControl<BrowserLibrary>({ kind: 'library' })
    if (alive.current) setLibrary(value)
  }, [])
  const bounds = () => {
    const rect = viewport.current?.getBoundingClientRect()
    return rect && rect.width > 0 && rect.height > 0 ? { x: rect.x, y: rect.y, width: rect.width, height: rect.height, viewportHeight: window.innerHeight } : undefined
  }
  const openAddress = useCallback(async (input: string) => {
    const area = bounds()
    if (!area || !input.trim() || !activeTabId) return
    setError(null); setNotice(null)
    await browserControl({ kind: 'overlay', visible: false })
    setPanel(null)
    await browserRequest('sync', activeTabId, area, input)
    createdTabs.current.add(activeTabId)
    if (alive.current) {
      setAddress(input)
      setStatuses((old) => ({ ...old, [activeTabId]: { ...status, tabId: activeTabId, loading: true } }))
      onUpdateTab(activeTabId, { url: input })
    }
  }, [activeTabId, onUpdateTab, status])

  useEffect(() => {
    alive.current = true
    const unlisten = listen<BrowserStatus>('browser-status', ({ payload }) => {
      if (!alive.current) return
      setStatuses((old) => ({ ...old, [payload.tabId]: payload }))
      onUpdateTab(payload.tabId, { url: payload.url, ...(payload.title ? { title: payload.title } : {}) })
      // Native navigation can occur while the omnibox remains the nominally
      // focused React element (for example after a link click in the child
      // WebView). Always accept the canonical top-level URL from Rust.
      if (payload.tabId === activeTabRef.current) setAddress(payload.url)
    })
    const unlibrary = listen('browser-library-changed', () => { void refreshLibrary().catch(fail) })
    const unpopup = listen<{ tabId: string; url: string }>('browser-open-popup', ({ payload }) => {
      const area = bounds()
      if (alive.current && area) void browserRequest('sync', payload.tabId, area, payload.url).catch(fail)
    })
    void refreshLibrary().catch(fail)
    return () => {
      alive.current = false
      void unlisten.then((stop) => stop()).catch(() => {})
      void unlibrary.then((stop) => stop()).catch(() => {})
      void unpopup.then((stop) => stop()).catch(() => {})
      void browserRequest('hideAll').catch(() => {})
    }
  }, [fail, onUpdateTab, refreshLibrary])

  // Keep native geometry in sync without destroying navigation history on topic changes.
  useEffect(() => {
    const element = viewport.current
    if (!element) return
    let frame = 0
    const sync = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        const area = bounds()
        if (area && activeTabId) void browserRequest('sync', activeTabId, area).catch(fail)
      })
    }
    const observer = new ResizeObserver(sync)
    observer.observe(element)
    window.addEventListener('resize', sync)
    window.addEventListener('scroll', sync, true)
    sync()
    return () => { cancelAnimationFrame(frame); observer.disconnect(); window.removeEventListener('resize', sync); window.removeEventListener('scroll', sync, true) }
  }, [activeTabId, device, deviceHeight, deviceWidth, fail])

  useEffect(() => {
    if (!activeTab) return
    setAddress(activeTab.url)
    setPanel(null)
    const area = bounds()
    if (!area) return
    const firstUrl = !createdTabs.current.has(activeTab.id) && activeTab.url ? activeTab.url : undefined
    void browserRequest('sync', activeTab.id, area, firstUrl).then(() => {
      if (firstUrl) createdTabs.current.add(activeTab.id)
    }).catch(fail)
  }, [activeTab?.id, fail])

  const showPanel = async (next: Panel) => {
    setError(null); setNotice(null); setFilter('')
    const overlay = await browserControl<{ preview: string | null }>({ kind: 'overlay', visible: next !== null, capture: panel === null && next !== null && status.url !== '' })
    if (panel === null) setPreview(overlay.preview)
    setPanel(next)
    if (next && next !== 'menu') await refreshLibrary()
    if (next !== 'import') setImportText('')
  }
  const find = async (backwards = false) => {
    if (!findText) return
    const found = await browserControl<boolean>({ kind: 'find', text: findText, backwards, sensitive })
    setFindResult(found)
  }
  const setZoom = async (factor: number) => {
    await browserControl({ kind: 'zoom', factor })
    setLibrary((old) => ({ ...old, settings: { ...old.settings, zoom: factor } }))
  }
  const stepZoom = (direction: number) => {
    const current = library.settings.zoom
    const factor = direction > 0 ? ZOOMS.find((z) => z > current) : [...ZOOMS].reverse().find((z) => z < current)
    if (factor !== undefined) void setZoom(factor).catch(fail)
  }
  const perform = async (operation: () => Promise<unknown>, message?: string) => {
    setBusy(true); setError(null); setNotice(null)
    try { await operation(); if (message) setNotice(message) }
    catch (reason) { fail(reason) }
    finally { setBusy(false) }
  }
  const nativeAction = async (kind: 'print' | 'screenshot') => {
    await showPanel(null)
    await perform(async () => {
      const result = await browserControl<{ path?: string }>({ kind })
      if (kind === 'screenshot') setNotice(t('截图已保存到下载文件夹：', 'Screenshot saved to Downloads: ') + result.path)
    })
  }

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'l') { event.preventDefault(); addressInput.current?.focus(); addressInput.current?.select() }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') { event.preventDefault(); setFindOpen(true) }
      if (event.key === 'Escape') { if (panel) void showPanel(null).catch(fail); else setFindOpen(false) }
    }
    window.addEventListener('keydown', keydown)
    return () => window.removeEventListener('keydown', keydown)
  })

  const menuItem = (cn: string, en: string, operation: () => void, disabled = false, hint?: string) => <button className="browser-menu-item" type="button" disabled={disabled} onClick={operation}><span>{t(cn, en)}</span>{hint ? <small>{hint}</small> : null}</button>
  const panelTitle = panel === 'history' ? t('历史记录', 'History') : panel === 'downloads' ? t('下载', 'Downloads') : panel === 'import' ? t('导入 Cookie', 'Import cookies') : panel === 'clear' ? t('清除浏览数据', 'Clear browsing data') : t('浏览器设置', 'Browser settings')
  const hasPage = status.url !== ''
  const isXiaohongshu = (() => {
    try { const host = new URL(status.url).hostname; return host === 'xiaohongshu.com' || host.endsWith('.xiaohongshu.com') }
    catch { return false }
  })()

  return <aside className={`browser-pane browser-chrome${device ? ' device-mode' : ''}${closing ? ' browser-pane-closing' : ''}`} aria-label={t('内置浏览器', 'Browser')}>
    <div className="reader-divider" role="separator" aria-label={t('调整浏览器宽度', 'Resize browser')} aria-orientation="vertical" tabIndex={0}
      onKeyDown={(event) => {
        const workspace = event.currentTarget.parentElement?.parentElement
        if (!workspace || !['ArrowLeft', 'ArrowRight'].includes(event.key)) return
        event.preventDefault()
        const width = event.currentTarget.parentElement!.getBoundingClientRect().width / workspace.getBoundingClientRect().width * 100
        onResize(Math.max(35, Math.min(70, width + (event.key === 'ArrowLeft' ? 3 : -3))))
      }}
      onPointerDown={(event) => event.currentTarget.setPointerCapture(event.pointerId)}
      onPointerMove={(event) => {
        if (!event.currentTarget.hasPointerCapture(event.pointerId)) return
        const workspace = event.currentTarget.parentElement?.parentElement?.getBoundingClientRect()
        if (workspace) onResize(Math.max(35, Math.min(70, (workspace.right - event.clientX) / workspace.width * 100)))
      }} onPointerUp={(event) => event.currentTarget.releasePointerCapture(event.pointerId)} />
    <div className="browser-tabbar"><div className="browser-tabs" role="tablist" aria-label={t('浏览器标签页', 'Browser tabs')}>{tabs.map((tab) => <div className={`browser-tab${tab.id === activeTabId ? ' active' : ''}`} key={tab.id}><button role="tab" aria-selected={tab.id === activeTabId} title={tab.title || tab.url || t('新标签页', 'New tab')} onClick={() => onActivate(tab.id)}><Icon name="globe" /><span>{tab.title || t('新标签页', 'New tab')}</span></button><button className="browser-tab-close" title={t('关闭标签页', 'Close tab')} onClick={() => { void browserRequest('close', tab.id).catch(fail); onCloseTab(tab.id) }}>×</button></div>)}<button className="browser-new-tab" title={t('新建标签页', 'New tab')} onClick={onNewTab}>＋</button></div><div className="browser-tab-actions"><button className="browser-icon-button reader-expand" title={expanded ? t('恢复分栏', 'Split view') : t('放大', 'Expand')} onClick={onExpand}><Icon name="expand" /></button><button className="browser-icon-button" title={t('隐藏浏览器', 'Hide browser')} onClick={onClose}><Icon name="close" /></button></div></div>
    <header className="browser-toolbar">
      <button className="browser-icon-button" disabled={!status.canBack} title={t('后退', 'Back')} onClick={() => void browserRequest('back', activeTabId).catch(fail)}><Icon name="back" /></button>
      <button className="browser-icon-button" disabled={!status.canForward} title={t('前进', 'Forward')} onClick={() => void browserRequest('forward', activeTabId).catch(fail)}><Icon name="forward" /></button>
      <button className={`browser-icon-button${status.loading ? ' is-loading' : ''}`} disabled={!hasPage} title={t('刷新', 'Reload')} onClick={() => void browserRequest('reload', activeTabId).catch(fail)}><Icon name="reload" /></button>
      <form className="browser-address" onSubmit={(event) => { event.preventDefault(); void openAddress(address).catch(fail) }}><input ref={addressInput} aria-label={t('搜索或输入网址', 'Search or enter URL')} placeholder={t('搜索或输入网址', 'Search or enter URL')} value={address} spellCheck={false} autoComplete="off" onChange={(event) => setAddress(event.target.value)} onFocus={(event) => event.target.select()} /></form>
      {isXiaohongshu ? <button className="browser-collect-button" disabled={busy || status.loading} onClick={() => void perform(async () => setNotice(await onCollectXiaohongshu(activeTabId)))}>{busy ? t('采集中…', 'Collecting…') : t('采集当前页', 'Collect page')}</button> : null}
      <button className={`browser-icon-button${device ? ' active' : ''}`} title={t('显示设备工具栏', 'Show device toolbar')} onClick={() => setDevice((value) => !value)}><Icon name="device" /></button>
      <button className={`browser-icon-button${panel === 'menu' ? ' active' : ''}`} aria-expanded={panel === 'menu'} title={t('更多', 'More')} onClick={() => void showPanel(panel === 'menu' ? null : 'menu').catch(fail)}><Icon name="more" /></button>
    </header>
    {status.loading ? <div className="browser-loading-line" /> : null}
    {findOpen ? <form className="browser-find" onSubmit={(event) => { event.preventDefault(); void find(false).catch(fail) }}><Icon name="search" /><input autoFocus placeholder={t('在页面中查找', 'Find in page')} aria-label={t('在页面中查找', 'Find in page')} value={findText} onChange={(e) => { setFindText(e.target.value); setFindResult(null) }} /><button type="button" className={sensitive ? 'active' : ''} title={t('区分大小写', 'Match case')} onClick={() => setSensitive(!sensitive)}>Aa</button><span>{findResult === false ? t('未找到', 'No match') : ''}</span><button type="button" title={t('上一项', 'Previous match')} onClick={() => void find(true).catch(fail)}>↑</button><button title={t('下一项', 'Next match')}>↓</button><button type="button" title={t('关闭查找', 'Close find')} onClick={() => setFindOpen(false)}>×</button></form> : null}
    {device ? <div className="browser-device"><select aria-label={t('设备尺寸', 'Device size')} onChange={(event) => { const [width, height] = event.target.value.split('x').map(Number); if (width && height) { setDeviceWidth(width); setDeviceHeight(height) } }} defaultValue="390x844"><option value="390x844">{t('手机', 'Phone')} · 390 × 844</option><option value="768x1024">{t('平板', 'Tablet')} · 768 × 1024</option><option value="1280x800">{t('桌面', 'Desktop')} · 1280 × 800</option></select><input type="number" aria-label={t('视口宽度', 'Viewport width')} min={240} max={2560} value={deviceWidth} onChange={(e) => setDeviceWidth(Math.max(240, Math.min(2560, Number(e.target.value))))} /><span>×</span><input type="number" aria-label={t('视口高度', 'Viewport height')} min={200} max={2560} value={deviceHeight} onChange={(e) => setDeviceHeight(Math.max(200, Math.min(2560, Number(e.target.value))))} /><button title={t('旋转', 'Rotate')} onClick={() => { setDeviceWidth(deviceHeight); setDeviceHeight(deviceWidth) }}>↻</button><small>{t('视口预览', 'Viewport preview')}</small></div> : null}
    {error ? <div className="reader-error browser-notice" role="alert">{error}<button onClick={() => setError(null)}>×</button></div> : null}
    {notice ? <div className="browser-notice" role="status">{notice}<button onClick={() => setNotice(null)}>×</button></div> : null}
    <div className="browser-stage">
      <div className="reader-viewport" ref={viewport} style={device ? { width: deviceWidth, maxWidth: '100%', height: deviceHeight, maxHeight: '100%', flex: 'none' } : undefined} />
      {panel && preview ? <img className="browser-preview" src={preview} alt="" aria-hidden="true" /> : null}
      {!hasPage ? <div className="browser-start"><Icon name="globe" /><h2>{t('开始浏览', 'Start browsing')}</h2><p>{t('输入 URL 或搜索，探索更多内容', 'Enter a URL or search to explore')}</p></div> : null}
      {panel ? <div className={`browser-overlay${panel === 'menu' ? ' menu-overlay' : ''}`} onPointerDown={(e) => { if (e.target === e.currentTarget) void showPanel(null).catch(fail) }}>
        {panel === 'menu' ? <div className="browser-menu" role="menu">
          {menuItem('在页面中查找', 'Find in page', () => { void showPanel(null).then(() => setFindOpen(true)).catch(fail) }, !hasPage, '⌘F')}
          {menuItem('打印', 'Print', () => void nativeAction('print'), !hasPage)}<hr />
          <div className="browser-zoom-row"><span>{t('缩放', 'Zoom')}</span><div className="browser-zoom-controls"><button disabled={!hasPage || library.settings.zoom <= .25} onClick={() => stepZoom(-1)}>−</button><button disabled={!hasPage} title={t('重置缩放', 'Reset zoom')} onClick={() => void setZoom(1).catch(fail)}>{Math.round(library.settings.zoom * 100)}%</button><button disabled={!hasPage || library.settings.zoom >= 3} onClick={() => stepZoom(1)}>＋</button></div><button className="browser-icon-button" disabled={!hasPage} title={t('重置缩放', 'Reset zoom')} onClick={() => void setZoom(1).catch(fail)}><Icon name="reload" /></button></div><hr />
          {menuItem(device ? '隐藏设备工具栏' : '显示设备工具栏', device ? 'Hide device toolbar' : 'Show device toolbar', () => { void showPanel(null).then(() => setDevice(!device)).catch(fail) })}
          {menuItem('截取屏幕截图', 'Capture screenshot', () => void nativeAction('screenshot'), !hasPage || busy)}<hr />
          {menuItem('导入 Cookie…', 'Import cookies…', () => void showPanel('import').catch(fail))}
          {menuItem('下载', 'Downloads', () => void showPanel('downloads').catch(fail))}
          {menuItem('历史记录', 'History', () => void showPanel('history').catch(fail))}
          {menuItem('清除浏览数据', 'Clear browsing data', () => void showPanel('clear').catch(fail))}<hr />
          {menuItem('浏览器设置', 'Browser settings', () => void showPanel('settings').catch(fail))}
        </div> : <section className="browser-management" aria-label={panelTitle}>
          <header><h2>{panelTitle}</h2><button className="browser-icon-button" title={t('返回网页', 'Return to page')} onClick={() => void showPanel(null).catch(fail)}><Icon name="close" /></button></header>
          {['history', 'downloads'].includes(panel) ? <>
            <input className="browser-library-filter" aria-label={t('搜索记录', 'Search records')} placeholder={t('搜索记录…', 'Search records…')} value={filter} onChange={(e) => setFilter(e.target.value)} />
            {(panel === 'history' ? library.history : library.downloads).filter((row) => `${row.title} ${row.url}`.toLowerCase().includes(filter.toLowerCase())).map((row) => <div className="browser-record" key={row.id}><div><strong>{row.title || row.url}</strong><small>{row.url}</small><small>{new Date(row.time).toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US')}{panel === 'downloads' ? ` · ${row.detail.startsWith('complete') ? t('已完成', 'Complete') : t('失败', 'Failed')}` : ''}</small></div>{panel === 'history' ? <button onClick={() => void openAddress(row.url).catch(fail)}>{t('打开', 'Open')}</button> : <button onClick={() => void browserControl({ kind: 'revealDownload', id: row.id }).catch(fail)}>{t('显示文件', 'Show file')}</button>}</div>)}
            {(panel === 'history' ? library.history : library.downloads).length === 0 ? <p className="browser-panel-empty">{t('暂无记录', 'No records yet')}</p> : null}
          </> : null}
          {panel === 'import' ? <><p className="browser-panel-help">{t('粘贴浏览器导出的 Cookie JSON。仅导入你信任的内容。', 'Paste exported Cookie JSON. Import only trusted data.')}</p><textarea spellCheck={false} autoComplete="off" aria-label={t('导入数据', 'Import data')} placeholder='[{"name":"session","value":"…","domain":"example.com"}]' value={importText} onChange={(e) => setImportText(e.target.value)} /><button className="browser-panel-primary" disabled={busy || !importText.trim() || !hasPage} onClick={() => void perform(async () => { const result = await browserControl<{ count: number }>({ kind: 'importCookies', content: importText }); setImportText(''); await refreshLibrary(); setNotice(t(`已导入 ${result.count} 项`, `Imported ${result.count} items`)) })}>{busy ? t('正在导入…', 'Importing…') : t('导入', 'Import')}</button>{!hasPage ? <p>{t('请先打开任意网页，初始化浏览器会话。', 'Open a webpage first to initialize the browser session.')}</p> : null}</> : null}
          {panel === 'clear' ? <><p className="browser-panel-help">{t('选择要清除的数据。Cookie 与网站数据清除后可能需要重新登录；下载记录清除不会删除文件。', 'Choose what to clear. Clearing cookies and site data may sign you out. Clearing download records keeps the files.')}</p>{(['history', 'cookies', 'downloads'] as const).map((key) => <label className="browser-check" key={key}><input type="checkbox" checked={clear[key]} onChange={(e) => setClear({ ...clear, [key]: e.target.checked })} />{({ history: t('浏览历史', 'Browsing history'), cookies: t('Cookie、缓存和网站数据', 'Cookies, cache and site data'), downloads: t('下载记录', 'Download records') })[key]}</label>)}<button className="browser-panel-primary danger" disabled={busy || !Object.values(clear).some(Boolean)} onClick={() => void perform(async () => { await browserControl({ kind: 'clear', ...clear }); await refreshLibrary() }, t('所选浏览数据已清除', 'Selected browsing data cleared'))}>{t('确认清除所选数据', 'Confirm and clear selected data')}</button></> : null}
          {panel === 'settings' ? <><label className="browser-setting"><span>{t('搜索引擎', 'Search engine')}</span><select value={library.settings.searchEngine} onChange={(e) => setLibrary({ ...library, settings: { ...library.settings, searchEngine: e.target.value as BrowserSettings['searchEngine'] } })}><option value="bing">Bing</option><option value="google">Google</option><option value="duckduckgo">DuckDuckGo</option></select></label><label className="browser-setting"><span>{t('默认缩放', 'Default zoom')}</span><select value={library.settings.zoom} onChange={(e) => setLibrary({ ...library, settings: { ...library.settings, zoom: Number(e.target.value) } })}>{ZOOMS.map((zoom) => <option key={zoom} value={zoom}>{Math.round(zoom * 100)}%</option>)}</select></label><label className="browser-check"><input type="checkbox" checked={library.settings.rememberHistory} onChange={(e) => setLibrary({ ...library, settings: { ...library.settings, rememberHistory: e.target.checked } })} />{t('保存浏览历史', 'Save browsing history')}</label><button className="browser-panel-primary" disabled={busy} onClick={() => void perform(() => browserControl({ kind: 'settings', settings: library.settings }), t('设置已保存', 'Settings saved'))}>{t('保存设置', 'Save settings')}</button></> : null}
        </section>}
      </div> : null}
    </div>
  </aside>
}
