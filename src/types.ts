/** 前端与 Rust commands 共享的序列化契约。 */

export type SourceRegion = 'domestic' | 'international'
export type TopicCategory = 'general' | 'technology' | 'finance' | 'developer'
export type TopicSort = 'rank' | 'updated'

/** 页面展示的一条持久化话题。 */
export interface TopicView {
  readonly id: number
  readonly platformCode: string
  readonly platformName: string
  readonly category: TopicCategory
  readonly title: string
  readonly url: string
  readonly publishedTime: string | null
  readonly globalRank: number
  readonly rank: number
  readonly heat: number | null
  readonly firstSeenAt: string
  readonly updatedAt: string
  readonly rankDelta: number | null
  readonly consecutiveRuns: number
  readonly trend: number[]
  readonly queued: boolean
  readonly queuedAt: string | null
}

/** 来源最近一次采集的健康状态。 */
export interface PlatformStatusView {
  readonly code: string
  readonly displayName: string
  readonly region: SourceRegion
  readonly category: TopicCategory
  readonly enabled: boolean
  readonly status: string | null
  readonly lastRunAt: string | null
  readonly error: string | null
  readonly topicCount: number
}

/** SQLite 查询参数；所有可选字段均在 Rust 边界再次校验。 */
export interface TopicQuery {
  readonly source?: string
  readonly region?: SourceRegion
  readonly category?: TopicCategory
  readonly search?: string
  readonly sort?: TopicSort
  readonly queuedOnly?: boolean
  readonly topicIds?: number[]
  readonly limit?: number
  readonly offset?: number
}

/** 分页查询结果及来源健康摘要。 */
export interface TopicPage {
  readonly topics: TopicView[]
  readonly total: number
  readonly queuedTotal: number
  readonly statuses: PlatformStatusView[]
  readonly historyEnabled: boolean
}

/** 手动刷新结果；后续进度细节通过 Tauri events 推送。 */
export interface RefreshResult {
  readonly accepted: boolean
  readonly message: string
  readonly inserted: number
  readonly updated: number
  readonly insertedTopicIds: number[]
}

/** Non-secret model routing returned by Rust; API key contents never cross back into the WebView. */
export interface ModelSettings {
  readonly endpoint: string
  readonly model: string
  readonly hasApiKey: boolean
}

/** Settings update; an empty API key keeps the credential already stored by the operating system. */
export interface SaveModelSettings {
  readonly endpoint: string
  readonly model: string
  readonly apiKey?: string
}

/** One on-demand title translation. */
export interface TranslationResult {
  readonly topicId: number
  readonly translation: string
}
