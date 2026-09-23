//! Narrow, validated Tauri command surface exposed to the sandboxed WebView.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Deserialize;
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::catalog::PLATFORM_CATALOG;
use crate::collector;
use crate::credential_cipher::{CredentialCipher, MASTER_KEY_FILE};
use crate::error::AppError;
use crate::models::{
    CollectionStatusEvent, ModelSettings, NetworkSettings, RefreshResult, SaveModelSettings,
    SaveNetworkSettings, SaveSourceConfiguration, SaveUiPreferences, SourceConfiguration,
    SourceConfigurationBundle, SourceParserType, StorageOperationResult, StorageStatus, TopicPage,
    TopicQuery, TranslationResult, UiPreferences,
};
use crate::repository::TopicRepository;
use crate::translator;

/// Shared native state; the connection lock is held only for short SQLite operations.
pub struct AppState {
    pub database: Mutex<Connection>,
    pub database_path: PathBuf,
    pub refreshing: Arc<AtomicBool>,
}

/// Notify the trusted desktop shell about native collection work, including scheduler runs.
pub fn emit_collection_status(
    app: &tauri::AppHandle,
    phase: &str,
    trigger: &str,
    message: String,
    inserted: u32,
    updated: u32,
) {
    let _ = app.emit_to(
        "main",
        "collection-status",
        CollectionStatusEvent {
            phase: phase.into(),
            trigger: trigger.into(),
            message,
            inserted,
            updated,
        },
    );
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
    let connection =
        crate::database::open_reader(&state.database_path).map_err(|error| error.to_string())?;
    TopicRepository::new(&connection)
        .list(&query)
        .map_err(|error| error.to_string())
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

/// Hide one disliked topic and blacklist its stable identity from later syncs.
#[tauri::command]
pub fn hide_topic(state: State<'_, AppState>, topic_id: i64) -> Result<(), String> {
    with_repository(&state, |repository| repository.hide_topic(topic_id))
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

/// Return all editable built-in and custom source definitions.
#[tauri::command]
pub fn list_source_configurations(
    state: State<'_, AppState>,
) -> Result<Vec<SourceConfiguration>, String> {
    with_repository(&state, |repository| repository.source_configurations())
}

/// Persist an exact user-defined order after the native layer validates the permutation.
#[tauri::command]
pub fn reorder_source_configurations(
    state: State<'_, AppState>,
    codes: Vec<String>,
) -> Result<Vec<SourceConfiguration>, String> {
    with_repository(&state, |repository| {
        repository.reorder_source_configurations(&codes)?;
        repository.source_configurations()
    })
}

/// Validate and persist a declarative source without allowing executable configuration.
#[tauri::command]
pub fn save_source_configuration(
    state: State<'_, AppState>,
    source: SaveSourceConfiguration,
) -> Result<Vec<SourceConfiguration>, String> {
    validate_source_configuration(&source)?;
    validate_source_parser_identity(&source)?;
    with_repository(&state, |repository| {
        let existing = repository
            .source_configurations()?
            .into_iter()
            .find(|item| item.code == source.code);
        if source.parser_type == SourceParserType::Builtin
            && !existing.is_some_and(|item| item.built_in)
            && !is_default_source(&source.code)
        {
            return Err(AppError::InvalidInput(
                "自定义来源不能使用内置解析器".into(),
            ));
        }
        repository.save_source_configuration(&source)?;
        repository.source_configurations()
    })
}

/// Delete any source while retaining its historical topics.
#[tauri::command]
pub fn delete_source(
    state: State<'_, AppState>,
    code: String,
) -> Result<Vec<SourceConfiguration>, String> {
    with_repository(&state, |repository| {
        repository.delete_source(code.trim())?;
        repository.source_configurations()
    })
}

/// Export one source or the complete active list as a versioned JSON document.
#[tauri::command]
pub fn export_source_configurations(
    state: State<'_, AppState>,
    path: String,
    code: Option<String>,
) -> Result<(), String> {
    let sources = with_repository(&state, |repository| repository.source_configurations())?;
    let selected = sources
        .into_iter()
        .filter(|source| code.as_ref().is_none_or(|code| source.code == *code))
        .map(source_for_export)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("没有可导出的数据源".into());
    }
    let document = SourceConfigurationBundle {
        format: "topic-desk-sources".into(),
        version: 1,
        sources: selected,
    };
    let json = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("无法生成数据源文件：{error}"))?;
    fs::write(PathBuf::from(path), json).map_err(|error| format!("无法写入数据源文件：{error}"))
}

/// Import a portable JSON document after validating every row, then commit it atomically.
#[tauri::command]
pub fn import_source_configurations(
    state: State<'_, AppState>,
    path: String,
    target_code: Option<String>,
) -> Result<Vec<SourceConfiguration>, String> {
    let path = PathBuf::from(path);
    let metadata = fs::metadata(&path).map_err(|error| format!("无法读取数据源文件：{error}"))?;
    if metadata.len() > 1024 * 1024 {
        return Err("数据源文件不能超过 1 MB".into());
    }
    let json = fs::read_to_string(path).map_err(|error| format!("无法读取数据源文件：{error}"))?;
    let bundle: SourceConfigurationBundle =
        serde_json::from_str(&json).map_err(|error| format!("数据源 JSON 格式无效：{error}"))?;
    if bundle.format != "topic-desk-sources" || bundle.version != 1 {
        return Err("不是受支持的 Topic Desk 数据源文件".into());
    }
    if bundle.sources.is_empty() || bundle.sources.len() > 500 {
        return Err("数据源文件必须包含 1–500 个来源".into());
    }
    if let Some(target_code) = target_code.as_deref() {
        if bundle.sources.len() != 1 || bundle.sources[0].code != target_code {
            return Err(format!("请选择仅包含来源“{target_code}”的单来源文件"));
        }
    }
    let mut codes = HashSet::new();
    for source in &bundle.sources {
        if !codes.insert(source.code.as_str()) {
            return Err(format!("数据源文件包含重复代码：{}", source.code));
        }
        validate_source_configuration(source)?;
        validate_source_parser_identity(source)?;
    }
    with_repository(&state, |repository| {
        repository.import_source_configurations(&bundle.sources)?;
        repository.source_configurations()
    })
}

/// Recreate and reset every catalog source while leaving custom sources untouched.
#[tauri::command]
pub fn restore_default_sources(
    state: State<'_, AppState>,
) -> Result<Vec<SourceConfiguration>, String> {
    with_repository(&state, |repository| {
        repository.restore_default_sources()?;
        repository.source_configurations()
    })
}

fn source_for_export(source: SourceConfiguration) -> SaveSourceConfiguration {
    SaveSourceConfiguration {
        code: source.code,
        display_name: source.display_name,
        home_url: source.home_url,
        endpoint_url: source.endpoint_url,
        region: source.region,
        category: source.category,
        parser_type: source.parser_type,
        proxy_mode: source.proxy_mode,
        enabled: source.enabled,
        parser_config: source.parser_config,
    }
}

fn is_default_source(code: &str) -> bool {
    PLATFORM_CATALOG.iter().any(|source| source.code == code)
}

fn validate_source_parser_identity(source: &SaveSourceConfiguration) -> Result<(), String> {
    match (is_default_source(&source.code), source.parser_type) {
        (true, SourceParserType::Builtin)
        | (false, SourceParserType::Rss | SourceParserType::Json | SourceParserType::Html) => {
            Ok(())
        }
        (true, _) => Err(format!("默认来源 {} 必须使用内置解析器", source.code)),
        (false, SourceParserType::Builtin) => Err(format!("{} 不能使用内置解析器", source.code)),
    }
}

fn validate_source_configuration(source: &SaveSourceConfiguration) -> Result<(), String> {
    let code = source.code.trim();
    if code.len() < 2
        || code.len() > 48
        || !code
            .bytes()
            .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
    {
        return Err("来源代码需为 2–48 位小写字母、数字或连字符".into());
    }
    if source.display_name.trim().is_empty() || source.display_name.chars().count() > 80 {
        return Err("来源名称不能为空且最多 80 个字符".into());
    }
    for (label, raw) in [
        ("首页", &source.home_url),
        ("采集地址", &source.endpoint_url),
    ] {
        if raw.len() > 4096 {
            return Err(format!("{label}不能超过 4096 个字符"));
        }
        let url = url::Url::parse(raw.trim()).map_err(|error| format!("{label}无效：{error}"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(format!("{label}必须是不含凭据的 HTTP(S) URL"));
        }
    }
    for value in [
        &source.parser_config.items_path,
        &source.parser_config.id_path,
        &source.parser_config.title_path,
        &source.parser_config.url_path,
        &source.parser_config.published_path,
        &source.parser_config.rank_path,
        &source.parser_config.heat_path,
        &source.parser_config.item_selector,
        &source.parser_config.title_selector,
        &source.parser_config.link_selector,
    ] {
        if value.as_ref().is_some_and(|value| value.len() > 256) {
            return Err("字段路径和 CSS 选择器不能超过 256 个字符".into());
        }
    }
    match source.parser_type {
        SourceParserType::Json => {
            required_source_field(&source.parser_config.title_path, "JSON 标题字段")?;
            required_source_field(&source.parser_config.url_path, "JSON 链接字段")?;
        }
        SourceParserType::Html => {
            for (value, label) in [
                (&source.parser_config.item_selector, "HTML 条目选择器"),
                (&source.parser_config.title_selector, "HTML 标题选择器"),
            ] {
                let selector = required_source_field(value, label)?;
                scraper::Selector::parse(selector)
                    .map_err(|_| format!("{label}不是有效的 CSS 选择器"))?;
            }
            if let Some(selector) = source
                .parser_config
                .link_selector
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                scraper::Selector::parse(selector)
                    .map_err(|_| "HTML 链接选择器不是有效的 CSS 选择器".to_string())?;
            }
        }
        SourceParserType::Builtin | SourceParserType::Rss => {}
    }
    Ok(())
}

fn required_source_field<'a>(value: &'a Option<String>, label: &str) -> Result<&'a str, String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label}不能为空"))
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

/// Return aggregate storage health without exposing topic, browser or credential contents.
#[tauri::command]
pub fn get_storage_status(
    state: State<'_, AppState>,
    browser: State<'_, crate::browser_profile::BrowserProfile>,
) -> Result<StorageStatus, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| AppError::PoisonedState.to_string())?;
    let browser = browser.inner.lock().map_err(|error| error.to_string())?;
    let data_directory = state.database_path.parent().ok_or("应用数据目录无效")?;
    crate::storage::status(data_directory, &database, &browser.database)
        .map_err(|error| error.to_string())
}

/// Create a consistent application-managed snapshot of both local databases.
#[tauri::command]
pub fn backup_storage(
    state: State<'_, AppState>,
    browser: State<'_, crate::browser_profile::BrowserProfile>,
) -> Result<StorageOperationResult, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| AppError::PoisonedState.to_string())?;
    let browser = browser.inner.lock().map_err(|error| error.to_string())?;
    crate::storage::backup(
        state.database_path.parent().ok_or("应用数据目录无效")?,
        &database,
        &browser.database,
    )
    .map_err(|error| error.to_string())
}

/// Restore only application-created snapshots while collection is paused.
#[tauri::command]
pub fn restore_latest_backup(
    state: State<'_, AppState>,
    browser: State<'_, crate::browser_profile::BrowserProfile>,
) -> Result<StorageOperationResult, String> {
    if state
        .refreshing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("采集进行中，暂时不能恢复备份".into());
    }
    let result = (|| {
        let mut database = state
            .database
            .lock()
            .map_err(|_| AppError::PoisonedState.to_string())?;
        let mut browser = browser.inner.lock().map_err(|error| error.to_string())?;
        let operation = crate::storage::restore_latest(
            state.database_path.parent().ok_or("应用数据目录无效")?,
            &mut database,
            &mut browser.database,
        )
        .map_err(|error| error.to_string())?;
        browser.settings = browser
            .database
            .query_row("SELECT json FROM preferences WHERE id=1", [], |row| {
                row.get::<_, String>(0)
            })
            .ok()
            .and_then(|value| serde_json::from_str(&value).ok())
            .unwrap_or_default();
        browser.tabs.clear();
        browser.active_tab = None;
        Ok(operation)
    })();
    state.refreshing.store(false, Ordering::Release);
    result
}

/// Run retention, index optimization and passive WAL checkpoints on demand.
#[tauri::command]
pub fn optimize_storage(
    state: State<'_, AppState>,
    browser: State<'_, crate::browser_profile::BrowserProfile>,
) -> Result<StorageOperationResult, String> {
    with_repository(&state, |repository| repository.maintain_storage())?;
    let browser = browser.inner.lock().map_err(|error| error.to_string())?;
    crate::browser_profile::prune_records(&browser.database).map_err(|error| error.to_string())?;
    browser
        .database
        .execute_batch("PRAGMA optimize; PRAGMA wal_checkpoint(PASSIVE);")
        .map_err(|error| error.to_string())?;
    let database = state
        .database
        .lock()
        .map_err(|_| AppError::PoisonedState.to_string())?;
    database
        .execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
        .map_err(|error| error.to_string())?;
    Ok(StorageOperationResult {
        message: "本地数据已整理".into(),
        backup_name: None,
    })
}

/// Reveal the private application data directory in the operating-system file manager.
#[tauri::command]
pub fn open_data_directory(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(&state.database_path)
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
pub async fn refresh_topics(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RefreshResult, String> {
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

    emit_collection_status(&app, "started", "manual", "正在采集数据…".into(), 0, 0);
    let database_path = state.database_path.clone();
    let refreshing = Arc::clone(&state.refreshing);
    let event_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = collector::collect_all(&database_path, "manual");
        refreshing.store(false, Ordering::Release);
        match &result {
            Ok(stats) => emit_collection_status(
                &event_app,
                "finished",
                "manual",
                format!("采集完成：新增 {}，更新 {}", stats.inserted, stats.updated),
                stats.inserted,
                stats.updated,
            ),
            Err(error) => {
                emit_collection_status(&event_app, "failed", "manual", error.to_string(), 0, 0)
            }
        }
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
            Ok(stats) => {
                repository.record_recent_additions("browser-session", &stats.inserted_topic_ids)?;
                Ok(stats)
            }
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
