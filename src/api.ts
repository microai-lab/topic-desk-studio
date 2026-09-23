/** 类型化 Tauri command 客户端，集中隔离前端与原生层通信细节。 */
import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import type { Locale } from './i18n'
import type { ModelSettings, NetworkSettings, RefreshResult, SaveModelSettings, SaveNetworkSettings, SaveSourceConfiguration, SaveUiPreferences, SourceConfiguration, StorageOperationResult, StorageStatus, TopicPage, TopicQuery, TranslationResult, UiPreferences } from './types'

/** 查询本地 SQLite 中的当前话题。 */
export async function listTopics(query: TopicQuery): Promise<TopicPage> {
  return invoke<TopicPage>('list_topics', { query })
}

/** 触发受控采集；采集器尚未就绪时后端会返回明确状态。 */
export async function refreshTopics(): Promise<RefreshResult> {
  return invoke<RefreshResult>('refresh_topics')
}

/** Import only normalized public cards from an ephemeral logged-in Xiaohongshu tab. */
export async function collectXiaohongshuSession(tabId: string): Promise<RefreshResult> {
  return invoke<RefreshResult>('collect_xiaohongshu_session', { tabId })
}

/** 幂等修改待创作状态。 */
export async function setTopicQueued(topicId: number, queued: boolean): Promise<void> {
  await invoke('set_topic_queued', { topicId, queued })
}

/** Persist one source switch while keeping historical rows available locally. */
export async function setPlatformEnabled(code: string, enabled: boolean): Promise<void> {
  await invoke('set_platform_enabled', { code, enabled })
}

/** Load source definitions for the native-only collection engine. */
export async function listSourceConfigurations(): Promise<SourceConfiguration[]> {
  return invoke<SourceConfiguration[]>('list_source_configurations')
}

/** Create or update a validated RSS, JSON or HTML source. */
export async function saveSourceConfiguration(source: SaveSourceConfiguration): Promise<SourceConfiguration[]> {
  return invoke<SourceConfiguration[]>('save_source_configuration', { source })
}

/** Remove one source while retaining its historical topics. */
export async function deleteSource(code: string): Promise<SourceConfiguration[]> {
  return invoke<SourceConfiguration[]>('delete_source', { code })
}

/** Export one source or the complete active list through a native save dialog. */
export async function exportSourceConfigurations(locale: Locale, code?: string): Promise<boolean> {
  const chinese = locale === 'zh'
  const path = await save({
    title: code === undefined
      ? (chinese ? '导出全部数据源' : 'Export all sources')
      : (chinese ? `导出数据源 ${code}` : `Export source ${code}`),
    defaultPath: code === undefined ? 'topic-desk-sources.json' : `topic-desk-source-${code}.json`,
    filters: [{ name: chinese ? 'Topic Desk 数据源' : 'Topic Desk sources', extensions: ['json'] }],
  })
  if (path === null) return false
  await invoke('export_source_configurations', { path, code: code ?? null })
  return true
}

/** Import one source or a batch through a native file dialog. */
export async function importSourceConfigurations(locale: Locale, targetCode?: string): Promise<SourceConfiguration[] | undefined> {
  const chinese = locale === 'zh'
  const path = await open({
    title: targetCode === undefined
      ? (chinese ? '批量导入数据源' : 'Import sources')
      : (chinese ? `导入数据源 ${targetCode}` : `Import source ${targetCode}`),
    multiple: false,
    directory: false,
    filters: [{ name: chinese ? 'Topic Desk 数据源' : 'Topic Desk sources', extensions: ['json'] }],
  })
  if (path === null) return undefined
  return invoke<SourceConfiguration[]>('import_source_configurations', { path, targetCode: targetCode ?? null })
}

/** Recreate and reset all catalog defaults without touching custom sources. */
export async function restoreDefaultSources(): Promise<SourceConfiguration[]> {
  return invoke<SourceConfiguration[]>('restore_default_sources')
}

/** Read model routing and only a boolean indicating whether SQLite contains a model key. */
export async function getModelSettings(): Promise<ModelSettings> {
  return invoke<ModelSettings>('get_model_settings')
}

/** Save model routing and optionally replace the API key in the native SQLite database. */
export async function saveModelSettings(settings: SaveModelSettings): Promise<ModelSettings> {
  return invoke<ModelSettings>('save_model_settings', { settings })
}

/** Read language and theme from native SQLite instead of persistent WebView storage. */
export async function getUiPreferences(): Promise<UiPreferences> {
  return invoke<UiPreferences>('get_ui_preferences')
}

/** Persist a complete language and theme selection in native SQLite. */
export async function saveUiPreferences(settings: SaveUiPreferences): Promise<UiPreferences> {
  return invoke<UiPreferences>('save_ui_preferences', { settings })
}

/** Read the explicit native collection proxy; null means direct access. */
export async function getNetworkSettings(): Promise<NetworkSettings> {
  return invoke<NetworkSettings>('get_network_settings')
}

/** Validate and persist an HTTP(S) proxy for future collection runs. */
export async function saveNetworkSettings(settings: SaveNetworkSettings): Promise<NetworkSettings> {
  return invoke<NetworkSettings>('save_network_settings', { settings })
}

/** Read aggregate database size, record counts, integrity and backup state. */
export async function getStorageStatus(): Promise<StorageStatus> {
  return invoke<StorageStatus>('get_storage_status')
}

/** Snapshot both SQLite databases and the optional credential key. */
export async function backupStorage(): Promise<StorageOperationResult> {
  return invoke<StorageOperationResult>('backup_storage')
}

/** Restore the newest snapshot created by this application. */
export async function restoreLatestBackup(): Promise<StorageOperationResult> {
  return invoke<StorageOperationResult>('restore_latest_backup')
}

/** Apply retention rules, optimize indexes and checkpoint WAL files. */
export async function optimizeStorage(): Promise<StorageOperationResult> {
  return invoke<StorageOperationResult>('optimize_storage')
}

/** Reveal the private application data folder in the system file manager. */
export async function openDataDirectory(): Promise<void> {
  await invoke('open_data_directory')
}

/** Translate the trusted database title for one topic through the configured model. */
export async function translateTopic(topicId: number): Promise<TranslationResult> {
  return invoke<TranslationResult>('translate_topic', { topicId })
}

/** Bounds of the native article view in CSS pixels inside the main window. */
export interface BrowserBounds { x: number; y: number; width: number; height: number; viewportHeight: number }
let browserQueue: Promise<void> = Promise.resolve()

/** Serialize creation, resize, navigation and close to avoid stale native views. */
export function browserRequest(action: 'sync' | 'close' | 'closeAll' | 'hideAll' | 'back' | 'forward' | 'reload' | 'mute' | 'unmute', tabId?: string, bounds?: BrowserBounds, url?: string): Promise<void> {
  const next = browserQueue.then(() => invoke<void>('browser_request', { action, tabId: tabId ?? null, bounds: bounds ?? null, url: url ?? null }))
  browserQueue = next.catch(() => {})
  return next
}

/** Native browser state is emitted only to the trusted main webview. */
export interface BrowserStatus { tabId: string; url: string; title: string; loading: boolean; canBack: boolean; canForward: boolean; muted: boolean }
/** Browser preferences persisted by Rust independently of topic settings. */
export interface BrowserSettings { searchEngine: 'bing' | 'google' | 'duckduckgo'; zoom: number; rememberHistory: boolean }
/** Local browser history or download metadata. */
export interface BrowserRecord { id: number; url: string; title: string; detail: string; time: number }
/** Complete local library for browser management panels. */
export interface BrowserLibrary { history: BrowserRecord[]; downloads: BrowserRecord[]; settings: BrowserSettings }
/** Explicit requests keep browser functionality behind a typed native boundary. */
export type BrowserAction =
  | { kind: 'library' | 'print' | 'screenshot' }
  | { kind: 'navigate'; input: string }
  | { kind: 'find'; text: string; backwards: boolean; sensitive: boolean }
  | { kind: 'zoom'; factor: number }
  | { kind: 'overlay'; visible: boolean; capture?: boolean }
  | { kind: 'settings'; settings: BrowserSettings }
  | { kind: 'clear'; history: boolean; cookies: boolean; downloads: boolean }
  | { kind: 'importCookies'; content: string }
  | { kind: 'revealDownload'; id: number }

/** Share one queue with native geometry updates so a close cannot race an overlay. */
export function browserControl<T = unknown>(request: BrowserAction): Promise<T> {
  const next = browserQueue.then(() => invoke<T>('browser_control', { request }))
  browserQueue = next.then(() => {}, () => {})
  return next
}
