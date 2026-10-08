/** Browser chrome modeled on the reference: omnibox, native content and local tools. */
import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { ArrowDownToLine, ArrowLeft, ArrowRight, Camera, ChevronDown, ChevronUp, Cookie, EllipsisVertical, ExternalLink, File, FolderOpen, Globe2, History, Languages, LoaderCircle, Minus, MonitorSmartphone, Plus, Printer, RefreshCw, RotateCw, Search, Settings2, Trash2, X, ZoomIn } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { browserControl, browserRequest } from './api'
import type { BrowserLibrary, BrowserSettings, BrowserStatus, BrowserTranslationProgress } from './api'
import { clampDeviceDimension, isXiaohongshuAddress, nextZoom, pageTranslationRequest, parseDownloadDetail, runBrowserNavigation } from './browserPresentation'
import type { Locale } from './i18n'
import { clampReaderWidth } from './layout'
import { ReaderExpandButton } from './ReaderExpandButton'

type Panel = 'menu' | 'history' | 'downloads' | 'downloadsAll' | 'import' | 'clear' | 'settings' | null
type TabContextMenu = { tabId: string; x: number; y: number } | null
export interface BrowserTab {
  id: string
  url: string
  title: string
  pinned?: boolean
  /** Original topic identity survives redirects and browser status updates. */
  topicId?: number
  sourceUrl?: string
}
const DEFAULT_SETTINGS: BrowserSettings = { searchEngine: 'bing', zoom: 1, rememberHistory: true, translationLanguage: 'auto' }
const ZOOMS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3]

type BrowserIconName = 'back' | 'forward' | 'reload' | 'more' | 'close' | 'globe' | 'search' | 'translate' | 'downloads'
const BROWSER_ICONS: Record<BrowserIconName, LucideIcon> = {
  back: ArrowLeft,
  forward: ArrowRight,
  reload: RefreshCw,
  more: EllipsisVertical,
  close: X,
  globe: Globe2,
  search: Search,
  translate: Languages,
  downloads: ArrowDownToLine,
}

/** Browser controls use the same established icon family as the application shell. */
function Icon({ name }: { name: BrowserIconName }) {
  const Glyph = BROWSER_ICONS[name]
  return <Glyph aria-hidden="true" />
}

/** Host a native browser while React owns the address bar, overlays and layout. */
export function BrowserPane({ tabs, activeTabId, locale, expanded, closing, onActivate, onNewTab, onUpdateTab, onCloseTab, onMoveTab, onPinTab, onExpand, onCollectXiaohongshu, onClose, onResize }: {
  tabs: BrowserTab[]; activeTabId: string; locale: Locale; expanded: boolean; closing?: boolean
  onActivate: (id: string) => void; onNewTab: () => void
  onUpdateTab: (id: string, update: Partial<Pick<BrowserTab, 'url' | 'title'>>) => void
  onCloseTab: (id: string) => void
  onMoveTab: (draggedId: string, targetId: string, after: boolean) => void
  onPinTab: (id: string, pinned: boolean) => void
  onExpand: () => void
  onCollectXiaohongshu: (tabId: string) => Promise<string>
  onClose: () => void; onResize: (width: number) => void
}) {
  const viewport = useRef<HTMLDivElement>(null)
  const tabStrip = useRef<HTMLDivElement>(null)
  const addressInput = useRef<HTMLInputElement>(null)
  const alive = useRef(true)
  const tabContextVisible = useRef(false)
  const activeTabRef = useRef(activeTabId)
  const translationTokens = useRef<Record<string, string>>({})
  const createdTabs = useRef(new Set<string>())
  const tabDrag = useRef<{ id: string; pointerId: number; startX: number; startY: number; moved: boolean } | null>(null)
  const dropTargetRef = useRef<{ id: string; after: boolean } | null>(null)
  const suppressTabClick = useRef(false)
  const [statuses, setStatuses] = useState<Record<string, BrowserStatus>>({})
  const [address, setAddress] = useState('')
  const [panel, setPanel] = useState<Panel>(null)
  const [tabContextMenu, setTabContextMenu] = useState<TabContextMenu>(null)
  const [draggingTabId, setDraggingTabId] = useState<string | null>(null)
  const [dropTarget, setDropTarget] = useState<{ id: string; after: boolean } | null>(null)
  const [preview, setPreview] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [library, setLibrary] = useState<BrowserLibrary>({ history: [], downloads: [], settings: DEFAULT_SETTINGS, downloadCancellation: false })
  const [busy, setBusy] = useState(false)
  const [slowLoading, setSlowLoading] = useState(false)
  const [findOpen, setFindOpen] = useState(false)
  const [findText, setFindText] = useState('')
  const [findResult, setFindResult] = useState<boolean | null>(null)
  const [sensitive, setSensitive] = useState(false)
  const [device, setDevice] = useState(false)
  const [deviceWidth, setDeviceWidth] = useState(390)
  const [deviceHeight, setDeviceHeight] = useState(844)
  const [filter, setFilter] = useState('')
  const [translating, setTranslating] = useState(false)
  const [translatedTabs, setTranslatedTabs] = useState<Record<string, boolean>>({})
  const [translationProgress, setTranslationProgress] = useState<Record<string, BrowserTranslationProgress>>({})
  const [importText, setImportText] = useState('')
  const [clear, setClear] = useState({ history: true, cookies: false, downloads: false })
  const zh = locale === 'zh'
  const t = (cn: string, en: string): string => zh ? cn : en
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0]
  const status = statuses[activeTabId] ?? { tabId: activeTabId, url: activeTab?.url ?? '', title: activeTab?.title ?? '', loading: false, canBack: false, canForward: false, muted: false }
  const pageProgress = translationProgress[activeTabId]
  const pendingTranslation = (pageProgress?.queued ?? 0) + (pageProgress?.active ?? 0)
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
    const startingStatus = { ...status, tabId: activeTabId, loading: true }
    await runBrowserNavigation(
      () => browserRequest('sync', activeTabId, area, input),
      () => { if (alive.current) setStatuses((old) => ({ ...old, [activeTabId]: startingStatus })) },
      () => { if (alive.current) setStatuses((old) => old[activeTabId] === startingStatus
        ? { ...old, [activeTabId]: { ...startingStatus, loading: false } } : old) },
    )
    createdTabs.current.add(activeTabId)
    // Native events own the canonical URL and final state, including redirects.
  }, [activeTabId, status])

  useEffect(() => {
    alive.current = true
    const unlisten = listen<BrowserStatus>('browser-status', ({ payload }) => {
      if (!alive.current) return
      setStatuses((old) => ({ ...old, [payload.tabId]: payload }))
      if (payload.loading) {
        delete translationTokens.current[payload.tabId]
        setTranslatedTabs((old) => ({ ...old, [payload.tabId]: false }))
        setTranslationProgress((old) => { const next = { ...old }; delete next[payload.tabId]; return next })
      }
      onUpdateTab(payload.tabId, { url: payload.url, ...(payload.title ? { title: payload.title } : {}) })
      // Native navigation can occur while the omnibox remains the nominally
      // focused React element (for example after a link click in the child
      // WebView). Always accept the canonical top-level URL from Rust.
      if (payload.tabId === activeTabRef.current) setAddress(payload.url)
    })
    const unlibrary = listen('browser-library-changed', () => { void refreshLibrary().catch(fail) })
    const untranslation = listen<BrowserTranslationProgress>('browser-translation-progress', ({ payload }) => {
      if (alive.current && translationTokens.current[payload.tabId] === payload.token) {
        setTranslationProgress((old) => ({ ...old, [payload.tabId]: payload }))
      }
    })
    const unpopup = listen<{ tabId: string; url: string }>('browser-open-popup', ({ payload }) => {
      const area = bounds()
      if (alive.current && area) void browserRequest('sync', payload.tabId, area, payload.url).catch(fail)
    })
    void refreshLibrary().catch(fail)
    return () => {
      alive.current = false
      void unlisten.then((stop) => stop()).catch(() => {})
      void unlibrary.then((stop) => stop()).catch(() => {})
      void untranslation.then((stop) => stop()).catch(() => {})
      void unpopup.then((stop) => stop()).catch(() => {})
      void browserRequest('hideAll').catch(() => {})
    }
  }, [fail, onUpdateTab, refreshLibrary])

  useEffect(() => {
    if (notice === null) return
    const timer = window.setTimeout(() => setNotice(null), 4000)
    return () => window.clearTimeout(timer)
  }, [notice])

  useEffect(() => {
    if (error === null) return
    const timer = window.setTimeout(() => setError(null), 4000)
    return () => window.clearTimeout(timer)
  }, [error])

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
    // New tabs and explicit address changes navigate; matching URLs only
    // synchronize geometry and preserve native navigation history.
    const firstUrl = activeTab.url && (
      !createdTabs.current.has(activeTab.id) || activeTab.url !== status.url
    ) ? activeTab.url : undefined
    if (firstUrl) {
      setStatuses((old) => ({ ...old, [activeTab.id]: { ...status, tabId: activeTab.id, url: firstUrl, loading: true } }))
    }
    void browserRequest('sync', activeTab.id, area, firstUrl).then(() => {
      if (firstUrl) createdTabs.current.add(activeTab.id)
    }).catch(fail)
  }, [activeTab?.id, activeTab?.url, fail, status.url])

  useEffect(() => {
    setSlowLoading(false)
    if (!status.loading || !status.url) return
    const timer = window.setTimeout(() => setSlowLoading(true), 8_000)
    return () => window.clearTimeout(timer)
  }, [activeTabId, status.loading, status.url])

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
    const factor = nextZoom(current, direction, ZOOMS)
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
  const translatePage = async () => {
    await showPanel(null)
    setTranslating(true)
    try {
      await perform(async () => {
        if (translatedTabs[activeTabId]) await browserControl({ kind: 'restorePageTranslation' })
        delete translationTokens.current[activeTabId]
        const result = await browserControl<{ started: boolean; token: string; queued: number }>(pageTranslationRequest(library.settings))
        translationTokens.current[activeTabId] = result.token
        setTranslationProgress((old) => ({ ...old, [activeTabId]: {
          tabId: activeTabId, token: result.token, queued: result.queued, deferred: 0,
          active: 0, completed: 0, failed: 0, running: true,
        } }))
        setTranslatedTabs((old) => ({ ...old, [activeTabId]: true }))
        setNotice(t('已开启按需翻译，滚动到的内容会自动翻译', 'On-demand translation enabled. Visible content translates as you scroll.'))
      })
    } finally {
      setTranslating(false)
    }
  }
  const restorePageTranslation = async () => {
    await showPanel(null)
    await perform(async () => {
      await browserControl({ kind: 'restorePageTranslation' })
      delete translationTokens.current[activeTabId]
      setTranslationProgress((old) => { const next = { ...old }; delete next[activeTabId]; return next })
      setTranslatedTabs((old) => ({ ...old, [activeTabId]: false }))
      setNotice(t('已显示原文', 'Original page restored'))
    })
  }
  const revealDownload = async (id: number) => {
    await browserControl({ kind: 'revealDownload', id })
  }

  /** Native child WebViews render above React, so hide the active page while
   * the tab context menu crosses into its bounds. */
  const dismissTabContextMenu = useCallback(() => {
    if (!tabContextVisible.current) return
    tabContextVisible.current = false
    setTabContextMenu(null)
    void browserControl({ kind: 'overlay', visible: false }).catch(fail)
  }, [fail])

  const openTabContextMenu = (tabId: string, x: number, y: number) => {
    tabContextVisible.current = true
    setTabContextMenu({ tabId, x, y })
    void browserControl({ kind: 'overlay', visible: true, capture: false }).catch(fail)
  }

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'l') { event.preventDefault(); addressInput.current?.focus(); addressInput.current?.select() }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') { event.preventDefault(); setFindOpen(true) }
      if (event.key === 'Escape') { dismissTabContextMenu(); if (panel) void showPanel(null).catch(fail); else setFindOpen(false) }
    }
    window.addEventListener('keydown', keydown)
    return () => window.removeEventListener('keydown', keydown)
  }, [dismissTabContextMenu, fail, panel])

  useEffect(() => {
    const dismiss = (event: PointerEvent) => {
      const target = event.target
      if (!(target instanceof Element) || !target.closest('.browser-tab-context-menu')) dismissTabContextMenu()
    }
    const resize = () => dismissTabContextMenu()
    document.addEventListener('pointerdown', dismiss)
    window.addEventListener('resize', resize)
    window.addEventListener('blur', resize)
    return () => {
      document.removeEventListener('pointerdown', dismiss)
      window.removeEventListener('resize', resize)
      window.removeEventListener('blur', resize)
    }
  }, [dismissTabContextMenu])

  /** Close native views and React tabs together so bulk context-menu actions stay consistent. */
  const closeTabs = (ids: string[]) => {
    dismissTabContextMenu()
    void (async () => {
      for (const id of ids) {
        await browserRequest('close', id).catch(fail)
        onCloseTab(id)
      }
    })()
  }

  const toggleTabMute = (tabId: string) => {
    const muted = !(statuses[tabId]?.muted ?? false)
    dismissTabContextMenu()
    void browserRequest(muted ? 'mute' : 'unmute', tabId).catch(fail)
  }

  const finishTabDrag = () => {
    tabDrag.current = null
    dropTargetRef.current = null
    setDraggingTabId(null)
    setDropTarget(null)
  }

  /** Pointer capture keeps an active drag reliable on macOS WebKit. It must
   * start only after the movement threshold, because capturing on pointer-down
   * can retarget the synthesized click away from the tab button. */
  const locateTabDropTarget = (clientX: number, clientY: number, draggedId: string): { id: string; after: boolean } | null => {
    const strip = tabStrip.current
    if (!strip) return null
    const stripRect = strip.getBoundingClientRect()
    if (clientY < stripRect.top - 8 || clientY > stripRect.bottom + 8) return null
    const candidates = Array.from(strip.querySelectorAll<HTMLElement>('[data-browser-tab-id]'))
      .filter((element) => element.dataset.browserTabId !== draggedId)
    if (candidates.length === 0) return null
    for (const element of candidates) {
      const rect = element.getBoundingClientRect()
      if (clientX < rect.left + rect.width / 2) {
        return { id: element.dataset.browserTabId!, after: false }
      }
    }
    const last = candidates.at(-1)
    return last?.dataset.browserTabId ? { id: last.dataset.browserTabId, after: true } : null
  }

  const menuItem = (Glyph: LucideIcon, cn: string, en: string, operation: () => void, disabled = false, hint?: string) => <button className="browser-menu-item" type="button" disabled={disabled} onClick={operation}><span className="browser-menu-label"><Glyph aria-hidden="true" /><span>{t(cn, en)}</span></span>{hint ? <small>{hint}</small> : null}</button>
  const panelTitle = panel === 'history' ? t('历史记录', 'History') : panel === 'downloadsAll' ? t('完整下载记录', 'All downloads') : panel === 'import' ? t('导入 Cookie', 'Import cookies') : panel === 'clear' ? t('清除浏览数据', 'Clear browsing data') : t('浏览器设置', 'Browser settings')
  const recentDownloads = library.downloads.slice(0, 8)
  const activeDownloads = library.downloads.filter((row) => parseDownloadDetail(row.detail).state === 'downloading')
  const finishedDownloads = recentDownloads.filter((row) => parseDownloadDetail(row.detail).state !== 'downloading')
  const hasPage = status.url !== ''
  const isXiaohongshu = isXiaohongshuAddress(status.url)

  return <aside className={`browser-pane browser-chrome${device ? ' device-mode' : ''}${closing ? ' browser-pane-closing' : ''}`} aria-label={t('内置浏览器', 'Browser')}>
    <div className="reader-divider" role="separator" aria-label={t('调整浏览器宽度', 'Resize browser')} aria-orientation="vertical" tabIndex={0}
      onKeyDown={(event) => {
        const workspace = event.currentTarget.parentElement?.parentElement
        if (!workspace || !['ArrowLeft', 'ArrowRight'].includes(event.key)) return
        event.preventDefault()
        const width = event.currentTarget.parentElement!.getBoundingClientRect().width / workspace.getBoundingClientRect().width * 100
        onResize(clampReaderWidth(width + (event.key === 'ArrowLeft' ? 3 : -3)))
      }}
      onPointerDown={(event) => event.currentTarget.setPointerCapture(event.pointerId)}
      onPointerMove={(event) => {
        if (!event.currentTarget.hasPointerCapture(event.pointerId)) return
        const workspace = event.currentTarget.parentElement?.parentElement?.getBoundingClientRect()
        if (workspace) onResize(clampReaderWidth((workspace.right - event.clientX) / workspace.width * 100))
      }} onPointerUp={(event) => event.currentTarget.releasePointerCapture(event.pointerId)} />
    <div className="browser-tabbar">
      <div ref={tabStrip} className="browser-tabs" role="tablist" aria-label={t('浏览器标签页', 'Browser tabs')}>
        {tabs.map((tab) => {
          const target = dropTarget?.id === tab.id ? (dropTarget.after ? ' drop-after' : ' drop-before') : ''
          return <div
            className={`browser-tab${tab.id === activeTabId ? ' active' : ''}${tab.pinned ? ' pinned' : ''}${draggingTabId === tab.id ? ' dragging' : ''}${target}`}
            key={tab.id}
            data-browser-tab-id={tab.id}
            onPointerDown={(event) => {
              if (event.button !== 0 || (event.target instanceof Element && event.target.closest('.browser-tab-close'))) return
              tabDrag.current = { id: tab.id, pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, moved: false }
            }}
            onPointerMove={(event) => {
              const drag = tabDrag.current
              if (!drag || drag.pointerId !== event.pointerId) return
              if (!drag.moved && Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < 6) return
              if (!drag.moved) event.currentTarget.setPointerCapture(event.pointerId)
              drag.moved = true
              event.preventDefault()
              setDraggingTabId(drag.id)
              const nextTarget = locateTabDropTarget(event.clientX, event.clientY, drag.id)
              if (!nextTarget) {
                dropTargetRef.current = null
                setDropTarget(null)
                return
              }
              dropTargetRef.current = nextTarget
              setDropTarget(nextTarget)
            }}
            onPointerUp={(event) => {
              const drag = tabDrag.current
              const destination = dropTargetRef.current
              if (drag?.moved) {
                if (destination) onMoveTab(drag.id, destination.id, destination.after)
                suppressTabClick.current = true
                window.setTimeout(() => { suppressTabClick.current = false }, 0)
              }
              if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
              finishTabDrag()
            }}
            onPointerCancel={finishTabDrag}
            onContextMenu={(event) => {
              event.preventDefault()
              event.stopPropagation()
              openTabContextMenu(tab.id, Math.min(event.clientX, window.innerWidth - 200), Math.min(event.clientY, window.innerHeight - 210))
            }}
          >
            <button role="tab" aria-selected={tab.id === activeTabId} title={tab.title || tab.url || t('新标签页', 'New tab')} onClick={(event) => { if (suppressTabClick.current) event.preventDefault(); else onActivate(tab.id) }}><Icon name="globe" /><span>{tab.title || t('新标签页', 'New tab')}</span></button>
            <button className="browser-tab-close" aria-label={t('关闭标签页', 'Close tab')} title={t('关闭标签页', 'Close tab')} onClick={() => closeTabs([tab.id])}><X aria-hidden="true" /></button>
          </div>
        })}
        <button className="browser-new-tab" aria-label={t('新建标签页', 'New tab')} title={t('新建标签页', 'New tab')} onClick={onNewTab}><Plus aria-hidden="true" /></button>
      </div>
      <div className="browser-tab-actions"><ReaderExpandButton expanded={expanded} locale={locale} onToggle={onExpand} /><button className="browser-icon-button" title={t('隐藏浏览器', 'Hide browser')} onClick={onClose}><Icon name="close" /></button></div>
    </div>
    {tabContextMenu ? (() => {
      const index = tabs.findIndex((tab) => tab.id === tabContextMenu.tabId)
      const contextTab = index < 0 ? undefined : tabs[index]
      const rightTabs = index < 0 ? [] : tabs.slice(index + 1).map((tab) => tab.id)
      const otherTabs = tabs.filter((tab) => tab.id !== tabContextMenu.tabId).map((tab) => tab.id)
      const muted = statuses[tabContextMenu.tabId]?.muted ?? false
      return <div className="browser-tab-context-menu" role="menu" aria-label={t('标签页菜单', 'Tab menu')} style={{ left: tabContextMenu.x, top: tabContextMenu.y }} onContextMenu={(event) => event.preventDefault()}>
        <button role="menuitem" onClick={() => closeTabs([tabContextMenu.tabId])}>{t('关闭', 'Close')}</button>
        <button role="menuitem" disabled={otherTabs.length === 0} onClick={() => { onActivate(tabContextMenu.tabId); closeTabs(otherTabs) }}>{t('关闭其他标签页', 'Close other tabs')}</button>
        <button role="menuitem" disabled={rightTabs.length === 0} onClick={() => closeTabs(rightTabs)}>{t('关闭右侧标签页', 'Close tabs to the right')}</button>
        <hr />
        <button role="menuitemcheckbox" aria-checked={contextTab?.pinned === true} onClick={() => { onPinTab(tabContextMenu.tabId, contextTab?.pinned !== true); dismissTabContextMenu() }}>{contextTab?.pinned ? t('取消固定标签页', 'Unpin tab') : t('固定标签页', 'Pin tab')}</button>
        <button role="menuitemcheckbox" aria-checked={muted} disabled={!contextTab?.url} onClick={() => toggleTabMute(tabContextMenu.tabId)}>{muted ? t('取消网站静音', 'Unmute site') : t('将网站静音', 'Mute site')}</button>
      </div>
    })() : null}
    <header className="browser-toolbar">
      <button className="browser-icon-button" disabled={!status.canBack} title={t('后退', 'Back')} onClick={() => void browserRequest('back', activeTabId).catch(fail)}><Icon name="back" /></button>
      <button className="browser-icon-button" disabled={!status.canForward} title={t('前进', 'Forward')} onClick={() => void browserRequest('forward', activeTabId).catch(fail)}><Icon name="forward" /></button>
      <button className={`browser-icon-button${status.loading ? ' is-loading' : ''}`} disabled={!hasPage} title={t('刷新', 'Reload')} onClick={() => void browserRequest('reload', activeTabId).catch(fail)}><Icon name="reload" /></button>
      <form className="browser-address" onSubmit={(event) => { event.preventDefault(); void openAddress(address).catch(fail) }}><Search className="browser-address-icon" aria-hidden="true" /><input ref={addressInput} aria-label={t('搜索或输入网址', 'Search or enter URL')} placeholder={t('搜索或输入网址', 'Search or enter URL')} value={address} spellCheck={false} autoComplete="off" onChange={(event) => setAddress(event.target.value)} onFocus={(event) => event.target.select()} /></form>
      {isXiaohongshu ? <button className="browser-collect-button" disabled={busy || status.loading} onClick={() => void perform(async () => setNotice(await onCollectXiaohongshu(activeTabId)))}>{busy ? t('采集中…', 'Collecting…') : t('采集当前页', 'Collect page')}</button> : null}
      <button className={`browser-icon-button${translatedTabs[activeTabId] ? ' active' : ''}${pendingTranslation > 0 ? ' is-loading' : ''}`} disabled={busy || !hasPage} aria-busy={pendingTranslation > 0} title={pageProgress ? t(`已译 ${pageProgress.completed} · 处理中 ${pageProgress.active} · 排队 ${pageProgress.queued} · 滚动后翻译 ${pageProgress.deferred} · 失败 ${pageProgress.failed}`, `Translated ${pageProgress.completed} · active ${pageProgress.active} · queued ${pageProgress.queued} · deferred ${pageProgress.deferred} · failed ${pageProgress.failed}`) : t('按设置的目标语言翻译页面', 'Translate page to the configured language')} onClick={() => void translatePage()}>{translating && !pageProgress ? <LoaderCircle className="translation-spinner" aria-hidden="true" /> : <Icon name="translate" />}{pendingTranslation > 0 ? <span className="browser-translation-count" aria-hidden="true">{pendingTranslation > 99 ? '99+' : pendingTranslation}</span> : null}</button>
      <button className={`browser-icon-button browser-download-trigger${panel === 'downloads' ? ' active' : ''}${activeDownloads.length > 0 ? ' downloading' : ''}`} aria-expanded={panel === 'downloads'} aria-label={activeDownloads.length > 0 ? t(`正在下载 ${activeDownloads.length} 项`, `${activeDownloads.length} download${activeDownloads.length === 1 ? '' : 's'} in progress`) : t('下载', 'Downloads')} title={activeDownloads.length > 0 ? t(`正在下载 ${activeDownloads.length} 项`, `${activeDownloads.length} download${activeDownloads.length === 1 ? '' : 's'} in progress`) : t('下载', 'Downloads')} onClick={() => void showPanel(panel === 'downloads' ? null : 'downloads').catch(fail)}><Icon name="downloads" /></button>
      <button className={`browser-icon-button${panel === 'menu' ? ' active' : ''}`} aria-expanded={panel === 'menu'} title={t('更多', 'More')} onClick={() => void showPanel(panel === 'menu' ? null : 'menu').catch(fail)}><Icon name="more" /></button>
    </header>
    {status.loading ? <div className="browser-loading-line" /> : null}
    {slowLoading ? <div className="browser-slow-notice" role="status"><span>{t('目标网站响应较慢，请检查网络后重试。', 'The website is responding slowly. Check your network and try again.')}</span><button type="button" onClick={() => void browserRequest('reload', activeTabId).catch(fail)}>{t('重试', 'Retry')}</button></div> : null}
    {findOpen ? <form className="browser-find" onSubmit={(event) => { event.preventDefault(); void find(false).catch(fail) }}><Icon name="search" /><input autoFocus placeholder={t('在页面中查找', 'Find in page')} aria-label={t('在页面中查找', 'Find in page')} value={findText} onChange={(e) => { setFindText(e.target.value); setFindResult(null) }} /><button type="button" className={sensitive ? 'active' : ''} title={t('区分大小写', 'Match case')} onClick={() => setSensitive(!sensitive)}>Aa</button><span>{findResult === false ? t('未找到', 'No match') : ''}</span><button type="button" aria-label={t('上一项', 'Previous match')} title={t('上一项', 'Previous match')} onClick={() => void find(true).catch(fail)}><ChevronUp aria-hidden="true" /></button><button aria-label={t('下一项', 'Next match')} title={t('下一项', 'Next match')}><ChevronDown aria-hidden="true" /></button><button type="button" aria-label={t('关闭查找', 'Close find')} title={t('关闭查找', 'Close find')} onClick={() => setFindOpen(false)}><X aria-hidden="true" /></button></form> : null}
    {device ? <div className="browser-device"><select aria-label={t('设备尺寸', 'Device size')} onChange={(event) => { const [width, height] = event.target.value.split('x').map(Number); if (width && height) { setDeviceWidth(width); setDeviceHeight(height) } }} defaultValue="390x844"><option value="390x844">{t('手机', 'Phone')} · 390 × 844</option><option value="768x1024">{t('平板', 'Tablet')} · 768 × 1024</option><option value="1280x800">{t('桌面', 'Desktop')} · 1280 × 800</option></select><input type="number" aria-label={t('视口宽度', 'Viewport width')} min={240} max={2560} value={deviceWidth} onChange={(e) => setDeviceWidth(clampDeviceDimension(Number(e.target.value), 240))} /><span>×</span><input type="number" aria-label={t('视口高度', 'Viewport height')} min={200} max={2560} value={deviceHeight} onChange={(e) => setDeviceHeight(clampDeviceDimension(Number(e.target.value), 200))} /><button aria-label={t('旋转', 'Rotate')} title={t('旋转', 'Rotate')} onClick={() => { setDeviceWidth(deviceHeight); setDeviceHeight(deviceWidth) }}><RotateCw aria-hidden="true" /></button><small>{t('视口预览', 'Viewport preview')}</small></div> : null}
    {error || notice ? <div className="toast-stack" aria-live="polite">
      {error ? <div key={error} className="error-banner app-toast" role="alert"><span>{error}</span><button aria-label={t('关闭', 'Close')} onClick={() => setError(null)}><X aria-hidden="true" /></button></div> : null}
      {notice ? <div key={notice} className="notice app-toast" role="status"><span>{notice}</span><button aria-label={t('关闭', 'Close')} onClick={() => setNotice(null)}><X aria-hidden="true" /></button></div> : null}
    </div> : null}
    <div className="browser-stage">
      <div className="reader-viewport" ref={viewport} style={device ? { width: deviceWidth, maxWidth: '100%', height: deviceHeight, maxHeight: '100%', flex: 'none' } : undefined} />
      {hasPage && status.loading ? <div className="browser-page-loading" aria-label={t('正在打开网页', 'Opening webpage')}><span /><p>{t('正在打开网页…', 'Opening webpage…')}</p></div> : null}
      {panel && preview ? <img className="browser-preview" src={preview} alt="" aria-hidden="true" /> : null}
      {!hasPage ? <div className="browser-start"><Icon name="globe" /><h2>{t('开始浏览', 'Start browsing')}</h2><p>{t('输入 URL 或搜索，探索更多内容', 'Enter a URL or search to explore')}</p></div> : null}
      {panel ? <div className={`browser-overlay${panel === 'menu' ? ' menu-overlay' : ''}${panel === 'downloads' ? ' downloads-overlay' : ''}`} onPointerDown={(e) => { if (e.target === e.currentTarget) void showPanel(null).catch(fail) }}>
        {panel === 'downloads' ? <section className="browser-download-popover" aria-label={t('下载', 'Downloads')}>
          <header><h2>{t('下载', 'Downloads')}</h2><button className="browser-icon-button" aria-label={t('关闭', 'Close')} title={t('关闭', 'Close')} onClick={() => void showPanel(null).catch(fail)}><X aria-hidden="true" /></button></header>
          {activeDownloads.length > 0 ? <div className="browser-download-section"><h3>{t('近期的下载记录', 'Recent downloads')}</h3>{activeDownloads.map((row) => <div className="browser-download-item downloading" key={row.id}>
            <span className="browser-download-file"><File aria-hidden="true" /></span><div className="browser-download-copy"><strong title={row.title || row.url}>{row.title || row.url}</strong><small>{t('正在下载…', 'Downloading…')}</small><span className="browser-download-progress" aria-hidden="true"><i /></span></div>{library.downloadCancellation ? <button className="browser-download-action danger" aria-label={t('取消下载', 'Cancel download')} title={t('取消下载', 'Cancel download')} onClick={() => void browserControl({ kind: 'cancelDownload', id: row.id }).catch(fail)}><X aria-hidden="true" /></button> : null}
          </div>)}</div> : null}
          {finishedDownloads.length > 0 ? <div className="browser-download-section"><h3>{activeDownloads.length > 0 ? t('已完成', 'Completed') : t('最近下载', 'Recent downloads')}</h3>{finishedDownloads.map((row) => {
            const detail = parseDownloadDetail(row.detail)
            const complete = detail.state === 'complete'
            return <div className={`browser-download-item ${complete ? 'complete' : 'failed'}`} key={row.id}>
              <span className="browser-download-file"><File aria-hidden="true" /></span><button className="browser-download-copy" disabled={!complete} title={complete ? t('打开文件', 'Open file') : undefined} onClick={() => void browserControl({ kind: 'openDownload', id: row.id }).catch(fail)}><strong title={row.title || row.url}>{row.title || row.url}</strong><small>{complete ? t('已完成', 'Complete') : detail.state === 'cancelled' ? t('已取消', 'Cancelled') : t('下载失败', 'Download failed')} · {new Date(row.time).toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US')}</small></button>{complete ? <button className="browser-download-action folder" aria-label={t('打开文件所在文件夹', 'Open containing folder')} title={t('打开文件所在文件夹', 'Open containing folder')} onClick={() => void revealDownload(row.id).catch(fail)}><FolderOpen aria-hidden="true" /></button> : null}
            </div>
          })}</div> : null}
          {recentDownloads.length === 0 ? <p className="browser-download-empty">{t('暂无下载记录', 'No downloads yet')}</p> : null}
          <footer><button type="button" onClick={() => void showPanel('downloadsAll').catch(fail)}>{t('完整的下载记录', 'All downloads')}<ExternalLink aria-hidden="true" /></button></footer>
        </section> : panel === 'menu' ? <div className="browser-menu" role="menu">
          {menuItem(Search, '在页面中查找', 'Find in page', () => { void showPanel(null).then(() => setFindOpen(true)).catch(fail) }, !hasPage, '⌘F')}
          {translatedTabs[activeTabId] ? menuItem(Languages, '显示页面原文', 'Show original page', () => void restorePageTranslation(), busy) : null}
          {menuItem(Printer, '打印', 'Print', () => void nativeAction('print'), !hasPage)}<hr />
          <div className="browser-zoom-row"><span className="browser-menu-label"><ZoomIn aria-hidden="true" /><span>{t('缩放', 'Zoom')}</span></span><div className="browser-zoom-controls"><button aria-label={t('缩小', 'Zoom out')} disabled={!hasPage || library.settings.zoom <= .25} onClick={() => stepZoom(-1)}><Minus aria-hidden="true" /></button><button disabled={!hasPage} title={t('重置缩放', 'Reset zoom')} onClick={() => void setZoom(1).catch(fail)}>{Math.round(library.settings.zoom * 100)}%</button><button aria-label={t('放大', 'Zoom in')} disabled={!hasPage || library.settings.zoom >= 3} onClick={() => stepZoom(1)}><Plus aria-hidden="true" /></button></div><button className="browser-icon-button" disabled={!hasPage} title={t('重置缩放', 'Reset zoom')} onClick={() => void setZoom(1).catch(fail)}><Icon name="reload" /></button></div><hr />
          {menuItem(MonitorSmartphone, device ? '隐藏设备工具栏' : '显示设备工具栏', device ? 'Hide device toolbar' : 'Show device toolbar', () => { void showPanel(null).then(() => setDevice(!device)).catch(fail) })}
          {menuItem(Camera, '截取屏幕截图', 'Capture screenshot', () => void nativeAction('screenshot'), !hasPage || busy)}<hr />
          {menuItem(Cookie, '导入 Cookie…', 'Import cookies…', () => void showPanel('import').catch(fail))}
          {menuItem(ArrowDownToLine, '下载', 'Downloads', () => void showPanel('downloads').catch(fail))}
          {menuItem(History, '历史记录', 'History', () => void showPanel('history').catch(fail))}
          {menuItem(Trash2, '清除浏览数据', 'Clear browsing data', () => void showPanel('clear').catch(fail))}<hr />
          {menuItem(Settings2, '浏览器设置', 'Browser settings', () => void showPanel('settings').catch(fail))}
        </div> : <section className="browser-management" aria-label={panelTitle}>
          <header><h2>{panelTitle}</h2><button className="browser-icon-button" title={t('返回网页', 'Return to page')} onClick={() => void showPanel(null).catch(fail)}><Icon name="close" /></button></header>
          {['history', 'downloadsAll'].includes(panel) ? <>
            <input className="browser-library-filter" aria-label={t('搜索记录', 'Search records')} placeholder={t('搜索记录…', 'Search records…')} value={filter} onChange={(e) => setFilter(e.target.value)} />
            {(panel === 'history' ? library.history : library.downloads).filter((row) => `${row.title} ${row.url}`.toLowerCase().includes(filter.toLowerCase())).map((row) => <div className="browser-record" key={row.id}><div><strong>{row.title || row.url}</strong><small>{row.url}</small><small>{new Date(row.time).toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US')}{panel === 'downloadsAll' ? ` · ${parseDownloadDetail(row.detail).state === 'complete' ? t('已完成', 'Complete') : parseDownloadDetail(row.detail).state === 'downloading' ? t('正在下载', 'Downloading') : parseDownloadDetail(row.detail).state === 'cancelled' ? t('已取消', 'Cancelled') : t('失败', 'Failed')}` : ''}</small></div>{panel === 'history' ? <button onClick={() => void openAddress(row.url).catch(fail)}>{t('打开', 'Open')}</button> : parseDownloadDetail(row.detail).state === 'complete' ? <button className="browser-record-folder" aria-label={t('打开文件所在文件夹', 'Open containing folder')} title={t('打开文件所在文件夹', 'Open containing folder')} onClick={() => void revealDownload(row.id).catch(fail)}><FolderOpen aria-hidden="true" /></button> : parseDownloadDetail(row.detail).state === 'downloading' && library.downloadCancellation ? <button onClick={() => void browserControl({ kind: 'cancelDownload', id: row.id }).catch(fail)}>{t('取消', 'Cancel')}</button> : null}</div>)}
            {(panel === 'history' ? library.history : library.downloads).length === 0 ? <p className="browser-panel-empty">{t('暂无记录', 'No records yet')}</p> : null}
          </> : null}
          {panel === 'import' ? <><p className="browser-panel-help">{t('粘贴浏览器导出的 Cookie JSON。仅导入你信任的内容。', 'Paste exported Cookie JSON. Import only trusted data.')}</p><textarea spellCheck={false} autoComplete="off" aria-label={t('导入数据', 'Import data')} placeholder='[{"name":"session","value":"…","domain":"example.com"}]' value={importText} onChange={(e) => setImportText(e.target.value)} /><button className="browser-panel-primary" disabled={busy || !importText.trim() || !hasPage} onClick={() => void perform(async () => { const result = await browserControl<{ count: number }>({ kind: 'importCookies', content: importText }); setImportText(''); await refreshLibrary(); setNotice(t(`已导入 ${result.count} 项`, `Imported ${result.count} items`)) })}>{busy ? t('正在导入…', 'Importing…') : t('导入', 'Import')}</button>{!hasPage ? <p>{t('请先打开任意网页，初始化浏览器会话。', 'Open a webpage first to initialize the browser session.')}</p> : null}</> : null}
          {panel === 'clear' ? <><p className="browser-panel-help">{t('选择要清除的数据。Cookie 与网站数据清除后可能需要重新登录；下载记录清除不会删除文件。', 'Choose what to clear. Clearing cookies and site data may sign you out. Clearing download records keeps the files.')}</p>{(['history', 'cookies', 'downloads'] as const).map((key) => <label className="browser-check" key={key}><input type="checkbox" checked={clear[key]} onChange={(e) => setClear({ ...clear, [key]: e.target.checked })} />{({ history: t('浏览历史', 'Browsing history'), cookies: t('Cookie、缓存和网站数据', 'Cookies, cache and site data'), downloads: t('下载记录', 'Download records') })[key]}</label>)}<button className="browser-panel-primary danger" disabled={busy || !Object.values(clear).some(Boolean)} onClick={() => void perform(async () => { await browserControl({ kind: 'clear', ...clear }); await refreshLibrary() }, t('所选浏览数据已清除', 'Selected browsing data cleared'))}>{t('确认清除所选数据', 'Confirm and clear selected data')}</button></> : null}
          {panel === 'settings' ? <><label className="browser-setting"><span>{t('搜索引擎', 'Search engine')}</span><select value={library.settings.searchEngine} onChange={(e) => setLibrary({ ...library, settings: { ...library.settings, searchEngine: e.target.value as BrowserSettings['searchEngine'] } })}><option value="bing">Bing</option><option value="google">Google</option><option value="duckduckgo">DuckDuckGo</option></select></label><label className="browser-setting"><span>{t('页面翻译目标语言', 'Page translation language')}</span><select value={library.settings.translationLanguage} onChange={(e) => setLibrary({ ...library, settings: { ...library.settings, translationLanguage: e.target.value as BrowserSettings['translationLanguage'] } })}><option value="auto">{t('自动（中文 ↔ English）', 'Auto (Chinese ↔ English)')}</option><option value="zh-CN">简体中文</option><option value="en">English</option><option value="ja">日本語</option><option value="ko">한국어</option><option value="fr">Français</option><option value="de">Deutsch</option><option value="es">Español</option><option value="ru">Русский</option></select></label><label className="browser-setting"><span>{t('默认缩放', 'Default zoom')}</span><select value={library.settings.zoom} onChange={(e) => setLibrary({ ...library, settings: { ...library.settings, zoom: Number(e.target.value) } })}>{ZOOMS.map((zoom) => <option key={zoom} value={zoom}>{Math.round(zoom * 100)}%</option>)}</select></label><label className="browser-check"><input type="checkbox" checked={library.settings.rememberHistory} onChange={(e) => setLibrary({ ...library, settings: { ...library.settings, rememberHistory: e.target.checked } })} />{t('保存浏览历史', 'Save browsing history')}</label><button className="browser-panel-primary" disabled={busy} onClick={() => void perform(() => browserControl({ kind: 'settings', settings: library.settings }), t('设置已保存', 'Settings saved'))}>{t('保存设置', 'Save settings')}</button></> : null}
        </section>}
      </div> : null}
    </div>
  </aside>
}
