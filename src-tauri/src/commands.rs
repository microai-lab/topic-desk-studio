//! Narrow, validated Tauri command surface exposed to the sandboxed WebView.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::State;

use crate::collector;
use crate::error::AppError;
use crate::models::{
    ModelSettings, RefreshResult, SaveModelSettings, TopicPage, TopicQuery, TranslationResult,
};
use crate::repository::TopicRepository;
use crate::translator;

/// Shared native state; the connection lock is held only for short SQLite operations.
pub struct AppState {
    pub database: Mutex<Connection>,
    pub database_path: PathBuf,
    pub refreshing: Arc<AtomicBool>,
    pub api_key_cache: Arc<Mutex<translator::CredentialCache>>,
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

/// Return model routing and credential presence without exposing the stored secret.
#[tauri::command]
pub fn get_model_settings(state: State<'_, AppState>) -> Result<ModelSettings, String> {
    let mut settings = with_repository(&state, |repository| repository.model_settings())?;
    settings.has_api_key =
        translator::has_api_key(&state.api_key_cache).map_err(|error| error.to_string())?;
    Ok(settings)
}

/// Validate and save standalone model routing, placing a supplied key in the OS credential vault.
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
    with_repository(&state, |repository| {
        repository.save_model_settings(endpoint, model)
    })?;
    if let Some(api_key) = settings.api_key.as_deref() {
        if !api_key.trim().is_empty() {
            translator::save_api_key(&state.api_key_cache, api_key)
                .map_err(|error| error.to_string())?;
        }
    }
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
    let (title, settings) = with_repository(&state, |repository| {
        let title = repository
            .topic_title(topic_id)?
            .ok_or_else(|| AppError::InvalidInput("话题不存在或已失效".into()))?;
        Ok((title, repository.model_settings()?))
    })?;
    let api_key_cache = Arc::clone(&state.api_key_cache);
    tauri::async_runtime::spawn_blocking(move || {
        translator::translate_title(topic_id, &title, &settings, &api_key_cache)
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
