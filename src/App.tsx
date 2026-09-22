/** Topic Desk Studio — main UI component. */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Archive, ArrowDown, ArrowLeft, ArrowUp, Bot, Bookmark, ChevronDown, Compass, Database, FolderOpen, HardDrive, Network, PanelRight, Radar, RefreshCw, RotateCcw, Search, Settings, SlidersHorizontal, Sparkles, Wrench } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import {
  backupStorage, browserRequest, collectXiaohongshuSession, getModelSettings, getNetworkSettings,
  getStorageStatus, getUiPreferences, listTopics, openDataDirectory, optimizeStorage, refreshTopics,
  restoreLatestBackup, saveModelSettings, saveNetworkSettings, saveUiPreferences, setPlatformEnabled,
  setTopicQueued, translateTopic,
} from './api'
import { BrowserPane } from './BrowserPane'
import type { BrowserTab } from './BrowserPane'
import { isEnglishTitle, rankTrendPoints } from './presentation'
import {
  Locale, Theme, Messages, messages,
  detectLocale, detectTheme, applyTheme,
} from './i18n'
import type { ModelSettings, SourceRegion, StorageStatus, TopicCategory, TopicPage, TopicQuery, TopicView } from './types'

const PAGE_SIZE = 20
type ViewMode = 'discover' | 'queue' | 'new' | 'settings'
type SettingsTab = 'general' | 'network' | 'sources' | 'model' | 'storage'
interface TranslationState { readonly loading?: boolean; readonly text?: string; readonly error?: string }

function formatTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  }).format(new Date(value))
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${(value / 1024 / 1024).toFixed(1)} MB`
}

function formatBackupName(value: string | null, fallback: string): string {
  if (value === null) return fallback
  const seconds = Number(value.replace(/^backup-/, ''))
  return Number.isFinite(seconds) ? new Date(seconds * 1000).toLocaleString() : value
}

/** Compare article URLs without cosmetic host, fragment, or trailing-slash differences. */
function normalizedArticleUrl(value: string): string {
  try {
    const url = new URL(value)
    url.hash = ''
    url.hostname = url.hostname.replace(/^www\./, '')
    return url.toString().replace(/\/$/, '')
  } catch {
    return value.trim().replace(/\/$/, '')
  }
}

function RankTrend({ values, label }: { readonly values: number[]; readonly label: string }) {
  if (values.length < 2) return <span className="trend-placeholder">{label}</span>
  const points = rankTrendPoints(values)
  return (
    <svg className="rank-trend" viewBox="0 0 72 24" aria-label={`rank history`}>
      <polyline points={points} />
    </svg>
  )
}

function TopicCard({ topic, displayRank, translation, m, onQueueChange, onTranslate, onOpen, selected }: {
  readonly topic: TopicView
  /** Mixed-source pages use the query-wide rank; one source keeps its native rank. */
  readonly displayRank: number
  readonly onOpen: (topic: TopicView) => void
  readonly selected: boolean
  readonly translation?: TranslationState
  readonly m: Messages
  readonly onQueueChange: (topic: TopicView, queued: boolean) => Promise<void>
  readonly onTranslate: (topic: TopicView) => Promise<void>
}) {
  const [saving, setSaving] = useState(false)
  const toggleQueue = async (): Promise<void> => {
    setSaving(true)
    try { await onQueueChange(topic, !topic.queued) }
    finally { setSaving(false) }
  }

  return (
    <article className={`topic-card${selected ? ' topic-selected' : ''}`}>
      <div className="topic-rank" aria-label={`${m.rankLabel} ${displayRank}`}>
        <span>{displayRank}</span>
        <small>{m.rankLabel}</small>
      </div>
      <div className="topic-content">
        <div className="topic-meta">
          <span className="source-pill">{topic.platformName}</span>
          <span>{formatTime(topic.updatedAt)}</span>
          {topic.rankDelta !== null && topic.rankDelta !== 0 ? (
            <span className={topic.rankDelta > 0 ? 'rank-up' : 'rank-down'}>
              {topic.rankDelta > 0 ? <ArrowUp aria-hidden="true" /> : <ArrowDown aria-hidden="true" />}
              {Math.abs(topic.rankDelta)}
            </span>
          ) : null}
        </div>
        <button className="topic-title" type="button" onClick={() => onOpen(topic)}>
          {topic.title}
        </button>
        {isEnglishTitle(topic.title) ? (
          <div className="translation-row">
            <button
              className="text-button" type="button"
              disabled={translation?.loading === true || translation?.text !== undefined}
              onClick={() => void onTranslate(topic)}
            >
              {translation?.loading === true ? m.translating : translation?.text === undefined ? m.translateBtn : m.translated}
            </button>
            {translation?.text !== undefined ? <span lang="zh-CN">{translation.text}</span> : null}
            {translation?.error !== undefined ? <span className="translation-error">{translation.error}</span> : null}
          </div>
        ) : null}
        <div className="topic-footer">
          <span>{m.firstSeen} {formatTime(topic.firstSeenAt)} · {m.consecutive} {topic.consecutiveRuns} {m.consecutiveUnit}</span>
          <RankTrend values={topic.trend} label={m.trendAccum} />
          <button className="text-button" type="button" disabled={saving} onClick={() => void toggleQueue()}>
            {saving ? m.saving : topic.queued ? m.queueRemove : m.queueAdd}
          </button>
        </div>
      </div>
    </article>
  )
}

/** Compose topic discovery, settings and the optional native article pane. */
export function App() {
  useEffect(() => { void browserRequest('closeAll').catch(() => {}) }, [])
  // ── Locale & theme ──────────────────────────────────────────────
  const [locale, setLocale] = useState<Locale>(detectLocale)
  const [theme, setTheme]   = useState<Theme>(detectTheme)
  const [uiPreferencesReady, setUiPreferencesReady] = useState(false)
  const m = messages[locale]

  useEffect(() => {
    let active = true
    void getUiPreferences()
      .then((preferences) => {
        if (!active) return
        if (preferences.locale !== null) setLocale(preferences.locale)
        if (preferences.theme !== null) setTheme(preferences.theme)
      })
      .catch((reason: unknown) => console.error('Failed to load UI preferences', reason))
      .finally(() => { if (active) setUiPreferencesReady(true) })
    return () => { active = false }
  }, [])

  useEffect(() => {
    applyTheme(theme)
  }, [theme])

  // Native WebView menus vary by operating system; the browser tab strip owns
  // the only contextual menu exposed by the application.
  useEffect(() => {
    const disableDefaultContextMenu = (event: MouseEvent) => event.preventDefault()
    document.addEventListener('contextmenu', disableDefaultContextMenu, true)
    return () => document.removeEventListener('contextmenu', disableDefaultContextMenu, true)
  }, [])

  useEffect(() => {
    if (!uiPreferencesReady) return
    void saveUiPreferences({ locale, theme })
      .catch((reason: unknown) => console.error('Failed to save UI preferences', reason))
  }, [locale, theme, uiPreferencesReady])

  // ── View state ──────────────────────────────────────────────────
  const [browserOpen, setBrowserOpen] = useState(false)
  const [browserClosing, setBrowserClosing] = useState(false)
  const browserCloseTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [browserTabs, setBrowserTabs] = useState<BrowserTab[]>([])
  const [activeBrowserTabId, setActiveBrowserTabId] = useState('')
  const browserTabsRef = useRef<BrowserTab[]>([])
  const activeBrowserTabIdRef = useRef('')
  const browserTabCounter = useRef(0)
  const topicTabIdsRef = useRef(new Map<string, string>())
  const [readerExpanded, setReaderExpanded] = useState(false)
  const [readerWidth, setReaderWidth] = useState(60)
  const [view, setView]           = useState<ViewMode>('discover')
  const [prevView, setPrevView]   = useState<Exclude<ViewMode, 'settings'>>('discover')
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('general')

  // ── Filter state ────────────────────────────────────────────────
  const [region,   setRegion]   = useState<'all' | SourceRegion>('all')
  const [category, setCategory] = useState<'all' | TopicCategory>('all')
  const [source,   setSource]   = useState('all')
  const [sort,     setSort]     = useState<'rank' | 'updated'>('rank')
  const [search,   setSearch]   = useState('')
  const [pageIndex, setPageIndex] = useState(0)

  // ── Data state ──────────────────────────────────────────────────
  const [page,              setPage]              = useState<TopicPage>()
  const [insertedTopicIds,  setInsertedTopicIds]  = useState<number[]>([])
  const [loading,           setLoading]           = useState(true)
  const [refreshing,        setRefreshing]        = useState(false)
  const [notice,            setNotice]            = useState<string>()
  const [error,             setError]             = useState<string>()
  const [translations,      setTranslations]      = useState<Record<number, TranslationState>>({})

  // ── Settings state ──────────────────────────────────────────────
  const [modelSettings,   setModelSettingsState] = useState<ModelSettings>()
  const [modelEndpoint,   setModelEndpoint]      = useState('https://api.deepseek.com')
  const [modelName,       setModelName]          = useState('deepseek-chat')
  const [apiKey,          setApiKey]             = useState('')
  const [savingSettings,  setSavingSettings]     = useState(false)
  const [proxyUrl,        setProxyUrl]           = useState('')
  const [savingNetwork,   setSavingNetwork]      = useState(false)
  const [storageStatus,   setStorageStatus]      = useState<StorageStatus>()
  const [storageAction,   setStorageAction]      = useState<'backup' | 'restore' | 'optimize' | null>(null)

  browserTabsRef.current = browserTabs
  activeBrowserTabIdRef.current = activeBrowserTabId

  /** Keep refs synchronized immediately so consecutive clicks see the latest tab state. */
  const replaceBrowserTabs = useCallback((next: BrowserTab[]): void => {
    browserTabsRef.current = next
    setBrowserTabs(next)
  }, [])

  const activateBrowserTab = useCallback((id: string): void => {
    activeBrowserTabIdRef.current = id
    setActiveBrowserTabId(id)
  }, [])

  /** Browser tabs live above the panel so hiding it does not discard the session. */
  const addBrowserTab = useCallback((tab?: Omit<BrowserTab, 'id'>): string => {
    const id = `tab-${++browserTabCounter.current}`
    replaceBrowserTabs([...browserTabsRef.current, {
      id,
      url: tab?.url ?? '',
      title: tab?.title ?? '',
      ...(tab?.topicId === undefined ? {} : { topicId: tab.topicId }),
      ...(tab?.sourceUrl === undefined ? {} : { sourceUrl: tab.sourceUrl }),
    }])
    activateBrowserTab(id)
    setBrowserOpen(true)
    setBrowserClosing(false)
    if (browserCloseTimer.current) { clearTimeout(browserCloseTimer.current); browserCloseTimer.current = null }
    return id
  }, [activateBrowserTab, replaceBrowserTabs])

  const closeBrowser = useCallback((): void => {
    if (browserCloseTimer.current) clearTimeout(browserCloseTimer.current)
    setBrowserClosing(true)
    setBrowserOpen(false)
    browserCloseTimer.current = setTimeout(() => {
      setBrowserClosing(false)
      setReaderExpanded(false)
      browserCloseTimer.current = null
    }, 280)
  }, [])

  /** The top-right control directly toggles the browser while retaining its tabs. */
  const toggleBrowser = useCallback((): void => {
    if (browserOpen) {
      closeBrowser()
      return
    }
    if (browserTabs.length === 0) addBrowserTab()
    else {
      setBrowserOpen(true)
      setBrowserClosing(false)
      if (browserCloseTimer.current) { clearTimeout(browserCloseTimer.current); browserCloseTimer.current = null }
      if (!activeBrowserTabId && browserTabs[0]) activateBrowserTab(browserTabs[0].id)
    }
  }, [activateBrowserTab, activeBrowserTabId, addBrowserTab, browserOpen, browserTabs, closeBrowser])

  const openTopic = useCallback((topic: TopicView): void => {
    const topicUrl = normalizedArticleUrl(topic.url)
    const topicKeys = [`id:${topic.id}`, `url:${topicUrl}`]
    const newId = `tab-${++browserTabCounter.current}`
    setBrowserTabs((currentTabs) => {
      const mappedId = topicKeys.map((key) => topicTabIdsRef.current.get(key)).find((id) => (
        id !== undefined && currentTabs.some((tab) => tab.id === id)
      ))
      const existing = currentTabs.find((tab) => tab.id === mappedId) ?? currentTabs.find((tab) => (
        tab.topicId === topic.id
        || (tab.sourceUrl !== undefined && normalizedArticleUrl(tab.sourceUrl) === topicUrl)
        || (tab.url !== '' && normalizedArticleUrl(tab.url) === topicUrl)
      ))
      // Each distinct topic owns one tab; repeat clicks only activate it.
      const targetId = existing?.id ?? newId
      const next = existing
        ? currentTabs
        : [...currentTabs, {
            id: newId,
            url: topic.url,
            title: topic.title,
            topicId: topic.id,
            sourceUrl: topic.url,
          }]
      browserTabsRef.current = next
      topicKeys.forEach((key) => topicTabIdsRef.current.set(key, targetId))
      activeBrowserTabIdRef.current = targetId
      setActiveBrowserTabId(targetId)
      return next
    })
    setBrowserOpen(true)
    setBrowserClosing(false)
    if (browserCloseTimer.current) { clearTimeout(browserCloseTimer.current); browserCloseTimer.current = null }
  }, [])

  const openXiaohongshuSession = (): void => {
    switchView(prevView)
    addBrowserTab({ url: 'https://www.xiaohongshu.com/explore', title: m.xhsLoginCollect })
  }

  const collectXiaohongshu = async (tabId: string): Promise<string> => {
    const result = await collectXiaohongshuSession(tabId)
    setInsertedTopicIds(result.insertedTopicIds)
    await load()
    return result.message
  }

  const updateBrowserTab = useCallback((id: string, update: Partial<Pick<BrowserTab, 'url' | 'title'>>): void => {
    replaceBrowserTabs(browserTabsRef.current.map((tab) => tab.id === id ? { ...tab, ...update } : tab))
  }, [replaceBrowserTabs])

  /** Reorder tabs while keeping Chrome-style pinned tabs grouped on the left. */
  const moveBrowserTab = useCallback((draggedId: string, targetId: string, after: boolean): void => {
    if (draggedId === targetId) return
    const current = browserTabsRef.current
    const dragged = current.find((tab) => tab.id === draggedId)
    if (!dragged) return
    const withoutDragged = current.filter((tab) => tab.id !== draggedId)
    const targetIndex = withoutDragged.findIndex((tab) => tab.id === targetId)
    if (targetIndex < 0) return
    withoutDragged.splice(targetIndex + (after ? 1 : 0), 0, dragged)
    replaceBrowserTabs([
      ...withoutDragged.filter((tab) => tab.pinned === true),
      ...withoutDragged.filter((tab) => tab.pinned !== true),
    ])
  }, [replaceBrowserTabs])

  /** Pinning is local browser UI state and never recreates the native WebView. */
  const pinBrowserTab = useCallback((id: string, pinned: boolean): void => {
    const updated = browserTabsRef.current.map((tab) => tab.id === id ? { ...tab, pinned } : tab)
    replaceBrowserTabs([
      ...updated.filter((tab) => tab.pinned === true),
      ...updated.filter((tab) => tab.pinned !== true),
    ])
  }, [replaceBrowserTabs])

  const closeBrowserTab = useCallback((id: string): void => {
    const current = browserTabsRef.current
    const index = current.findIndex((tab) => tab.id === id)
    const next = current.filter((tab) => tab.id !== id)
    replaceBrowserTabs(next)
    for (const [key, tabId] of topicTabIdsRef.current) {
      if (tabId === id) topicTabIdsRef.current.delete(key)
    }
    if (activeBrowserTabIdRef.current === id) {
      activateBrowserTab(next[Math.min(Math.max(index, 0), next.length - 1)]?.id ?? '')
    }
    if (next.length === 0) setBrowserOpen(false)
  }, [activateBrowserTab, replaceBrowserTabs])

  useEffect(() => {
    const openTab = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 't') {
        event.preventDefault()
        addBrowserTab()
      }
    }
    window.addEventListener('keydown', openTab)
    return () => window.removeEventListener('keydown', openTab)
  }, [addBrowserTab])

  // ── Query ────────────────────────────────────────────────────────
  const query = useMemo<TopicQuery>(() => ({
    ...(view === 'queue' ? { queuedOnly: true } : {}),
    ...(view === 'new'   ? { topicIds: insertedTopicIds } : {}),
    ...(source   === 'all' ? {} : { source }),
    ...(region   === 'all' ? {} : { region }),
    ...(category === 'all' ? {} : { category }),
    ...(search.trim() === '' ? {} : { search: search.trim() }),
    sort, limit: PAGE_SIZE, offset: pageIndex * PAGE_SIZE,
  }), [category, insertedTopicIds, pageIndex, region, search, sort, source, view])

  // ── Data loading ─────────────────────────────────────────────────
  const querySequence = useRef(0)
  const load = useCallback(async (silent = false): Promise<void> => {
    const sequence = ++querySequence.current
    if (!silent) {
      setLoading(true)
      setError(undefined)
    }
    try {
      const nextPage = await listTopics(query)
      if (sequence === querySequence.current) setPage(nextPage)
    } catch (reason) {
      // Silent reloads keep the last valid page after actions such as collection.
      if (!silent && sequence === querySequence.current) {
        setError(reason instanceof Error ? reason.message : String(reason))
      }
    } finally {
      if (!silent && sequence === querySequence.current) setLoading(false)
    }
  }, [query])

  useEffect(() => {
    const timer = window.setTimeout(() => void load(), 160)
    return () => window.clearTimeout(timer)
  }, [load])

  useEffect(() => {
    if (view !== 'settings') return
    void getModelSettings().then((s) => {
      setModelSettingsState(s)
      setModelEndpoint(s.endpoint)
      setModelName(s.model)
    }).catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)))
    void getNetworkSettings().then((s) => setProxyUrl(s.proxyUrl ?? ''))
      .catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)))
    void getStorageStatus().then(setStorageStatus)
      .catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)))
  }, [view])

  // ── Actions ──────────────────────────────────────────────────────
  const collect = async (): Promise<void> => {
    setRefreshing(true); setError(undefined); setNotice(undefined)
    try {
      const result = await refreshTopics()
      setNotice(result.message)
      setInsertedTopicIds(result.insertedTopicIds)
      if (result.insertedTopicIds.length > 0) { setView('new'); setPageIndex(0) }
      // Manual refresh is a network collection followed by an immediate local query.
      await load(true)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally { setRefreshing(false) }
  }

  const changeQueue = async (topic: TopicView, queued: boolean): Promise<void> => {
    await setTopicQueued(topic.id, queued)
    await load()
  }

  const changePlatform = async (code: string, enabled: boolean): Promise<void> => {
    setError(undefined)
    try { await setPlatformEnabled(code, enabled); await load() }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
  }

  const translate = async (topic: TopicView): Promise<void> => {
    setTranslations((cur) => ({ ...cur, [topic.id]: { loading: true } }))
    try {
      const result = await translateTopic(topic.id)
      setTranslations((cur) => ({ ...cur, [topic.id]: { text: result.translation } }))
    } catch (reason) {
      setTranslations((cur) => ({ ...cur, [topic.id]: { error: reason instanceof Error ? reason.message : String(reason) } }))
    }
  }

  const saveSettings = async (): Promise<void> => {
    setSavingSettings(true); setError(undefined)
    try {
      const s = await saveModelSettings({
        endpoint: modelEndpoint, model: modelName,
        ...(apiKey.trim() === '' ? {} : { apiKey: apiKey.trim() }),
      })
      setModelSettingsState(s); setApiKey(''); setNotice(m.saveNotice)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally { setSavingSettings(false) }
  }

  const saveNetwork = async (): Promise<void> => {
    setSavingNetwork(true); setError(undefined); setNotice(undefined)
    try {
      const settings = await saveNetworkSettings({ proxyUrl })
      setProxyUrl(settings.proxyUrl ?? '')
      setNotice(m.networkSaveNotice)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally { setSavingNetwork(false) }
  }

  const runStorageAction = async (action: 'backup' | 'restore' | 'optimize'): Promise<void> => {
    if (action === 'restore' && !window.confirm(m.storageRestoreConfirm)) return
    setStorageAction(action); setError(undefined); setNotice(undefined)
    try {
      const result = action === 'backup'
        ? await backupStorage()
        : action === 'restore'
          ? await restoreLatestBackup()
          : await optimizeStorage()
      setNotice(result.message)
      setStorageStatus(await getStorageStatus())
      if (action === 'restore') await load(true)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally { setStorageAction(null) }
  }

  const switchView = (next: ViewMode): void => {
    if (next === 'settings' && view !== 'settings') {
      setPrevView(view as Exclude<ViewMode, 'settings'>)
    }
    setView(next); setPageIndex(0)
  }

  const totalPages = Math.max(1, Math.ceil((page?.total ?? 0) / PAGE_SIZE))

  // ── Settings fullscreen ──────────────────────────────────────────
  if (view === 'settings') {
    const tabIcons: Record<SettingsTab, LucideIcon> = {
      general: SlidersHorizontal,
      network: Network,
      sources: Database,
      model: Bot,
      storage: HardDrive,
    }
    const tabLabels: Record<SettingsTab, string> = {
      general: m.tabGeneral,
      network: m.sectionNetwork,
      sources: m.tabSources,
      model: m.tabModel,
      storage: m.tabStorage,
    }
    return (
      <div className="settings-fullscreen">
        <header className="settings-fs-header">
          <button className="back-button" type="button" onClick={() => switchView(prevView)}>
            <ArrowLeft aria-hidden="true" />{m.settingsBack}
          </button>
          <span className="settings-fs-title">{m.settingsTitle}</span>
        </header>

        <div className="settings-layout">
          {/* Left tab nav */}
          <nav className="settings-tabs" aria-label={m.settingsTitle}>
            {(['general', 'sources', 'model', 'storage', 'network'] as SettingsTab[]).map((tab) => {
              const TabIcon = tabIcons[tab]
              return (
                <button
                  key={tab}
                  className={settingsTab === tab ? 'settings-tab active' : 'settings-tab'}
                  onClick={() => setSettingsTab(tab)}
                >
                  <TabIcon className="settings-tab-icon" aria-hidden="true" />
                  {tabLabels[tab]}
                </button>
              )
            })}
          </nav>

          {/* Right panel */}
          <div className="settings-panel">
            {settingsTab === 'general' && (
              <>
                {/* Appearance */}
                <div className="pref-section">
                  <p className="pref-section-title">{m.sectionAppearance}</p>
                  <div className="pref-row">
                    <span className="pref-label">{m.labelTheme}</span>
                    <div className="seg-control">
                      {(['light', 'system', 'dark'] as Theme[]).map((t) => (
                        <button
                          key={t}
                          className={theme === t ? 'seg-btn active' : 'seg-btn'}
                          onClick={() => setTheme(t)}
                        >
                          {t === 'light' ? m.themeLight : t === 'dark' ? m.themeDark : m.themeSystem}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>

                {/* Language */}
                <div className="pref-section">
                  <p className="pref-section-title">{m.sectionLanguage}</p>
                  <div className="pref-row">
                    <span className="pref-label">{m.sectionLanguage}</span>
                    <div className="seg-control">
                      {(['zh', 'en'] as Locale[]).map((l) => (
                        <button
                          key={l}
                          className={locale === l ? 'seg-btn active' : 'seg-btn'}
                          onClick={() => setLocale(l)}
                        >
                          {l === 'zh' ? m.langZh : m.langEn}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>
              </>
            )}

            {settingsTab === 'network' && (
              /* Native collection traffic never crosses the WebView network boundary. */
              <div className="pref-section">
                <p className="pref-section-title">{m.sectionNetwork}</p>
                <div className="network-setting">
                  <label>
                    <span>{m.labelProxy}</span>
                    <input
                      value={proxyUrl}
                      onChange={(event) => setProxyUrl(event.target.value)}
                      placeholder={m.proxyPlaceholder}
                      spellCheck={false}
                      autoComplete="off"
                    />
                  </label>
                  <p>{m.proxyHelp}</p>
                  <button className="primary-button" type="button" disabled={savingNetwork} onClick={() => void saveNetwork()}>
                    {savingNetwork ? m.btnSaving : m.btnSave}
                  </button>
                </div>
              </div>
            )}

            {settingsTab === 'sources' && (
              <div className="source-list" aria-live="polite">
                {page?.statuses.map((status) => (
                  <div className="source-row" key={status.code}>
                    <span className={`health-dot ${status.status === 'failed' ? 'failed' : status.status === 'succeeded' ? 'healthy' : ''}`} />
                    <div className="source-row-info">
                      <span className="source-row-name">{status.displayName}</span>
                      <span className="source-row-meta" title={status.error ?? undefined}>
                        {status.region === 'domestic' ? m.regionDomestic : m.regionIntl} · {status.error ?? (status.lastRunAt === null ? m.notCollected : m.topicCount(status.topicCount))}
                      </span>
                    </div>
                    {status.code === 'xiaohongshu' ? (
                      <button className="source-action" type="button" onClick={openXiaohongshuSession}>{m.xhsLoginCollect}</button>
                    ) : null}
                    <label className="switch">
                      <input type="checkbox" checked={status.enabled} onChange={(e) => void changePlatform(status.code, e.target.checked)} />
                      <span />
                    </label>
                  </div>
                ))}
              </div>
            )}

            {settingsTab === 'storage' && (
              <div className="storage-card">
                <div>
                  <p className="eyebrow">LOCAL-FIRST</p>
                  <h2>{m.storageHeading}</h2>
                  <p>{m.storageDesc}</p>
                </div>
                {storageStatus !== undefined ? (
                  <>
                    <div className={`storage-health ${storageStatus.integrityOk ? 'healthy' : 'failed'}`}>
                      <span />{storageStatus.integrityOk ? m.storageHealthy : m.storageDamaged}
                    </div>
                    <div className="storage-metrics">
                      <div><strong>{storageStatus.topicCount.toLocaleString()}</strong><span>{m.storageTopics}</span></div>
                      <div><strong>{storageStatus.observationCount.toLocaleString()}</strong><span>{m.storageTrends}</span></div>
                      <div><strong>{storageStatus.collectionRunCount.toLocaleString()}</strong><span>{m.storageRuns}</span></div>
                      <div><strong>{storageStatus.browserRecordCount.toLocaleString()}</strong><span>{m.storageBrowser}</span></div>
                    </div>
                    <dl className="storage-details">
                      <div><dt>{m.storageTopicDb}</dt><dd>{formatBytes(storageStatus.topicDatabaseBytes)}</dd></div>
                      <div><dt>{m.storageBrowserDb}</dt><dd>{formatBytes(storageStatus.browserDatabaseBytes)}</dd></div>
                      <div><dt>{m.storageLatestBackup}</dt><dd>{formatBackupName(storageStatus.latestBackup, m.storageNoBackup)}</dd></div>
                    </dl>
                  </>
                ) : null}
                {notice !== undefined ? <div className="notice">{notice}</div> : null}
                {error !== undefined ? <div className="error-banner">{error}</div> : null}
                <div className="storage-actions">
                  <button type="button" disabled={storageAction !== null} onClick={() => void openDataDirectory().catch((reason: unknown) => setError(reason instanceof Error ? reason.message : String(reason)))}><FolderOpen aria-hidden="true" />{m.storageOpenFolder}</button>
                  <button type="button" disabled={storageAction !== null} onClick={() => void runStorageAction('optimize')}><Wrench aria-hidden="true" />{storageAction === 'optimize' ? m.storageWorking : m.storageOptimize}</button>
                  <button type="button" disabled={storageAction !== null} onClick={() => void runStorageAction('backup')}><Archive aria-hidden="true" />{storageAction === 'backup' ? m.storageWorking : m.storageBackup}</button>
                  <button className="storage-restore" type="button" disabled={storageAction !== null || storageStatus?.latestBackup == null} onClick={() => void runStorageAction('restore')}><RotateCcw aria-hidden="true" />{storageAction === 'restore' ? m.storageWorking : m.storageRestore}</button>
                </div>
                {storageStatus !== undefined ? <code className="storage-path">{storageStatus.dataDirectory}</code> : null}
              </div>
            )}

            {settingsTab === 'model' && (
              <div className="settings-card">
                <div>
                  <p className="eyebrow">OPENAI-COMPATIBLE</p>
                  <h2>{m.modelHeading}</h2>
                  <p>{m.modelDesc}</p>
                </div>
                <label>
                  <span>{m.labelEndpoint}</span>
                  <input value={modelEndpoint} onChange={(e) => setModelEndpoint(e.target.value)} placeholder="https://api.deepseek.com" />
                </label>
                <label>
                  <span>{m.labelModel}</span>
                  <input value={modelName} onChange={(e) => setModelName(e.target.value)} placeholder="deepseek-chat" />
                </label>
                <label>
                  <span>{m.labelApiKey}</span>
                  <input
                    type="password" value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder={modelSettings?.hasApiKey === true ? m.apiKeySavedPlaceholder : m.apiKeyPlaceholder}
                    autoComplete="off"
                  />
                </label>
                {notice !== undefined ? <div className="notice">{notice}</div> : null}
                {error  !== undefined ? <div className="error-banner">{error}</div> : null}
                <button className="primary-button settings-save" type="button" disabled={savingSettings} onClick={() => void saveSettings()}>
                  {savingSettings ? m.btnSaving : m.btnSave}
                </button>
              </div>
            )}
          </div>
        </div>
      </div>
    )
  }

  // ── Normal view ──────────────────────────────────────────────────
  const viewTitle = view === 'discover' ? m.navDiscover : view === 'queue' ? m.navQueue : m.navNew

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">TD</span>
          <div>
            <strong>Topic Desk</strong>
            <small>STUDIO</small>
          </div>
        </div>

        <nav aria-label={m.navSettings}>
          <button className={view === 'discover' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('discover')}>
            <Compass aria-hidden="true" />{m.navDiscover}
          </button>
          <button className={view === 'queue' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('queue')}>
            <Bookmark aria-hidden="true" />{m.navQueue} <small>{page?.queuedTotal ?? 0}</small>
          </button>
          <button className={view === 'new' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('new')}>
            <Sparkles aria-hidden="true" />{m.navNew} <small>{insertedTopicIds.length}</small>
          </button>
        </nav>

        <div className="sidebar-footer">
          <button className="nav-item" onClick={() => switchView('settings')}>
            <Settings aria-hidden="true" />{m.navSettings}
          </button>
        </div>
      </aside>

      <div className={`desk-workspace${browserOpen ? ' reader-open' : ''}${readerExpanded ? ' reader-expanded' : ''}`} style={{ gridTemplateColumns: (browserOpen || browserClosing) && !readerExpanded ? `minmax(320px, ${100 - readerWidth}fr) minmax(0, ${readerWidth}fr)` : undefined }}>
      <main className="topic-list-pane">
        <header className="topbar">
          <div className="topbar-main">
            <h1>{viewTitle}</h1>
            <label className="topbar-search">
              <Search aria-hidden="true" />
              <input
                aria-label={m.filterSearch}
                value={search}
                placeholder={m.filterSearchPlaceholder}
                onChange={(e) => { setSearch(e.target.value); setPageIndex(0) }}
              />
            </label>
          </div>
          <div className="topbar-actions">
            <button className={`query-button topbar-icon-button${loading ? ' busy' : ''}`} type="button" disabled={loading || refreshing} aria-label={loading ? m.btnQuerying : m.btnQuery} title={loading ? m.btnQuerying : m.btnQuery} onClick={() => void load()}>
              <RefreshCw aria-hidden="true" />
            </button>
            <button className={`primary-button topbar-icon-button${refreshing ? ' busy' : ''}`} type="button" disabled={refreshing} aria-label={refreshing ? m.btnRefreshing : m.btnRefresh} title={refreshing ? m.btnRefreshing : m.btnRefresh} onClick={() => void collect()}>
              <Radar aria-hidden="true" />
            </button>
            <button className={`side-panel-toggle topbar-icon-button${browserOpen ? ' active' : ''}`} type="button" aria-label={locale === 'zh' ? '显示/隐藏浏览器' : 'Show/hide browser'} aria-pressed={browserOpen} title={locale === 'zh' ? '显示/隐藏浏览器' : 'Show/hide browser'} onClick={toggleBrowser}><PanelRight aria-hidden="true" /></button>
          </div>
        </header>

        <div className="list-sticky-controls">
          <section className="filters" aria-label={m.filterSearch}>
            <label>
              <span>{m.filterSource}</span>
              <select value={source} onChange={(e) => { setSource(e.target.value); setPageIndex(0) }}>
                <option value="all">{m.filterAllSources}</option>
                {page?.statuses.map((s) => <option key={s.code} value={s.code}>{s.displayName}</option>)}
              </select>
              <ChevronDown className="filter-chevron" aria-hidden="true" />
            </label>
            <label>
              <span>{m.filterRegion}</span>
              <select value={region} onChange={(e) => { setRegion(e.target.value as typeof region); setPageIndex(0) }}>
                <option value="all">{m.filterAllRegions}</option>
                <option value="domestic">{m.filterDomestic}</option>
                <option value="international">{m.filterInternational}</option>
              </select>
              <ChevronDown className="filter-chevron" aria-hidden="true" />
            </label>
            <label>
              <span>{m.filterCategory}</span>
              <select value={category} onChange={(e) => { setCategory(e.target.value as typeof category); setPageIndex(0) }}>
                <option value="all">{m.filterAllCategories}</option>
                <option value="general">{m.filterGeneral}</option>
                <option value="technology">{m.filterTech}</option>
                <option value="finance">{m.filterFinance}</option>
                <option value="developer">{m.filterDev}</option>
              </select>
              <ChevronDown className="filter-chevron" aria-hidden="true" />
            </label>
            <label>
              <span>{m.filterSort}</span>
              <select value={sort} onChange={(e) => { setSort(e.target.value as typeof sort); setPageIndex(0) }}>
                <option value="rank">{m.filterSortRank}</option>
                <option value="updated">{m.filterSortUpdated}</option>
              </select>
              <ChevronDown className="filter-chevron" aria-hidden="true" />
            </label>
          </section>
          <div className="results-heading">
            <span>{m.resultsTotal(page?.total ?? 0)}</span>
            <span>{m.resultsErrors(page?.statuses.filter((s) => s.error !== null).length ?? 0)}</span>
          </div>
        </div>

        {notice !== undefined ? <div className="notice">{notice}</div> : null}
        {error  !== undefined ? <div className="error-banner">{error}</div> : null}

        <section className="results" aria-live="polite">
          {loading ? (
            <div className="empty-state"><div className="loader" /><p>{m.resultsLoading}</p></div>
          ) : null}
          {!loading && page?.topics.length === 0 ? (
            <div className="empty-state">
              <div className="empty-glyph"><Sparkles aria-hidden="true" /></div>
              <h2>{m.resultsEmptyTitle}</h2>
              <p>{m.resultsEmptyBody}</p>
            </div>
          ) : null}
          {!loading ? page?.topics.map((topic) => (
            <TopicCard
              key={topic.id} topic={topic} m={m}
              displayRank={source === 'all' ? topic.globalRank : topic.rank}
              {...(translations[topic.id] === undefined ? {} : { translation: translations[topic.id] })}
              onQueueChange={changeQueue} onTranslate={translate}
              onOpen={openTopic} selected={browserTabs.find((tab) => tab.id === activeBrowserTabId)?.topicId === topic.id}
            />
          )) : null}
        </section>

        <footer className="pagination">
          <button disabled={pageIndex === 0} onClick={() => setPageIndex((v) => Math.max(0, v - 1))}>{m.prev}</button>
          <span>{pageIndex + 1} / {totalPages}</span>
          <button disabled={pageIndex + 1 >= totalPages} onClick={() => setPageIndex((v) => v + 1)}>{m.next}</button>
        </footer>
      </main>
      {(browserOpen || browserClosing) && activeBrowserTabId ? <BrowserPane tabs={browserTabs} activeTabId={activeBrowserTabId} locale={locale} expanded={readerExpanded} closing={browserClosing}
        onActivate={activateBrowserTab} onNewTab={() => addBrowserTab()} onUpdateTab={updateBrowserTab} onCloseTab={closeBrowserTab}
        onMoveTab={moveBrowserTab} onPinTab={pinBrowserTab}
        onExpand={() => setReaderExpanded((value) => !value)}
        onCollectXiaohongshu={collectXiaohongshu}
        onClose={closeBrowser}
        onResize={setReaderWidth} /> : null}
      </div>
    </div>
  )
}
