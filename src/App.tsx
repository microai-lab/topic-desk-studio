/** Topic Desk Studio — main UI component. */
import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getModelSettings, listTopics, openExternalUrl, refreshTopics, saveModelSettings,
  setPlatformEnabled, setTopicQueued, translateTopic,
} from './api'
import { isEnglishTitle, rankTrendPoints } from './presentation'
import {
  Locale, Theme, Messages, messages,
  detectLocale, detectTheme, applyTheme,
} from './i18n'
import type { ModelSettings, SourceRegion, TopicCategory, TopicPage, TopicQuery, TopicView } from './types'

const PAGE_SIZE = 20
type ViewMode = 'discover' | 'queue' | 'new' | 'settings'
type SettingsTab = 'general' | 'sources' | 'model'
interface TranslationState { readonly loading?: boolean; readonly text?: string; readonly error?: string }

function formatTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  }).format(new Date(value))
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

function TopicCard({ topic, translation, m, onQueueChange, onTranslate }: {
  readonly topic: TopicView
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
    <article className="topic-card">
      <div className="topic-rank" aria-label={`${m.rankLabel} ${topic.rank}`}>
        <span>{topic.rank}</span>
        <small>{m.rankLabel}</small>
      </div>
      <div className="topic-content">
        <div className="topic-meta">
          <span className="source-pill">{topic.platformName}</span>
          <span>{formatTime(topic.updatedAt)}</span>
          {topic.rankDelta !== null && topic.rankDelta !== 0 ? (
            <span className={topic.rankDelta > 0 ? 'rank-up' : 'rank-down'}>
              {topic.rankDelta > 0 ? `↑ ${topic.rankDelta}` : `↓ ${Math.abs(topic.rankDelta)}`}
            </span>
          ) : null}
        </div>
        <button className="topic-title" type="button" onClick={() => void openExternalUrl(topic.url)}>
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

export function App() {
  // ── Locale & theme ──────────────────────────────────────────────
  const [locale, setLocale] = useState<Locale>(detectLocale)
  const [theme, setTheme]   = useState<Theme>(detectTheme)
  const m = messages[locale]

  useEffect(() => {
    applyTheme(theme)
  }, [theme])

  useEffect(() => {
    localStorage.setItem('tds-locale', locale)
  }, [locale])

  // ── View state ──────────────────────────────────────────────────
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
  const load = useCallback(async (): Promise<void> => {
    setLoading(true)
    setError(undefined)
    try { setPage(await listTopics(query)) }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setLoading(false) }
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
  }, [view])

  // ── Actions ──────────────────────────────────────────────────────
  const collect = async (): Promise<void> => {
    setRefreshing(true); setError(undefined); setNotice(undefined)
    try {
      const result = await refreshTopics()
      setNotice(result.message)
      setInsertedTopicIds(result.insertedTopicIds)
      if (result.insertedTopicIds.length > 0) { setView('new'); setPageIndex(0) }
      await load()
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

  const switchView = (next: ViewMode): void => {
    if (next === 'settings' && view !== 'settings') {
      setPrevView(view as Exclude<ViewMode, 'settings'>)
    }
    setView(next); setPageIndex(0)
  }

  const totalPages = Math.max(1, Math.ceil((page?.total ?? 0) / PAGE_SIZE))

  // ── Settings fullscreen ──────────────────────────────────────────
  if (view === 'settings') {
    const tabIcons: Record<SettingsTab, string> = { general: '⊙', sources: '⌁', model: '◎' }
    return (
      <div className="settings-fullscreen">
        <header className="settings-fs-header">
          <button className="back-button" type="button" onClick={() => switchView(prevView)}>
            <span>←</span>{m.settingsBack}
          </button>
          <span className="settings-fs-title">{m.settingsTitle}</span>
        </header>

        <div className="settings-layout">
          {/* Left tab nav */}
          <nav className="settings-tabs" aria-label={m.settingsTitle}>
            {(['general', 'sources', 'model'] as SettingsTab[]).map((tab) => (
              <button
                key={tab}
                className={settingsTab === tab ? 'settings-tab active' : 'settings-tab'}
                onClick={() => setSettingsTab(tab)}
              >
                <span className="settings-tab-icon">{tabIcons[tab]}</span>
                {tab === 'general' ? m.tabGeneral : tab === 'sources' ? m.tabSources : m.tabModel}
              </button>
            ))}
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

            {settingsTab === 'sources' && (
              <div className="source-list" aria-live="polite">
                {page?.statuses.map((status) => (
                  <div className="source-row" key={status.code}>
                    <span className={`health-dot ${status.status === 'failed' ? 'failed' : status.status === 'succeeded' ? 'healthy' : ''}`} />
                    <div className="source-row-info">
                      <span className="source-row-name">{status.displayName}</span>
                      <span className="source-row-meta">
                        {status.region === 'domestic' ? m.regionDomestic : m.regionIntl} · {status.error ?? (status.lastRunAt === null ? m.notCollected : m.topicCount(status.topicCount))}
                      </span>
                    </div>
                    <label className="switch">
                      <input type="checkbox" checked={status.enabled} onChange={(e) => void changePlatform(status.code, e.target.checked)} />
                      <span />
                    </label>
                  </div>
                ))}
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
            <span>◫</span>{m.navDiscover}
          </button>
          <button className={view === 'queue' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('queue')}>
            <span>◇</span>{m.navQueue} <small>{page?.queuedTotal ?? 0}</small>
          </button>
          <button className={view === 'new' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('new')}>
            <span>✦</span>{m.navNew} <small>{insertedTopicIds.length}</small>
          </button>
        </nav>

        <div className="sidebar-footer">
          <button className="nav-item" onClick={() => switchView('settings')}>
            <span>⚙</span>{m.navSettings}
          </button>
        </div>
      </aside>

      <main>
        <header className="topbar">
          <h1>{viewTitle}</h1>
          <button className="primary-button" type="button" disabled={refreshing} onClick={() => void collect()}>
            {refreshing ? m.btnRefreshing : m.btnRefresh}
          </button>
        </header>

        <section className="filters" aria-label={m.filterSearch}>
          <label>
            <span>{m.filterSource}</span>
            <select value={source} onChange={(e) => { setSource(e.target.value); setPageIndex(0) }}>
              <option value="all">{m.filterAllSources}</option>
              {page?.statuses.map((s) => <option key={s.code} value={s.code}>{s.displayName}</option>)}
            </select>
          </label>
          <label>
            <span>{m.filterRegion}</span>
            <select value={region} onChange={(e) => { setRegion(e.target.value as typeof region); setPageIndex(0) }}>
              <option value="all">{m.filterAllRegions}</option>
              <option value="domestic">{m.filterDomestic}</option>
              <option value="international">{m.filterInternational}</option>
            </select>
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
          </label>
          <label className="search-field">
            <span>{m.filterSearch}</span>
            <input value={search} placeholder={m.filterSearchPlaceholder} onChange={(e) => { setSearch(e.target.value); setPageIndex(0) }} />
          </label>
          <label>
            <span>{m.filterSort}</span>
            <select value={sort} onChange={(e) => { setSort(e.target.value as typeof sort); setPageIndex(0) }}>
              <option value="rank">{m.filterSortRank}</option>
              <option value="updated">{m.filterSortUpdated}</option>
            </select>
          </label>
        </section>

        {notice !== undefined ? <div className="notice">{notice}</div> : null}
        {error  !== undefined ? <div className="error-banner">{error}</div> : null}

        <section className="results" aria-live="polite">
          <div className="results-heading">
            <span>{m.resultsTotal(page?.total ?? 0)}</span>
            <span>{m.resultsErrors(page?.statuses.filter((s) => s.error !== null).length ?? 0)}</span>
          </div>
          {loading ? (
            <div className="empty-state"><div className="loader" /><p>{m.resultsLoading}</p></div>
          ) : null}
          {!loading && page?.topics.length === 0 ? (
            <div className="empty-state">
              <div className="empty-glyph">✦</div>
              <h2>{m.resultsEmptyTitle}</h2>
              <p>{m.resultsEmptyBody}</p>
            </div>
          ) : null}
          {!loading ? page?.topics.map((topic) => (
            <TopicCard
              key={topic.id} topic={topic} m={m}
              {...(translations[topic.id] === undefined ? {} : { translation: translations[topic.id] })}
              onQueueChange={changeQueue} onTranslate={translate}
            />
          )) : null}
        </section>

        <footer className="pagination">
          <button disabled={pageIndex === 0} onClick={() => setPageIndex((v) => Math.max(0, v - 1))}>{m.prev}</button>
          <span>{pageIndex + 1} / {totalPages}</span>
          <button disabled={pageIndex + 1 >= totalPages} onClick={() => setPageIndex((v) => v + 1)}>{m.next}</button>
        </footer>
      </main>
    </div>
  )
}
