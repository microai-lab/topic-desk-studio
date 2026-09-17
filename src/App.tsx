/** Topic Desk Studio 主界面，负责查询、筛选、刷新和待创作交互。 */
import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getModelSettings, listTopics, openExternalUrl, refreshTopics, saveModelSettings,
  setPlatformEnabled, setTopicQueued, translateTopic,
} from './api'
import { isEnglishTitle, rankTrendPoints } from './presentation'
import type { ModelSettings, SourceRegion, TopicCategory, TopicPage, TopicQuery, TopicView } from './types'

const PAGE_SIZE = 20
type ViewMode = 'discover' | 'queue' | 'new' | 'sources' | 'settings'
interface TranslationState { readonly loading?: boolean; readonly text?: string; readonly error?: string }

/** 将持久化时间格式化为当前系统语言的紧凑日期。 */
function formatTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value))
}

/** Compact rank history where a visually higher point represents a better (smaller) rank. */
function RankTrend({ values }: { readonly values: number[] }) {
  if (values.length < 2) return <span className="trend-placeholder">趋势积累中</span>
  const points = rankTrendPoints(values)
  return <svg className="rank-trend" viewBox="0 0 72 24" aria-label={`最近排名 ${values.join('、')}`}><polyline points={points} /></svg>
}

/** 单条话题卡片；原文始终交给系统浏览器打开。 */
function TopicCard({ topic, translation, onQueueChange, onTranslate }: {
  readonly topic: TopicView
  readonly translation?: TranslationState
  readonly onQueueChange: (topic: TopicView, queued: boolean) => Promise<void>
  readonly onTranslate: (topic: TopicView) => Promise<void>
}) {
  const [saving, setSaving] = useState(false)

  const toggleQueue = async (): Promise<void> => {
    setSaving(true)
    try {
      await onQueueChange(topic, !topic.queued)
    } finally {
      setSaving(false)
    }
  }

  return (
    <article className="topic-card">
      <div className="topic-rank" aria-label={`平台排名 ${topic.rank}`}>
        <span>{topic.rank}</span>
        <small>RANK</small>
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
        {isEnglishTitle(topic.title) ? <div className="translation-row">
          <button className="text-button" type="button" disabled={translation?.loading === true || translation?.text !== undefined} onClick={() => void onTranslate(topic)}>
            {translation?.loading === true ? '翻译中…' : translation?.text === undefined ? '译为中文' : '已翻译'}
          </button>
          {translation?.text !== undefined ? <span lang="zh-CN">{translation.text}</span> : null}
          {translation?.error !== undefined ? <span className="translation-error">{translation.error}</span> : null}
        </div> : null}
        <div className="topic-footer">
          <span>首次发现 {formatTime(topic.firstSeenAt)} · 连续上榜 {topic.consecutiveRuns} 轮</span>
          <RankTrend values={topic.trend} />
          <button className="text-button" type="button" disabled={saving} onClick={() => void toggleQueue()}>
            {saving ? '保存中…' : topic.queued ? '移出待创作' : '加入待创作'}
          </button>
        </div>
      </div>
    </article>
  )
}

/** 应用主组件；筛选变化后以防抖方式从 SQLite 重新查询。 */
export function App() {
  const [view, setView] = useState<ViewMode>('discover')
  const [region, setRegion] = useState<'all' | SourceRegion>('all')
  const [category, setCategory] = useState<'all' | TopicCategory>('all')
  const [source, setSource] = useState('all')
  const [sort, setSort] = useState<'rank' | 'updated'>('rank')
  const [insertedTopicIds, setInsertedTopicIds] = useState<number[]>([])
  const [search, setSearch] = useState('')
  const [pageIndex, setPageIndex] = useState(0)
  const [page, setPage] = useState<TopicPage>()
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [notice, setNotice] = useState<string>()
  const [error, setError] = useState<string>()
  const [translations, setTranslations] = useState<Record<number, TranslationState>>({})
  const [modelSettings, setModelSettingsState] = useState<ModelSettings>()
  const [modelEndpoint, setModelEndpoint] = useState('https://api.deepseek.com')
  const [modelName, setModelName] = useState('deepseek-chat')
  const [apiKey, setApiKey] = useState('')
  const [savingSettings, setSavingSettings] = useState(false)

  const query = useMemo<TopicQuery>(() => ({
    ...(view === 'queue' ? { queuedOnly: true } : {}),
    ...(view === 'new' ? { topicIds: insertedTopicIds } : {}),
    ...(source === 'all' ? {} : { source }),
    ...(region === 'all' ? {} : { region }),
    ...(category === 'all' ? {} : { category }),
    ...(search.trim() === '' ? {} : { search: search.trim() }),
    sort,
    limit: PAGE_SIZE,
    offset: pageIndex * PAGE_SIZE,
  }), [category, insertedTopicIds, pageIndex, region, search, sort, source, view])

  const load = useCallback(async (): Promise<void> => {
    setLoading(true)
    setError(undefined)
    try {
      setPage(await listTopics(query))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }, [query])

  useEffect(() => {
    const timer = window.setTimeout(() => void load(), 160)
    return () => window.clearTimeout(timer)
  }, [load])

  useEffect(() => {
    if (view !== 'settings') return
    void getModelSettings().then((settings) => {
      setModelSettingsState(settings)
      setModelEndpoint(settings.endpoint)
      setModelName(settings.model)
    }).catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)))
  }, [view])

  const collect = async (): Promise<void> => {
    setRefreshing(true)
    setError(undefined)
    setNotice(undefined)
    try {
      const result = await refreshTopics()
      setNotice(result.message)
      setInsertedTopicIds(result.insertedTopicIds)
      if (result.insertedTopicIds.length > 0) {
        setView('new')
        setPageIndex(0)
      }
      await load()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setRefreshing(false)
    }
  }

  const changeQueue = async (topic: TopicView, queued: boolean): Promise<void> => {
    await setTopicQueued(topic.id, queued)
    await load()
  }

  const changePlatform = async (code: string, enabled: boolean): Promise<void> => {
    setError(undefined)
    try {
      await setPlatformEnabled(code, enabled)
      await load()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }

  const translate = async (topic: TopicView): Promise<void> => {
    setTranslations((current) => ({ ...current, [topic.id]: { loading: true } }))
    try {
      const result = await translateTopic(topic.id)
      setTranslations((current) => ({ ...current, [topic.id]: { text: result.translation } }))
    } catch (reason) {
      setTranslations((current) => ({ ...current, [topic.id]: { error: reason instanceof Error ? reason.message : String(reason) } }))
    }
  }

  const saveSettings = async (): Promise<void> => {
    setSavingSettings(true)
    setError(undefined)
    try {
      const settings = await saveModelSettings({ endpoint: modelEndpoint, model: modelName, ...(apiKey.trim() === '' ? {} : { apiKey: apiKey.trim() }) })
      setModelSettingsState(settings)
      setApiKey('')
      setNotice('模型设置已保存，API Key 已交给系统凭据库管理。')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSavingSettings(false)
    }
  }

  const switchView = (next: ViewMode): void => {
    setView(next)
    setPageIndex(0)
  }

  const totalPages = Math.max(1, Math.ceil((page?.total ?? 0) / PAGE_SIZE))

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">TD</span>
          <div><strong>Topic Desk</strong><small>STUDIO</small></div>
        </div>
        <nav aria-label="主要视图">
          <button className={view === 'discover' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('discover')}>
            <span>◫</span>发现选题
          </button>
          <button className={view === 'queue' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('queue')}>
            <span>◇</span>待创作 <small>{page?.queuedTotal ?? 0}</small>
          </button>
          <button className={view === 'new' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('new')}>
            <span>✦</span>本轮新增 <small>{insertedTopicIds.length}</small>
          </button>
          <button className={view === 'sources' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('sources')}>
            <span>⌁</span>数据来源 <small>{page?.statuses.filter((status) => status.enabled).length ?? 0}</small>
          </button>
          <button className={view === 'settings' ? 'nav-item active' : 'nav-item'} onClick={() => switchView('settings')}>
            <span>⚙</span>模型设置
          </button>
        </nav>
        <div className="sidebar-status">
          <span className="status-dot" />
          <div><strong>本地数据</strong><small>SQLite · 独立运行</small></div>
        </div>
      </aside>

      <main>
        <header className="topbar">
          <div>
            <p className="eyebrow">CURATION WORKSPACE</p>
            <h1>{view === 'discover' ? '发现选题' : view === 'queue' ? '待创作清单' : view === 'new' ? '本轮新增' : view === 'sources' ? '数据来源' : '模型设置'}</h1>
          </div>
          <button className="primary-button" type="button" disabled={refreshing} onClick={() => void collect()}>
            {refreshing ? '正在刷新…' : '刷新数据'}
          </button>
        </header>

        {!['sources', 'settings'].includes(view) ? <section className="filters" aria-label="筛选条件">
          <label>
            <span>来源</span>
            <select value={source} onChange={(event) => { setSource(event.target.value); setPageIndex(0) }}>
              <option value="all">全部来源</option>
              {page?.statuses.map((status) => <option key={status.code} value={status.code}>{status.displayName}</option>)}
            </select>
          </label>
          <label>
            <span>地区</span>
            <select value={region} onChange={(event) => { setRegion(event.target.value as typeof region); setPageIndex(0) }}>
              <option value="all">全部地区</option>
              <option value="domestic">国内</option>
              <option value="international">国外</option>
            </select>
          </label>
          <label>
            <span>分类</span>
            <select value={category} onChange={(event) => { setCategory(event.target.value as typeof category); setPageIndex(0) }}>
              <option value="all">全部分类</option>
              <option value="general">综合</option>
              <option value="technology">科技与 AI</option>
              <option value="finance">财经市场</option>
              <option value="developer">开发者</option>
            </select>
          </label>
          <label className="search-field">
            <span>搜索</span>
            <input value={search} placeholder="输入标题关键词…" onChange={(event) => { setSearch(event.target.value); setPageIndex(0) }} />
          </label>
          <label>
            <span>排序</span>
            <select value={sort} onChange={(event) => { setSort(event.target.value as typeof sort); setPageIndex(0) }}>
              <option value="rank">榜单排名</option>
              <option value="updated">最近更新</option>
            </select>
          </label>
        </section> : null}

        {notice !== undefined ? <div className="notice">{notice}</div> : null}
        {error !== undefined ? <div className="error-banner">{error}</div> : null}

        {view === 'sources' ? (
          <section className="source-grid" aria-live="polite">
            {page?.statuses.map((status) => (
              <article className="source-card" key={status.code}>
                <div>
                  <strong>{status.displayName}</strong>
                  <small>{status.code} · {status.region === 'domestic' ? '国内' : '国际'}</small>
                </div>
                <div className="source-health">
                  <span className={`health-dot ${status.status === 'failed' ? 'failed' : status.status === 'succeeded' ? 'healthy' : ''}`} />
                  <span>{status.error ?? (status.lastRunAt === null ? '尚未采集' : `${status.topicCount} 条`)}</span>
                </div>
                <label className="switch">
                  <input type="checkbox" checked={status.enabled} onChange={(event) => void changePlatform(status.code, event.target.checked)} />
                  <span />
                </label>
              </article>
            ))}
          </section>
        ) : view === 'settings' ? (
          <section className="settings-card">
            <div>
              <p className="eyebrow">OPENAI-COMPATIBLE</p>
              <h2>英文标题翻译</h2>
              <p>接口地址与模型名保存在本地 SQLite；API Key 仅进入操作系统凭据库，不会返回页面。</p>
            </div>
            <label><span>接口地址</span><input value={modelEndpoint} onChange={(event) => setModelEndpoint(event.target.value)} placeholder="https://api.deepseek.com" /></label>
            <label><span>模型名称</span><input value={modelName} onChange={(event) => setModelName(event.target.value)} placeholder="deepseek-chat" /></label>
            <label><span>API Key</span><input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={modelSettings?.hasApiKey === true ? '已安全保存；留空表示不修改' : '输入后保存到系统凭据库'} autoComplete="off" /></label>
            <button className="primary-button settings-save" type="button" disabled={savingSettings} onClick={() => void saveSettings()}>{savingSettings ? '保存中…' : '保存设置'}</button>
          </section>
        ) : <section className="results" aria-live="polite">
          <div className="results-heading">
            <span><strong>{page?.total ?? 0}</strong> 个选题</span>
            <span>{page?.statuses.filter((status) => status.error !== null).length ?? 0} 个来源异常</span>
          </div>
          {loading ? <div className="empty-state"><div className="loader" /><p>正在读取本地选题库…</p></div> : null}
          {!loading && page?.topics.length === 0 ? (
            <div className="empty-state">
              <div className="empty-glyph">✦</div>
              <h2>本地选题库还是空的</h2>
              <p>点击“刷新数据”开始采集，之后可以在这里筛选、追踪并加入待创作。</p>
            </div>
          ) : null}
          {!loading ? page?.topics.map((topic) => (
            <TopicCard key={topic.id} topic={topic} {...(translations[topic.id] === undefined ? {} : { translation: translations[topic.id] })} onQueueChange={changeQueue} onTranslate={translate} />
          )) : null}
        </section>}

        {!['sources', 'settings'].includes(view) ? <footer className="pagination">
          <button disabled={pageIndex === 0} onClick={() => setPageIndex((value) => Math.max(0, value - 1))}>上一页</button>
          <span>{pageIndex + 1} / {totalPages}</span>
          <button disabled={pageIndex + 1 >= totalPages} onClick={() => setPageIndex((value) => value + 1)}>下一页</button>
        </footer> : null}
      </main>
    </div>
  )
}
