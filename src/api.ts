/** 类型化 Tauri command 客户端，集中隔离前端与原生层通信细节。 */
import { invoke } from '@tauri-apps/api/core'
import type { ModelSettings, RefreshResult, SaveModelSettings, TopicPage, TopicQuery, TranslationResult } from './types'

/** 查询本地 SQLite 中的当前话题。 */
export async function listTopics(query: TopicQuery): Promise<TopicPage> {
  return invoke<TopicPage>('list_topics', { query })
}

/** 触发受控采集；采集器尚未就绪时后端会返回明确状态。 */
export async function refreshTopics(): Promise<RefreshResult> {
  return invoke<RefreshResult>('refresh_topics')
}

/** 幂等修改待创作状态。 */
export async function setTopicQueued(topicId: number, queued: boolean): Promise<void> {
  await invoke('set_topic_queued', { topicId, queued })
}

/** Persist one source switch while keeping historical rows available locally. */
export async function setPlatformEnabled(code: string, enabled: boolean): Promise<void> {
  await invoke('set_platform_enabled', { code, enabled })
}

/** Read model routing and only a boolean indicating whether the OS vault contains a key. */
export async function getModelSettings(): Promise<ModelSettings> {
  return invoke<ModelSettings>('get_model_settings')
}

/** Save model routing and optionally replace the API key in the OS credential vault. */
export async function saveModelSettings(settings: SaveModelSettings): Promise<ModelSettings> {
  return invoke<ModelSettings>('save_model_settings', { settings })
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
/** Library records contain metadata only, never password values. */
export interface BrowserRecord { id: number; url: string; title: string; detail: string; time: number }
/** Complete local library for browser management panels. */
export interface BrowserLibrary { history: BrowserRecord[]; downloads: BrowserRecord[]; passwords: BrowserRecord[]; settings: BrowserSettings }
/** Explicit requests keep sensitive functionality behind a typed native boundary. */
export type BrowserAction =
  | { kind: 'library' | 'print' | 'screenshot' }
  | { kind: 'navigate'; input: string }
  | { kind: 'find'; text: string; backwards: boolean; sensitive: boolean }
  | { kind: 'zoom'; factor: number }
  | { kind: 'overlay'; visible: boolean; capture?: boolean }
  | { kind: 'settings'; settings: BrowserSettings }
  | { kind: 'clear'; history: boolean; cookies: boolean; passwords: boolean; downloads: boolean }
  | { kind: 'importCookies' | 'importPasswords'; content: string }
  | { kind: 'fillPassword' | 'deletePassword' | 'revealDownload'; id: number }

/** Share one queue with native geometry updates so a close cannot race an overlay. */
export function browserControl<T = unknown>(request: BrowserAction): Promise<T> {
  const next = browserQueue.then(() => invoke<T>('browser_control', { request }))
  browserQueue = next.then(() => {}, () => {})
  return next
}
