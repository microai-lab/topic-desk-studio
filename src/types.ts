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
  readonly recentOnly?: boolean
  readonly topicIds?: number[]
  readonly limit?: number
  readonly offset?: number
}

/** 分页查询结果及来源健康摘要。 */
export interface TopicPage {
  readonly topics: TopicView[]
  readonly total: number
  readonly queuedTotal: number
  readonly recentTotal: number
  readonly statuses: PlatformStatusView[]
  readonly historyEnabled: boolean
}

/** Native lifecycle notification shared by manual and scheduled collection runs. */
export interface CollectionStatusEvent {
  readonly phase: 'started' | 'finished' | 'failed'
  readonly trigger: 'manual' | 'startup' | 'schedule' | string
  readonly message: string
  readonly inserted: number
  readonly updated: number
}

/** 手动刷新结果；后续进度细节通过 Tauri events 推送。 */
export interface RefreshResult {
  readonly accepted: boolean
  readonly message: string
  readonly inserted: number
  readonly updated: number
  readonly insertedTopicIds: number[]
}

/** Model routing returned by Rust; API key contents never cross back into the WebView. */
export interface ModelSettings {
  readonly endpoint: string
  readonly model: string
  readonly hasApiKey: boolean
}

/** Settings update; an empty API key keeps the credential already stored in SQLite. */
export interface SaveModelSettings {
  readonly endpoint: string
  readonly model: string
  readonly apiKey?: string
}

/** UI preferences persisted by Rust because the main WebView uses temporary storage. */
export interface UiPreferences {
  readonly locale: 'zh' | 'en' | null
  readonly theme: 'light' | 'dark' | 'system' | null
}

/** Complete validated UI preference update sent through the native boundary. */
export interface SaveUiPreferences {
  readonly locale: 'zh' | 'en'
  readonly theme: 'light' | 'dark' | 'system'
}

/** Native collection proxy; null means direct network access. */
export interface NetworkSettings {
  readonly proxyUrl: string | null
}

/** A blank value clears the explicitly configured collection proxy. */
export interface SaveNetworkSettings {
  readonly proxyUrl: string
}

/** Aggregate native storage health; no persisted content or credential crosses this boundary. */
export interface StorageStatus {
  readonly dataDirectory: string
  readonly topicDatabaseBytes: number
  readonly browserDatabaseBytes: number
  readonly topicCount: number
  readonly observationCount: number
  readonly collectionRunCount: number
  readonly browserRecordCount: number
  readonly integrityOk: boolean
  readonly latestBackup: string | null
}

/** Result returned by a local backup, restore or maintenance command. */
export interface StorageOperationResult {
  readonly message: string
  readonly backupName: string | null
}

/** One on-demand title translation. */
export interface TranslationResult {
  readonly topicId: number
  readonly translation: string
}
