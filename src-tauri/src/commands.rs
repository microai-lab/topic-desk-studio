//! Narrow, validated Tauri command surface exposed to the sandboxed WebView.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Deserialize;
use tauri::{Manager, State};

use crate::collector;
use crate::credential_cipher::{CredentialCipher, MASTER_KEY_FILE};
use crate::error::AppError;
use crate::models::{
    ModelSettings, NetworkSettings, RefreshResult, SaveModelSettings, SaveNetworkSettings,
    SaveUiPreferences, TopicPage, TopicQuery, TranslationResult, UiPreferences,
};
use crate::repository::TopicRepository;
use crate::translator;

/// Shared native state; the connection lock is held only for short SQLite operations.
pub struct AppState {
    pub database: Mutex<Connection>,
    pub database_path: PathBuf,
    pub refreshing: Arc<AtomicBool>,
}

/// Minimal untrusted card payload extracted from the rendered Xiaohongshu DOM.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserXiaohongshuTopic {
    id: String,
    title: String,
    url: String,
}

/// Convert poisoned locks and repository errors into safe command errors.
fn with_repository<T>(
    state: &State<'_, AppState>,
    operation: impl FnOnce(&TopicRepository<'_>) -> Result<T, AppError>,
) -> Result<T, String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| AppError::PoisonedState.to_string())?;
    operation(&TopicRepository::new(&connection)).map_err(|error| error.to_string())
}

/// Query one page of locally persisted topics.
#[tauri::command]
pub fn list_topics(state: State<'_, AppState>, query: TopicQuery) -> Result<TopicPage, String> {
    with_repository(&state, |repository| repository.list(&query))
}

/// Persist one topic's creation-queue state idempotently.
#[tauri::command]
pub fn set_topic_queued(
    state: State<'_, AppState>,
    topic_id: i64,
    queued: bool,
) -> Result<(), String> {
    with_repository(&state, |repository| repository.set_queued(topic_id, queued))
}

/// Enable or disable one known source without changing its history.
#[tauri::command]
pub fn set_platform_enabled(
    state: State<'_, AppState>,
    code: String,
    enabled: bool,
) -> Result<(), String> {
    with_repository(&state, |repository| {
        repository.set_platform_enabled(&code, enabled)
    })
}

/// Return model routing and SQLite credential presence without exposing the stored secret.
#[tauri::command]
pub fn get_model_settings(state: State<'_, AppState>) -> Result<ModelSettings, String> {
    with_repository(&state, |repository| repository.model_settings())
}

/// Return interface preferences persisted outside the temporary main WebView.
#[tauri::command]
pub fn get_ui_preferences(state: State<'_, AppState>) -> Result<UiPreferences, String> {
    with_repository(&state, |repository| repository.ui_preferences())
}

/// Validate through typed enums and atomically persist language and appearance.
#[tauri::command]
pub fn save_ui_preferences(
    state: State<'_, AppState>,
    settings: SaveUiPreferences,
) -> Result<UiPreferences, String> {
    with_repository(&state, |repository| {
        repository.save_ui_preferences(settings.locale, settings.theme)?;
        repository.ui_preferences()
    })
}

/// Return the explicit native proxy; operating-system and environment settings remain implicit.
#[tauri::command]
pub fn get_network_settings(state: State<'_, AppState>) -> Result<NetworkSettings, String> {
    with_repository(&state, |repository| repository.network_settings())
}

/// Validate and persist an unauthenticated HTTP(S) proxy for future collection runs.
#[tauri::command]
pub fn save_network_settings(
    state: State<'_, AppState>,
    settings: SaveNetworkSettings,
) -> Result<NetworkSettings, String> {
    let proxy_url = validate_proxy_url(&settings.proxy_url)?;
    with_repository(&state, |repository| {
        repository.save_network_settings(proxy_url.as_deref())?;
        repository.network_settings()
    })
    .map_err(|error| error.to_string())
}

fn validate_proxy_url(input: &str) -> Result<Option<String>, String> {
    let value = input.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let url = url::Url::parse(value).map_err(|error| format!("代理地址无效：{error}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("代理地址必须是完整的 HTTP(S) URL".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("请使用不含用户名和密码的本地代理地址".into());
    }
    if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
        return Err("代理地址不能包含路径、查询参数或片段".into());
    }
    Ok(Some(url.to_string().trim_end_matches('/').to_owned()))
}

/// Validate and save model routing, placing a supplied key in the private SQLite table.
#[tauri::command]
pub fn save_model_settings(
    state: State<'_, AppState>,
    settings: SaveModelSettings,
) -> Result<ModelSettings, String> {
    let endpoint = settings.endpoint.trim();
    let model = settings.model.trim();
    let url = url::Url::parse(endpoint).map_err(|error| format!("模型接口地址无效：{error}"))?;
    if !matches!(url.scheme(), "http" | "https") || model.is_empty() {
        return Err("模型接口必须使用 HTTP(S)，且模型名称不能为空".into());
    }
    let encrypted_api_key = settings
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            CredentialCipher::load_or_create(&state.database_path.with_file_name(MASTER_KEY_FILE))?
                .encrypt(value)
        })
        .transpose()
        .map_err(|error: AppError| error.to_string())?;
    with_repository(&state, |repository| {
        repository.save_model_settings(endpoint, model, encrypted_api_key.as_ref())
    })?;
    get_model_settings(state)
}

/// Translate only the current database title associated with the requested topic ID.
#[tauri::command]
pub async fn translate_topic(
    state: State<'_, AppState>,
    topic_id: i64,
) -> Result<TranslationResult, String> {
    if topic_id <= 0 {
        return Err("topicId 必须是正整数".into());
    }
    let (title, settings, encrypted_api_key) = with_repository(&state, |repository| {
        let title = repository
            .topic_title(topic_id)?
            .ok_or_else(|| AppError::InvalidInput("话题不存在或已失效".into()))?;
        Ok((
            title,
            repository.model_settings()?,
            repository.encrypted_api_key()?,
        ))
    })?;
    let api_key = encrypted_api_key
        .map(|encrypted| {
            CredentialCipher::load_existing(&state.database_path.with_file_name(MASTER_KEY_FILE))?
                .ok_or_else(|| AppError::Credential("模型凭据主密钥不存在".into()))?
                .decrypt(&encrypted)
        })
        .transpose()
        .map_err(|error: AppError| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        translator::translate_title(topic_id, &title, &settings, api_key)
    })
    .await
    .map_err(|error| format!("翻译任务异常结束：{error}"))?
    .map_err(|error| error.to_string())
}

/// Run the native collector off the UI thread while enforcing one process-wide refresh.
#[tauri::command]
pub async fn refresh_topics(state: State<'_, AppState>) -> Result<RefreshResult, String> {
    if state
        .refreshing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Ok(RefreshResult {
            accepted: false,
            message: "已有一轮采集正在运行".into(),
            inserted: 0,
            updated: 0,
            inserted_topic_ids: Vec::new(),
        });
    }

    let database_path = state.database_path.clone();
    let refreshing = Arc::clone(&state.refreshing);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = collector::collect_all(&database_path, "manual");
        refreshing.store(false, Ordering::Release);
        result
    })
    .await
    .map_err(|error| format!("采集任务异常结束：{error}"))?
    .map_err(|error| error.to_string())?;

    Ok(RefreshResult {
        accepted: true,
        message: format!(
            "刷新完成：新增 {}，更新 {}",
            result.inserted, result.updated
        ),
        inserted: result.inserted,
        updated: result.updated,
        inserted_topic_ids: result.inserted_topic_ids,
    })
}

/// Collect public cards rendered in an authenticated ephemeral Xiaohongshu WebView.
#[tauri::command]
pub async fn collect_xiaohongshu_session(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<RefreshResult, String> {
    if webview.label() != "main" {
        return Err("无权采集浏览器页面".into());
    }
    let label = crate::desktop::tab_label(&tab_id)?;
    let article = app.get_webview(&label).ok_or("小红书页面未打开")?;
    let current = article.url().map_err(|error| error.to_string())?;
    let host = current.host_str().unwrap_or_default();
    if host != "xiaohongshu.com" && !host.ends_with(".xiaohongshu.com") {
        return Err("当前标签页不是小红书页面".into());
    }

    let script = r#"
(() => {
  const rows = [];
  const seen = new Set();
  for (const link of document.querySelectorAll('a[href*="/explore/"]')) {
    let url;
    try { url = new URL(link.href, location.href); } catch (_) { continue; }
    const match = url.pathname.match(/^\/explore\/([a-f0-9]{24})(?:\/|$)/i);
    if (!match || seen.has(match[1])) continue;
    const card = link.closest('section, article, .note-item, [class*="note-item"]') || link.parentElement;
    const titleNode = card?.querySelector('.title span, .title, [class*="title"] span, [class*="title"]');
    const title = (titleNode?.textContent || link.getAttribute('aria-label') || '').trim();
    if (!title) continue;
    seen.add(match[1]);
    rows.push({ id: match[1].toLowerCase(), title, url: `https://www.xiaohongshu.com/explore/${match[1]}` });
    if (rows.length >= 50) break;
  }
  return rows;
})()
"#;
    let (sender, receiver) = std::sync::mpsc::channel();
    article
        .eval_with_callback(script, move |value| {
            let _ = sender.send(value);
        })
        .map_err(|error| error.to_string())?;
    let payload = tauri::async_runtime::spawn_blocking(move || {
        receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| "读取小红书页面超时".to_string())
    })
    .await
    .map_err(|error| format!("页面读取任务异常结束：{error}"))??;
    let cards: Vec<BrowserXiaohongshuTopic> = serde_json::from_str(&payload)
        .map_err(|error| format!("小红书页面数据格式无效：{error}"))?;
    if cards.is_empty() {
        return Err("当前页面没有可采集的公开笔记；请先完成登录并等待推荐流显示".into());
    }

    let mut topics = Vec::new();
    for card in cards {
        let title = card.title.split_whitespace().collect::<Vec<_>>().join(" ");
        let url = url::Url::parse(&card.url).map_err(|_| "小红书笔记地址无效")?;
        if card.id.len() != 24
            || !card.id.bytes().all(|value| value.is_ascii_hexdigit())
            || url.host_str() != Some("www.xiaohongshu.com")
            || url.path() != format!("/explore/{}", card.id)
            || title.is_empty()
        {
            continue;
        }
        topics.push(crate::models::CollectedTopic {
            platform_code: "xiaohongshu".into(),
            stable_id: Some(card.id),
            title: title.chars().take(180).collect(),
            url: url.to_string(),
            published_time: None,
            rank: topics.len() as u32 + 1,
            heat: None,
        });
    }
    if topics.is_empty() {
        return Err("当前页面的公开笔记均未通过安全校验".into());
    }
    let count = topics.len() as u32;
    let feed = crate::models::ParsedFeed {
        topics,
        fetched_count: count,
        invalid_count: 0,
    };
    let stats = with_repository(&state, |repository| {
        let platform = repository
            .enabled_platforms()?
            .into_iter()
            .find(|platform| platform.code == "xiaohongshu")
            .ok_or_else(|| AppError::InvalidInput("小红书来源未启用".into()))?;
        let run_id = repository.create_run(platform.id, "browser-session")?;
        match repository.commit_feed(&platform, run_id, &feed) {
            Ok(stats) => Ok(stats),
            Err(error) => {
                let _ = repository.fail_run(run_id, &error.to_string());
                Err(error)
            }
        }
    })?;
    Ok(RefreshResult {
        accepted: true,
        message: format!("小红书当前页采集完成：{} 条", count),
        inserted: stats.inserted,
        updated: stats.updated,
        inserted_topic_ids: stats.inserted_topic_ids,
    })
}

/// Route trusted UI browser requests asynchronously to avoid Windows UI deadlocks.
#[tauri::command]
pub async fn browser_request(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    action: String,
    tab_id: Option<String>,
    url: Option<String>,
    bounds: Option<crate::desktop::BrowserBounds>,
) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("无权操作浏览区域".into());
    }
    crate::desktop::browser_request(&app, &action, tab_id, url, bounds)
}

#[cfg(test)]
mod tests {
    use super::validate_proxy_url;

    #[test]
    fn validates_local_http_proxy_without_credentials() {
        assert_eq!(validate_proxy_url("  ").unwrap(), None);
        assert_eq!(
            validate_proxy_url("http://127.0.0.1:7897/").unwrap(),
            Some("http://127.0.0.1:7897".into())
        );
        for value in [
            "127.0.0.1:7897",
            "socks5://127.0.0.1:7897",
            "http://user:secret@127.0.0.1:7897",
            "http://127.0.0.1:7897/proxy",
        ] {
            assert!(validate_proxy_url(value).is_err(), "must reject {value}");
        }
    }
}
