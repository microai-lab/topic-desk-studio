/** 类型化 Tauri command 客户端，集中隔离前端与原生层通信细节。 */
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
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

/** 使用系统默认浏览器打开已校验的 HTTP(S) 原文。 */
export async function openExternalUrl(url: string): Promise<void> {
  const parsed = new URL(url)
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new TypeError('只能打开 HTTP(S) 链接')
  }
  await openUrl(parsed.href)
}
