/** Topic Desk Studio — main UI component. */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { PointerEvent as ReactPointerEvent, ReactNode } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Archive, ArrowDown, ArrowLeft, ArrowUp, Bot, Bookmark, ChevronDown, Compass, Database, Download, EyeOff, FolderOpen, GripVertical, HardDrive, Network, PanelRight, Pencil, Plus, Radar, RefreshCw, RotateCcw, Search, Settings, SlidersHorizontal, Sparkles, Trash2, Upload, Wrench, X } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import {
  backupStorage, browserRequest, collectXiaohongshuSession, deleteSource, exportSourceConfigurations, hideTopic,
  getModelSettings, getNetworkSettings, getStorageStatus, getUiPreferences, importSourceConfigurations, listSourceConfigurations, listTopics,
  openDataDirectory, optimizeStorage, refreshTopics, restoreLatestBackup, saveModelSettings,
  reorderSourceConfigurations, restoreDefaultSources, saveNetworkSettings, saveSourceConfiguration, saveUiPreferences, setPlatformEnabled,
  setTopicQueued, translateTopic,
} from './api'
import { BrowserPane } from './BrowserPane'
import type { BrowserTab } from './BrowserPane'
import { isEnglishTitle, parseStoredTimestamp, rankTrendPoints } from './presentation'
import {
  Locale, Theme, Messages, messages,
  detectLocale, detectTheme, applyTheme,
} from './i18n'
import type { CollectionStatusEvent, ModelSettings, SaveSourceConfiguration, SourceConfiguration, SourceRegion, StorageStatus, TopicCategory, TopicPage, TopicQuery, TopicView } from './types'

const PAGE_SIZE = 20
type ViewMode = 'discover' | 'queue' | 'new' | 'settings'
type SettingsTab = 'general' | 'network' | 'sources' | 'model' | 'storage'
interface TranslationState { readonly loading?: boolean; readonly text?: string; readonly error?: string }
interface NoticeState { readonly scope: 'workspace' | 'network' | 'sources' | 'model' | 'storage'; readonly text: string }
interface SourceDropTarget { readonly code: string; readonly after: boolean }
interface SourcePointerDrag { readonly code: string; readonly pointerId: number; readonly startY: number; readonly currentY: number; readonly active: boolean }
interface FilterOption { readonly value: string; readonly label: string; readonly removable?: boolean }
type ModelProviderId = 'deepseek' | 'openai' | 'dashscope' | 'siliconflow' | 'volcengine' | 'ollama' | 'custom'
interface ModelProviderPreset { readonly id: Exclude<ModelProviderId, 'custom'>; readonly label: string; readonly endpoint: string; readonly models: readonly string[] }

/** OpenAI-compatible providers that can use the native translation client unchanged. */
const MODEL_PROVIDER_PRESETS: readonly ModelProviderPreset[] = [
  { id: 'deepseek', label: 'DeepSeek', endpoint: 'https://api.deepseek.com', models: ['deepseek-flash', 'deepseek-v4-pro'] },
  { id: 'openai', label: 'OpenAI', endpoint: 'https://api.openai.com/v1', models: ['gpt-5-mini', 'gpt-4.1-mini', 'gpt-4.1-nano'] },
  { id: 'dashscope', label: '阿里云百炼', endpoint: 'https://dashscope.aliyuncs.com/compatible-mode/v1', models: ['qwen-flash', 'qwen-plus', 'qwen-turbo', 'qwen3-max'] },
  { id: 'siliconflow', label: '硅基流动', endpoint: 'https://api.siliconflow.cn/v1', models: ['Pro/deepseek-ai/DeepSeek-V3.2', 'Pro/zai-org/GLM-5.1', 'Qwen/Qwen3-8B'] },
  { id: 'volcengine', label: '火山方舟', endpoint: 'https://ark.cn-beijing.volces.com/api/v3', models: ['doubao-seed-2-1-pro-260628', 'doubao-seed-evolving'] },
  { id: 'ollama', label: 'Ollama（本地）', endpoint: 'http://localhost:11434/v1', models: ['qwen3:8b', 'qwen3:4b', 'llama3.2'] },
]

function normalizeEndpoint(value: string): string {
  return value.trim().replace(/\/+$/, '')
}

/** Infer a visual provider selection from the persisted endpoint without changing storage. */
function detectModelProvider(endpoint: string): ModelProviderId {
  return MODEL_PROVIDER_PRESETS.find((provider) => normalizeEndpoint(provider.endpoint) === normalizeEndpoint(endpoint))?.id ?? 'custom'
}

const EMPTY_SOURCE: SaveSourceConfiguration = {
  code: '', displayName: '', homeUrl: '', endpointUrl: '',
  region: 'international', category: 'general', parserType: 'rss',
  proxyMode: 'auto', enabled: true, parserConfig: {},
}

function formatTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  }).format(parseStoredTimestamp(value))
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

/** Shared filter dropdown; source options may additionally expose a disable action. */
function FilterDropdown({ label, options, value, onChange, onRemove, removeLabel }: {
  readonly label: string
  readonly options: FilterOption[]
  readonly value: string
  readonly onChange: (value: string) => void
  readonly onRemove?: (value: string) => Promise<void>
  readonly removeLabel?: (option: FilterOption) => string
}) {
  const [open, setOpen] = useState(false)
  const [disabling, setDisabling] = useState<string>()
  const root = useRef<HTMLDivElement>(null)
  const selected = options.find((option) => option.value === value) ?? options[0]

  useEffect(() => {
    if (!open) return
    const closeOutside = (event: PointerEvent): void => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false)
    }
    document.addEventListener('pointerdown', closeOutside)
    return () => document.removeEventListener('pointerdown', closeOutside)
  }, [open])

  const remove = async (optionValue: string): Promise<void> => {
    if (onRemove === undefined) return
    setDisabling(optionValue)
    try { await onRemove(optionValue) }
    finally { setDisabling(undefined) }
  }

  return <div className="source-filter-field" ref={root} onKeyDown={(event) => { if (event.key === 'Escape') setOpen(false) }}>
    <span>{label}</span>
    <div className="source-filter-control">
      <button className="source-filter-trigger" type="button" aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((current) => !current)}>
        <span>{selected?.label ?? ''}</span><ChevronDown aria-hidden="true" />
      </button>
      {open ? <div className="source-filter-menu" role="listbox" aria-label={label}>
        {options.map((option) => <div className={`source-filter-option-row${value === option.value ? ' selected' : ''}`} role="option" aria-selected={value === option.value} key={option.value}>
          <button className="source-filter-option" type="button" onClick={() => { onChange(option.value); setOpen(false) }}>{option.label}</button>
          {option.removable === true && onRemove !== undefined ? <button className="source-filter-disable" type="button" disabled={disabling === option.value} aria-label={removeLabel?.(option) ?? option.label} title={removeLabel?.(option) ?? option.label} onClick={() => void remove(option.value)}><X aria-hidden="true" /></button> : null}
        </div>)}
      </div> : null}
    </div>
  </div>
}

function TopicCard({ topic, displayRank, translation, m, onQueueChange, onTranslate, onHide, onOpen, selected }: {
  readonly topic: TopicView
  /** Mixed-source pages use the query-wide rank; one source keeps its native rank. */
  readonly displayRank: number
  readonly onOpen: (topic: TopicView) => void
  readonly selected: boolean
  readonly translation?: TranslationState
  readonly m: Messages
  readonly onQueueChange: (topic: TopicView, queued: boolean) => Promise<void>
  readonly onTranslate: (topic: TopicView) => Promise<void>
  readonly onHide?: (topic: TopicView) => Promise<void>
}) {
  const [saving, setSaving] = useState(false)
  const [hiding, setHiding] = useState(false)
  const toggleQueue = async (): Promise<void> => {
    setSaving(true)
    try { await onQueueChange(topic, !topic.queued) }
    finally { setSaving(false) }
  }
  const hide = async (): Promise<void> => {
    if (onHide === undefined) return
    setHiding(true)
    try { await onHide(topic) }
    finally { setHiding(false) }
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
          {onHide !== undefined ? <button className="text-button topic-hide-button" type="button" disabled={hiding} title={m.topicHide} onClick={() => void hide()}><EyeOff aria-hidden="true" />{hiding ? m.saving : m.topicHide}</button> : null}
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
  const [searchOpen, setSearchOpen] = useState(false)
  const searchInputRef = useRef<HTMLInputElement>(null)
  const [pageIndex, setPageIndex] = useState(0)

  // ── Data state ──────────────────────────────────────────────────
  const [page,              setPage]              = useState<TopicPage>()
  const [loading,           setLoading]           = useState(true)
  const [refreshing,        setRefreshing]        = useState(false)
  const [notice,            setNotice]            = useState<NoticeState>()
  const [error,             setError]             = useState<string>()
  const [translations,      setTranslations]      = useState<Record<number, TranslationState>>({})

  // ── Settings state ──────────────────────────────────────────────
  const [modelSettings,   setModelSettingsState] = useState<ModelSettings>()
  const [modelEndpoint,   setModelEndpoint]      = useState('https://api.deepseek.com')
  const [modelName,       setModelName]          = useState('deepseek-flash')
  const [modelProvider,   setModelProvider]      = useState<ModelProviderId>('deepseek')
  const [apiKey,          setApiKey]             = useState('')
  const [savingSettings,  setSavingSettings]     = useState(false)
  const [proxyUrl,        setProxyUrl]           = useState('')
  const [savingNetwork,   setSavingNetwork]      = useState(false)
  const [storageStatus,   setStorageStatus]      = useState<StorageStatus>()
  const [storageAction,   setStorageAction]      = useState<'backup' | 'restore' | 'optimize' | null>(null)
  const [sourceConfigs,   setSourceConfigs]      = useState<SourceConfiguration[]>([])
  const [sourceDraft,     setSourceDraft]        = useState<SaveSourceConfiguration>()
  const [editingSourceCode, setEditingSourceCode] = useState<string>()
  const [sourceBuiltIn,   setSourceBuiltIn]      = useState(false)
  const [draggingSourceCode, setDraggingSourceCode] = useState<string>()
  const [sourceDropTarget, setSourceDropTarget] = useState<SourceDropTarget>()
  const [sourcePointerDrag, setSourcePointerDrag] = useState<SourcePointerDrag>()
  const sourcePointerDragRef = useRef<SourcePointerDrag | undefined>(undefined)
  const [reorderingSources, setReorderingSources] = useState(false)
  const [savingSource,    setSavingSource]       = useState(false)
  const selectedModelProvider = MODEL_PROVIDER_PRESETS.find((provider) => provider.id === modelProvider)
  const modelUsesPreset = selectedModelProvider?.models.includes(modelName) === true
  const enabledSourceConfigs = useMemo(
    () => sourceConfigs.filter((sourceConfig) => sourceConfig.enabled),
    [sourceConfigs],
  )
  /** Cascading filters expose only combinations backed by an enabled source. */
  const regionSourceConfigs = useMemo(
    () => enabledSourceConfigs.filter((sourceConfig) => region === 'all' || sourceConfig.region === region),
    [enabledSourceConfigs, region],
  )
  const linkedSourceConfigs = useMemo(
    () => regionSourceConfigs.filter((sourceConfig) => category === 'all' || sourceConfig.category === category),
    [category, regionSourceConfigs],
  )

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
      // Reuse the active unpinned blank tab created by the browser toggle. Each
      // distinct topic otherwise owns one tab, and repeat clicks only activate it.
      const reusable = existing === undefined
        ? currentTabs.find((tab) => (
            tab.id === activeBrowserTabIdRef.current
            && tab.url === ''
            && tab.topicId === undefined
            && tab.sourceUrl === undefined
            && tab.pinned !== true
          ))
        : undefined
      const targetId = existing?.id ?? reusable?.id ?? newId
      const topicTab: BrowserTab = {
        id: targetId,
        url: topic.url,
        title: topic.title,
        topicId: topic.id,
        sourceUrl: topic.url,
      }
      const next = existing
        ? currentTabs
        : reusable
          ? currentTabs.map((tab) => tab.id === reusable.id ? topicTab : tab)
          : [...currentTabs, topicTab]
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
    ...(view === 'new'   ? { recentOnly: true } : {}),
    ...(source   === 'all' ? {} : { source }),
    ...(region   === 'all' ? {} : { region }),
    ...(category === 'all' ? {} : { category }),
    ...(search.trim() === '' ? {} : { search: search.trim() }),
    sort, limit: PAGE_SIZE, offset: pageIndex * PAGE_SIZE,
  }), [category, pageIndex, region, search, sort, source, view])

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
    let disposed = false
    const unlisten = listen<CollectionStatusEvent>('collection-status', ({ payload }) => {
      if (disposed) return
      if (payload.phase === 'started') {
        setRefreshing(true)
        setError(undefined)
        setNotice({ scope: 'workspace', text: payload.message })
        return
      }
      setRefreshing(false)
      if (payload.phase === 'finished') {
        setNotice({ scope: 'workspace', text: payload.message })
        void load(true)
      } else {
        setError(payload.message)
      }
    })
    return () => {
      disposed = true
      void unlisten.then((stop) => stop()).catch(() => {})
    }
  }, [load])

  useEffect(() => {
    void listSourceConfigurations().then(setSourceConfigs)
      .catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)))
  }, [])

  useEffect(() => {
    if (region === 'all' || enabledSourceConfigs.some((sourceConfig) => sourceConfig.region === region)) return
    setRegion('all')
    setCategory('all')
    setSource('all')
    setPageIndex(0)
  }, [enabledSourceConfigs, region])

  useEffect(() => {
    if (category === 'all' || regionSourceConfigs.some((sourceConfig) => sourceConfig.category === category)) return
    setCategory('all')
    setSource('all')
    setPageIndex(0)
  }, [category, regionSourceConfigs])

  useEffect(() => {
    if (source === 'all' || linkedSourceConfigs.some((sourceConfig) => sourceConfig.code === source)) return
    setSource('all')
    setPageIndex(0)
  }, [linkedSourceConfigs, source])

  useEffect(() => {
    if (view !== 'settings') return
    void getModelSettings().then((s) => {
      setModelSettingsState(s)
      setModelEndpoint(s.endpoint)
      setModelName(s.model)
      setModelProvider(detectModelProvider(s.endpoint))
    }).catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)))
    void getNetworkSettings().then((s) => setProxyUrl(s.proxyUrl ?? ''))
      .catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)))
    void getStorageStatus().then(setStorageStatus)
      .catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)))
  }, [view])

  useEffect(() => {
    if (notice === undefined) return
    const timer = window.setTimeout(() => setNotice(undefined), 4000)
    return () => window.clearTimeout(timer)
  }, [notice])

  useEffect(() => {
    if (error === undefined) return
    const timer = window.setTimeout(() => setError(undefined), 4000)
    return () => window.clearTimeout(timer)
  }, [error])

  // ── Actions ──────────────────────────────────────────────────────
  const collect = async (): Promise<void> => {
    setRefreshing(true); setError(undefined); setNotice(undefined)
    try {
      const result = await refreshTopics()
      setNotice({ scope: 'workspace', text: result.message })
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

  const hideDislikedTopic = async (topic: TopicView): Promise<void> => {
    if (!window.confirm(m.topicHideConfirm(topic.title))) return
    setError(undefined)
    try {
      await hideTopic(topic.id)
      setNotice({ scope: 'workspace', text: m.topicHidden })
      await load(true)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }

  const changePlatform = async (code: string, enabled: boolean): Promise<void> => {
    setError(undefined)
    try {
      await setPlatformEnabled(code, enabled)
      setSourceConfigs((items) => items.map((item) => item.code === code ? { ...item, enabled } : item))
      await load()
    }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
  }

  const editSource = (source?: SourceConfiguration): void => {
    setError(undefined); setNotice(undefined)
    if (source !== undefined && editingSourceCode === source.code && sourceDraft !== undefined) {
      setEditingSourceCode(undefined)
      setSourceDraft(undefined)
      setSourceBuiltIn(false)
      return
    }
    setEditingSourceCode(source?.code)
    setSourceBuiltIn(source?.builtIn === true)
    setSourceDraft(source === undefined ? { ...EMPTY_SOURCE, parserConfig: {} } : {
      code: source.code, displayName: source.displayName, homeUrl: source.homeUrl,
      endpointUrl: source.endpointUrl, region: source.region, category: source.category,
      parserType: source.parserType, proxyMode: source.proxyMode, enabled: source.enabled,
      parserConfig: { ...source.parserConfig },
    })
  }

  const updateSourceDraft = (update: Partial<SaveSourceConfiguration>): void => {
    setSourceDraft((current) => current === undefined ? current : { ...current, ...update })
  }

  const updateParserConfig = (key: keyof SaveSourceConfiguration['parserConfig'], value: string): void => {
    setSourceDraft((current) => current === undefined ? current : {
      ...current,
      parserConfig: { ...current.parserConfig, [key]: value },
    })
  }

  const saveSource = async (): Promise<void> => {
    if (!sourceDraft) return
    setSavingSource(true); setError(undefined)
    try {
      setSourceConfigs(await saveSourceConfiguration(sourceDraft))
      setSourceDraft(undefined)
      setEditingSourceCode(undefined)
      setNotice({ scope: 'sources', text: m.sourceSaved })
      await load(true)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally { setSavingSource(false) }
  }

  const removeSource = async (source: SourceConfiguration): Promise<void> => {
    if (!window.confirm(m.sourceDeleteConfirm(source.displayName))) return
    setError(undefined)
    try {
      setSourceConfigs(await deleteSource(source.code))
      if (editingSourceCode === source.code) {
        setSourceDraft(undefined)
        setEditingSourceCode(undefined)
      }
      await load(true)
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
  }

  const exportSources = async (code?: string): Promise<void> => {
    setError(undefined)
    try {
      if (await exportSourceConfigurations(locale, code)) setNotice({ scope: 'sources', text: m.sourceExported })
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
  }

  const importSources = async (targetCode?: string): Promise<void> => {
    setError(undefined)
    try {
      const sources = await importSourceConfigurations(locale, targetCode)
      if (sources === undefined) return
      setSourceConfigs(sources)
      setSourceDraft(undefined)
      setEditingSourceCode(undefined)
      setNotice({ scope: 'sources', text: m.sourceImported })
      await load(true)
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
  }

  const restoreSources = async (): Promise<void> => {
    if (!window.confirm(m.sourceRestoreConfirm)) return
    setError(undefined)
    try {
      setSourceConfigs(await restoreDefaultSources())
      setSourceDraft(undefined)
      setEditingSourceCode(undefined)
      setNotice({ scope: 'sources', text: m.sourceDefaultsRestored })
      await load(true)
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
  }

  const persistSourceOrder = async (next: SourceConfiguration[]): Promise<void> => {
    if (reorderingSources || next.map((item) => item.code).join('\0') === sourceConfigs.map((item) => item.code).join('\0')) return
    const previous = sourceConfigs
    setSourceConfigs(next)
    setReorderingSources(true)
    setError(undefined)
    try {
      setSourceConfigs(await reorderSourceConfigurations(next.map((item) => item.code)))
    } catch (reason) {
      setSourceConfigs(previous)
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setReorderingSources(false)
    }
  }

  const dropSource = (targetCode: string, after: boolean, draggedCode: string): void => {
    setDraggingSourceCode(undefined)
    setSourceDropTarget(undefined)
    if (draggedCode === targetCode) return
    const dragged = sourceConfigs.find((item) => item.code === draggedCode)
    if (dragged === undefined) return
    const next = sourceConfigs.filter((item) => item.code !== draggedCode)
    const targetIndex = next.findIndex((item) => item.code === targetCode)
    if (targetIndex < 0) return
    next.splice(targetIndex + (after ? 1 : 0), 0, dragged)
    void persistSourceOrder(next)
  }

  const sourceTargetAt = (clientX: number, clientY: number, draggedCode: string): SourceDropTarget | undefined => {
    for (const element of document.elementsFromPoint(clientX, clientY)) {
      const item = element.closest<HTMLElement>('[data-source-code]')
      const code = item?.dataset.sourceCode
      if (item === null || code === undefined || code === draggedCode) continue
      const bounds = (item.querySelector<HTMLElement>(':scope > .source-row') ?? item).getBoundingClientRect()
      return { code, after: clientY >= bounds.top + bounds.height / 2 }
    }
    return undefined
  }

  const beginSourcePointerDrag = (event: ReactPointerEvent<HTMLDivElement>, code: string): void => {
    if (reorderingSources || event.button !== 0) return
    const origin = event.target
    if (origin instanceof Element && origin.closest('button, input, label, select, textarea, a')) return
    if (event.pointerType !== 'mouse' && origin instanceof Element && !origin.closest('.source-drag-handle')) return
    event.preventDefault()
    window.getSelection()?.removeAllRanges()
    event.currentTarget.setPointerCapture(event.pointerId)
    const drag = { code, pointerId: event.pointerId, startY: event.clientY, currentY: event.clientY, active: false }
    sourcePointerDragRef.current = drag
    setSourcePointerDrag(drag)
  }

  const moveSourcePointerDrag = (event: ReactPointerEvent<HTMLDivElement>): void => {
    const current = sourcePointerDragRef.current
    if (current === undefined || current.pointerId !== event.pointerId) return
    const active = current.active || Math.abs(event.clientY - current.startY) >= 4
    const next = { ...current, currentY: event.clientY, active }
    sourcePointerDragRef.current = next
    setSourcePointerDrag(next)
    if (!active) return
    event.preventDefault()
    setDraggingSourceCode(current.code)
    setSourceDropTarget(sourceTargetAt(event.clientX, event.clientY, current.code))
  }

  const finishSourcePointerDrag = (event: ReactPointerEvent<HTMLDivElement>): void => {
    const current = sourcePointerDragRef.current
    if (current === undefined || current.pointerId !== event.pointerId) return
    const target = current.active ? sourceTargetAt(event.clientX, event.clientY, current.code) : undefined
    sourcePointerDragRef.current = undefined
    setSourcePointerDrag(undefined)
    setDraggingSourceCode(undefined)
    setSourceDropTarget(undefined)
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
    if (target !== undefined) dropSource(target.code, target.after, current.code)
  }

  const cancelSourcePointerDrag = (): void => {
    sourcePointerDragRef.current = undefined
    setSourcePointerDrag(undefined)
    setDraggingSourceCode(undefined)
    setSourceDropTarget(undefined)
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

  /** A provider preset fills both routing fields; custom mode leaves them editable. */
  const changeModelProvider = (value: string): void => {
    const providerId = value as ModelProviderId
    setModelProvider(providerId)
    const preset = MODEL_PROVIDER_PRESETS.find((provider) => provider.id === providerId)
    if (preset === undefined) return
    setModelEndpoint(preset.endpoint)
    setModelName(preset.models[0] ?? '')
  }

  const changeModelPreset = (value: string): void => {
    if (value === '__custom__') {
      if (modelUsesPreset) setModelName('')
      return
    }
    setModelName(value)
  }

  const saveSettings = async (): Promise<void> => {
    setSavingSettings(true); setError(undefined)
    try {
      const s = await saveModelSettings({
        endpoint: modelEndpoint, model: modelName,
        ...(apiKey.trim() === '' ? {} : { apiKey: apiKey.trim() }),
      })
      setModelSettingsState(s); setApiKey(''); setNotice({ scope: 'model', text: m.saveNotice })
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally { setSavingSettings(false) }
  }

  const saveNetwork = async (): Promise<void> => {
    setSavingNetwork(true); setError(undefined); setNotice(undefined)
    try {
      const settings = await saveNetworkSettings({ proxyUrl })
      setProxyUrl(settings.proxyUrl ?? '')
      setNotice({ scope: 'network', text: m.networkSaveNotice })
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
      setNotice({ scope: 'storage', text: result.message })
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
    // The sidebar badge is the global recent-addition count. Do not carry a
    // Discover-page filter into Recent Additions and make its list look incomplete.
    if (next === 'new' && view !== 'new') {
      setSource('all')
      setRegion('all')
      setCategory('all')
      setSearch('')
      setSearchOpen(false)
    }
    setNotice(undefined); setError(undefined)
    setView(next); setPageIndex(0)
  }

  const totalPages = Math.max(1, Math.ceil((page?.total ?? 0) / PAGE_SIZE))

  /** Render the same editor either below the selected source row or above the list for a new source. */
  const renderSourceEditor = (): ReactNode => sourceDraft === undefined ? null : (
    <div className="source-editor">
      <div className="source-editor-title"><strong>{editingSourceCode === undefined ? m.sourceAdd : m.sourceEdit}</strong><button type="button" aria-label={m.sourceCancel} onClick={() => { setSourceDraft(undefined); setEditingSourceCode(undefined) }}><X aria-hidden="true" /></button></div>
      <div className="source-form-grid">
        <label><span>{m.sourceCode}</span><input value={sourceDraft.code} disabled={editingSourceCode !== undefined} onChange={(e) => updateSourceDraft({ code: e.target.value.toLowerCase() })} placeholder="my-source" /></label>
        <label><span>{m.sourceName}</span><input value={sourceDraft.displayName} onChange={(e) => updateSourceDraft({ displayName: e.target.value })} /></label>
        <label className="wide"><span>{m.sourceHome}</span><input value={sourceDraft.homeUrl} onChange={(e) => updateSourceDraft({ homeUrl: e.target.value })} placeholder="https://example.com/" /></label>
        <label className="wide"><span>{m.sourceEndpoint}</span><input value={sourceDraft.endpointUrl} onChange={(e) => updateSourceDraft({ endpointUrl: e.target.value })} placeholder="https://example.com/feed" /></label>
        <label><span>{m.sourceParser}</span><select value={sourceDraft.parserType} disabled={sourceBuiltIn} onChange={(e) => updateSourceDraft({ parserType: e.target.value as SaveSourceConfiguration['parserType'], parserConfig: {} })}>{sourceBuiltIn ? <option value="builtin">{m.sourceBuiltIn}</option> : null}<option value="rss">RSS / Atom</option><option value="json">JSON</option><option value="html">HTML</option></select></label>
        <label><span>{m.filterRegion}</span><select value={sourceDraft.region} onChange={(e) => updateSourceDraft({ region: e.target.value as SourceRegion })}><option value="domestic">{m.regionDomestic}</option><option value="international">{m.regionIntl}</option></select></label>
        <label><span>{m.filterCategory}</span><select value={sourceDraft.category} onChange={(e) => updateSourceDraft({ category: e.target.value as TopicCategory })}><option value="general">{m.filterGeneral}</option><option value="technology">{m.filterTech}</option><option value="finance">{m.filterFinance}</option><option value="developer">{m.filterDev}</option></select></label>
        <label><span>{m.sourceProxyMode}</span><select value={sourceDraft.proxyMode} onChange={(e) => updateSourceDraft({ proxyMode: e.target.value as SaveSourceConfiguration['proxyMode'] })}><option value="auto">{m.sourceAuto}</option><option value="direct">{m.sourceDirect}</option><option value="proxy">{m.sourceProxy}</option></select></label>
      </div>
      {sourceDraft.parserType === 'json' ? <div className="source-parser-grid">
        {([['itemsPath', m.sourceItemsPath, 'data.items'], ['titlePath', m.sourceTitlePath, 'title'], ['urlPath', m.sourceUrlPath, 'url'], ['idPath', m.sourceIdPath, 'id'], ['publishedPath', m.sourcePublishedPath, 'publishedAt'], ['rankPath', m.sourceRankPath, 'rank'], ['heatPath', m.sourceHeatPath, 'score']] as const).map(([key, label, placeholder]) => <label key={key}><span>{label}</span><input value={sourceDraft.parserConfig[key] ?? ''} placeholder={placeholder} onChange={(e) => updateParserConfig(key, e.target.value)} /></label>)}
      </div> : null}
      {sourceDraft.parserType === 'html' ? <div className="source-parser-grid">
        {([['itemSelector', m.sourceItemSelector, 'article'], ['titleSelector', m.sourceTitleSelector, 'h2'], ['linkSelector', m.sourceLinkSelector, 'a']] as const).map(([key, label, placeholder]) => <label key={key}><span>{label}</span><input value={sourceDraft.parserConfig[key] ?? ''} placeholder={placeholder} onChange={(e) => updateParserConfig(key, e.target.value)} /></label>)}
      </div> : null}
      <div className="source-editor-actions"><button type="button" onClick={() => { setSourceDraft(undefined); setEditingSourceCode(undefined) }}>{m.sourceCancel}</button><button className="primary-button" type="button" disabled={savingSource} onClick={() => void saveSource()}>{savingSource ? m.btnSaving : m.btnSave}</button></div>
    </div>
  )

  const renderToastLayer = (): ReactNode => notice === undefined && error === undefined ? null : (
    <div className="toast-stack" aria-live="polite">
      {notice !== undefined ? <div key={notice.text} className="notice app-toast" role="status"><span>{notice.text}</span><button type="button" aria-label="关闭" onClick={() => setNotice(undefined)}><X aria-hidden="true" /></button></div> : null}
      {error !== undefined ? <div key={error} className="error-banner app-toast" role="alert"><span>{error}</span><button type="button" aria-label="关闭" onClick={() => setError(undefined)}><X aria-hidden="true" /></button></div> : null}
    </div>
  )

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
        {renderToastLayer()}
        <header className="settings-fs-header">
          <button className="back-button" type="button" onClick={() => switchView(prevView)}>
            <ArrowLeft aria-hidden="true" />{m.settingsBack}
          </button>
          <span className="settings-fs-title">{m.settingsTitle}</span>
        </header>

        <div className="settings-layout">
          {/* Left tab nav */}
          <nav className="settings-tabs" aria-label={m.settingsTitle}>
            {(['general', 'sources', 'model', 'network', 'storage'] as SettingsTab[]).map((tab) => {
              const TabIcon = tabIcons[tab]
              return (
                <button
                  key={tab}
                  className={settingsTab === tab ? 'settings-tab active' : 'settings-tab'}
                  onClick={() => { setSettingsTab(tab); setNotice(undefined); setError(undefined) }}
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
              <div className="settings-page">
                <header className="settings-page-header"><div><h2>{m.tabGeneral}</h2><p>{m.settingsGeneralDesc}</p></div></header>
                <div className="settings-surface general-settings-card">
                  <section className="settings-group">
                    <div className="settings-group-heading"><strong>{m.sectionAppearance}</strong><span>{m.labelTheme}</span></div>
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
                  </section>
                  <section className="settings-group">
                    <div className="settings-group-heading"><strong>{m.sectionLanguage}</strong><span>{locale === 'zh' ? '界面显示语言' : 'Interface language'}</span></div>
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
                  </section>
                </div>
              </div>
            )}

            {settingsTab === 'network' && (
              /* Native collection traffic never crosses the WebView network boundary. */
              <div className="settings-page">
                <header className="settings-page-header"><div><h2>{m.sectionNetwork}</h2><p>{m.settingsNetworkDesc}</p></div></header>
                <div className="settings-surface network-setting">
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
              <div className="settings-page source-settings" aria-live="polite">
                <header className="settings-page-header source-settings-header">
                  <div><h2>{m.tabSources}</h2><p>{m.settingsSourcesDesc}</p></div>
                  <div className="source-batch-actions">
                    <button type="button" onClick={() => void importSources()}><Download aria-hidden="true" />{m.sourceImportAll}</button>
                    <button type="button" onClick={() => void exportSources()}><Upload aria-hidden="true" />{m.sourceExportAll}</button>
                    <button type="button" onClick={() => void restoreSources()}><RotateCcw aria-hidden="true" />{m.sourceRestoreDefaults}</button>
                    <button className="primary-button source-add-button" type="button" onClick={() => editSource()}><Plus aria-hidden="true" />{m.sourceAdd}</button>
                  </div>
                </header>

                {sourceDraft !== undefined && editingSourceCode === undefined ? renderSourceEditor() : null}
                <div className="source-list" aria-busy={reorderingSources}>
                  {sourceConfigs.map((sourceConfig) => {
                    const status = page?.statuses.find((item) => item.code === sourceConfig.code)
                    return <div
                      className={`source-list-item${draggingSourceCode === sourceConfig.code ? ' dragging' : ''}${sourceDropTarget?.code === sourceConfig.code ? (sourceDropTarget.after ? ' drop-after' : ' drop-before') : ''}`}
                      key={sourceConfig.code}
                      data-source-code={sourceConfig.code}
                    >
                      <div
                        className={`source-row${sourcePointerDrag?.code === sourceConfig.code && sourcePointerDrag.active ? ' pointer-dragging' : ''}`}
                        style={sourcePointerDrag?.code === sourceConfig.code && sourcePointerDrag.active ? { transform: `translateY(${sourcePointerDrag.currentY - sourcePointerDrag.startY}px)` } : undefined}
                        onPointerDown={(event) => beginSourcePointerDrag(event, sourceConfig.code)}
                        onPointerMove={moveSourcePointerDrag}
                        onPointerUp={finishSourcePointerDrag}
                        onPointerCancel={cancelSourcePointerDrag}
                      >
                        <span
                          className="source-drag-handle"
                          aria-hidden="true"
                          title={m.sourceDragToReorder}
                        ><GripVertical aria-hidden="true" /></span>
                        <span className={`health-dot ${status?.status === 'failed' ? 'failed' : status?.status === 'succeeded' ? 'healthy' : ''}`} />
                        <div className="source-row-info">
                          <span className="source-row-name">{sourceConfig.displayName}<small className="source-kind">{sourceConfig.builtIn ? m.sourceBuiltIn : `${m.sourceCustom} · ${sourceConfig.parserType.toUpperCase()}`}</small></span>
                          <span className="source-row-meta" title={status?.error ?? undefined}>{sourceConfig.region === 'domestic' ? m.regionDomestic : m.regionIntl} · {status?.error ?? (status?.lastRunAt == null ? m.notCollected : m.topicCount(status.topicCount))}</span>
                        </div>
                        {sourceConfig.code === 'xiaohongshu' ? <button className="source-action" type="button" onClick={openXiaohongshuSession}>{m.xhsLoginCollect}</button> : null}
                        <button className="source-icon-action" type="button" title={m.sourceImport} onClick={() => void importSources(sourceConfig.code)}><Download aria-hidden="true" /></button>
                        <button className="source-icon-action" type="button" title={m.sourceExport} onClick={() => void exportSources(sourceConfig.code)}><Upload aria-hidden="true" /></button>
                        <button className={`source-icon-action${editingSourceCode === sourceConfig.code ? ' active' : ''}`} type="button" title={m.sourceEdit} aria-expanded={editingSourceCode === sourceConfig.code} onClick={() => editSource(sourceConfig)}><Pencil aria-hidden="true" /></button>
                        <button className="source-icon-action danger" type="button" title={m.sourceDeleteConfirm(sourceConfig.displayName)} onClick={() => void removeSource(sourceConfig)}><Trash2 aria-hidden="true" /></button>
                        <label className="switch"><input type="checkbox" checked={sourceConfig.enabled} onChange={(e) => void changePlatform(sourceConfig.code, e.target.checked)} /><span /></label>
                      </div>
                      {editingSourceCode === sourceConfig.code ? renderSourceEditor() : null}
                    </div>
                  })}
                </div>
              </div>
            )}

            {settingsTab === 'storage' && (
              <div className="settings-page">
                <header className="settings-page-header"><div><h2>{m.storageHeading}</h2><p>{m.storageDesc}</p></div></header>
                <div className="settings-surface storage-card">
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
                  <div className="settings-card-actions storage-actions">
                    <button type="button" disabled={storageAction !== null} onClick={() => void openDataDirectory().catch((reason: unknown) => setError(reason instanceof Error ? reason.message : String(reason)))}><FolderOpen aria-hidden="true" />{m.storageOpenFolder}</button>
                    <button type="button" disabled={storageAction !== null} onClick={() => void runStorageAction('optimize')}><Wrench aria-hidden="true" />{storageAction === 'optimize' ? m.storageWorking : m.storageOptimize}</button>
                    <button type="button" disabled={storageAction !== null} onClick={() => void runStorageAction('backup')}><Archive aria-hidden="true" />{storageAction === 'backup' ? m.storageWorking : m.storageBackup}</button>
                    <button className="storage-restore" type="button" disabled={storageAction !== null || storageStatus?.latestBackup == null} onClick={() => void runStorageAction('restore')}><RotateCcw aria-hidden="true" />{storageAction === 'restore' ? m.storageWorking : m.storageRestore}</button>
                  </div>
                  {storageStatus !== undefined ? <code className="storage-path">{storageStatus.dataDirectory}</code> : null}
                </div>
              </div>
            )}

            {settingsTab === 'model' && (
              <div className="settings-page">
                <header className="settings-page-header"><div><h2>{m.modelHeading}</h2><p>{m.modelDesc}</p></div></header>
                <div className="settings-surface settings-card">
                  <div className="model-preset-grid">
                    <FilterDropdown
                      label={m.labelModelProvider}
                      value={modelProvider}
                      options={[...MODEL_PROVIDER_PRESETS.map((provider) => ({ value: provider.id, label: provider.label })), { value: 'custom', label: m.modelCustomProvider }]}
                      onChange={changeModelProvider}
                    />
                    <FilterDropdown
                      label={m.labelModel}
                      value={modelUsesPreset ? modelName : '__custom__'}
                      options={[...(selectedModelProvider?.models.map((model) => ({ value: model, label: model })) ?? []), { value: '__custom__', label: m.modelCustomName }]}
                      onChange={changeModelPreset}
                    />
                  </div>
                  <label>
                    <span>{m.labelEndpoint}</span>
                    <input value={modelEndpoint} readOnly={modelProvider !== 'custom'} onChange={(e) => setModelEndpoint(e.target.value)} placeholder="https://api.example.com/v1" />
                  </label>
                  {!modelUsesPreset ? <label>
                    <span>{m.labelModel}</span>
                    <input value={modelName} onChange={(e) => setModelName(e.target.value)} placeholder="model-name" />
                  </label> : null}
                  <label>
                    <span>{m.labelApiKey}</span>
                    <input
                      type="password" value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      placeholder={modelSettings?.hasApiKey === true ? m.apiKeySavedPlaceholder : m.apiKeyPlaceholder}
                      autoComplete="off"
                    />
                  </label>
                  <div className="settings-card-actions"><button className="primary-button settings-save" type="button" disabled={savingSettings} onClick={() => void saveSettings()}>
                    {savingSettings ? m.btnSaving : m.btnSave}
                  </button></div>
                </div>
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
      {renderToastLayer()}
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
            <Sparkles aria-hidden="true" />{m.navNew} <small>{page?.recentTotal ?? 0}</small>
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
            <div className={`topbar-search${searchOpen ? ' expanded' : ''}${search.trim() === '' ? '' : ' has-value'}`}>
              <button
                className="topbar-search-toggle"
                type="button"
                aria-label={m.filterSearch}
                aria-expanded={searchOpen}
                title={m.filterSearch}
                onClick={() => {
                  setSearchOpen(true)
                  requestAnimationFrame(() => searchInputRef.current?.focus())
                }}
              ><Search aria-hidden="true" /></button>
              <input
                ref={searchInputRef}
                aria-label={m.filterSearch}
                tabIndex={searchOpen ? 0 : -1}
                value={search}
                placeholder={m.filterSearchPlaceholder}
                onChange={(e) => { setSearch(e.target.value); setPageIndex(0) }}
                onBlur={() => setSearchOpen(false)}
                onKeyDown={(event) => {
                  if (event.key === 'Escape') {
                    setSearchOpen(false)
                    event.currentTarget.blur()
                  }
                }}
              />
            </div>
          </div>
          <div className="topbar-actions">
            <button className={`query-button topbar-icon-button${loading ? ' active busy' : ''}`} type="button" disabled={loading || refreshing} aria-label={loading ? m.btnQuerying : m.btnQuery} title={loading ? m.btnQuerying : m.btnQuery} onClick={() => void load()}>
              <RefreshCw aria-hidden="true" />
            </button>
            <button className={`query-button topbar-icon-button collect-button${refreshing ? ' active busy' : ''}`} type="button" disabled={refreshing} aria-label={refreshing ? m.btnRefreshing : m.btnRefresh} title={refreshing ? m.btnRefreshing : m.btnRefresh} onClick={() => void collect()}>
              <Radar aria-hidden="true" />
            </button>
            <button className={`side-panel-toggle topbar-icon-button${browserOpen ? ' active' : ''}`} type="button" aria-label={locale === 'zh' ? '显示/隐藏浏览器' : 'Show/hide browser'} aria-pressed={browserOpen} title={locale === 'zh' ? '显示/隐藏浏览器' : 'Show/hide browser'} onClick={toggleBrowser}><PanelRight aria-hidden="true" /></button>
          </div>
        </header>

        <div className="list-sticky-controls">
          <section className="filters" aria-label={m.filterSearch}>
            <FilterDropdown label={m.filterRegion} value={region} options={[{ value: 'all', label: m.filterAllRegions }, ...enabledSourceConfigs.some((item) => item.region === 'domestic') ? [{ value: 'domestic', label: m.filterDomestic }] : [], ...enabledSourceConfigs.some((item) => item.region === 'international') ? [{ value: 'international', label: m.filterInternational }] : []]} onChange={(value) => { setRegion(value as typeof region); setPageIndex(0) }} />
            <FilterDropdown label={m.filterCategory} value={category} options={[{ value: 'all', label: m.filterAllCategories }, ...regionSourceConfigs.some((item) => item.category === 'general') ? [{ value: 'general', label: m.filterGeneral }] : [], ...regionSourceConfigs.some((item) => item.category === 'technology') ? [{ value: 'technology', label: m.filterTech }] : [], ...regionSourceConfigs.some((item) => item.category === 'finance') ? [{ value: 'finance', label: m.filterFinance }] : [], ...regionSourceConfigs.some((item) => item.category === 'developer') ? [{ value: 'developer', label: m.filterDev }] : []]} onChange={(value) => { setCategory(value as typeof category); setPageIndex(0) }} />
            <FilterDropdown label={m.filterSource} value={source} options={[{ value: 'all', label: m.filterAllSources }, ...linkedSourceConfigs.map((item) => ({ value: item.code, label: item.displayName, removable: true }))]} onChange={(value) => { setSource(value); setPageIndex(0) }} onRemove={(code) => changePlatform(code, false)} removeLabel={(option) => m.filterDisableSource(option.label)} />
            <FilterDropdown label={m.filterSort} value={sort} options={[{ value: 'rank', label: m.filterSortRank }, { value: 'updated', label: m.filterSortUpdated }]} onChange={(value) => { setSort(value as typeof sort); setPageIndex(0) }} />
          </section>
          <div className="results-heading">
            <span>{m.resultsTotal(page?.total ?? 0)}</span>
            <span>{m.resultsErrors(page?.statuses.filter((s) => s.error !== null).length ?? 0)}</span>
          </div>
        </div>

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
              {...(view === 'discover' ? { onHide: hideDislikedTopic } : {})}
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
