/** 类型化 Tauri command 客户端，集中隔离前端与原生层通信细节。 */
import { invoke } from '@tauri-apps/api/core'
import type { ModelSettings, NetworkSettings, RefreshResult, SaveModelSettings, SaveNetworkSettings, SaveUiPreferences, TopicPage, TopicQuery, TranslationResult, UiPreferences } from './types'

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

/** Translate the trusted database title for one topic through the configured model. */
export async function translateTopic(topicId: number): Promise<TranslationResult> {
  return invoke<TranslationResult>('translate_topic', { topicId })
}

/** Bounds of the native article view in CSS pixels inside the main window. */
export interface BrowserBounds { x: number; y: number; width: number; height: number; viewportHeight: number }
let browserQueue: Promise<void> = Promise.resolve()

/** Serialize creation, resize, navigation and close to avoid stale native views. */
export function browserRequest(action: 'sync' | 'close' | 'closeAll' | 'hideAll' | 'back' | 'forward' | 'reload', tabId?: string, bounds?: BrowserBounds, url?: string): Promise<void> {
  const next = browserQueue.then(() => invoke<void>('browser_request', { action, tabId: tabId ?? null, bounds: bounds ?? null, url: url ?? null }))
  browserQueue = next.catch(() => {})
  return next
}

/** Native browser state is emitted only to the trusted main webview. */
export interface BrowserStatus { tabId: string; url: string; title: string; loading: boolean; canBack: boolean; canForward: boolean }
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
