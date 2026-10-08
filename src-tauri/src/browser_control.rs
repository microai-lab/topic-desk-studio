//! Typed browser chrome actions; every native/profile operation requires the main webview.

use crate::browser_profile::{self, BrowserProfile, BrowserSettings};
use crate::commands::AppState;
use crate::credential_cipher::{CredentialCipher, MASTER_KEY_FILE};
use crate::error::AppError;
use crate::repository::TopicRepository;
use crate::translator;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{webview::Cookie, Emitter, EventTarget, Manager};
use tauri_plugin_opener::OpenerExt;

/// Explicit browser operations prevent remote pages from supplying arbitrary scripts or paths.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BrowserAction {
    Library,
    Navigate {
        input: String,
    },
    Find {
        text: String,
        backwards: bool,
        sensitive: bool,
    },
    Zoom {
        factor: f64,
    },
    Overlay {
        visible: bool,
        #[serde(default)]
        capture: bool,
    },
    Print,
    Screenshot,
    Settings {
        settings: BrowserSettings,
    },
    Clear {
        history: bool,
        cookies: bool,
        downloads: bool,
    },
    ImportCookies {
        content: String,
    },
    RevealDownload {
        id: i64,
    },
    OpenDownload {
        id: i64,
    },
    CancelDownload {
        id: i64,
    },
    TranslatePage {
        #[serde(rename = "targetLanguage")]
        target_language: String,
    },
    RestorePageTranslation,
}

/// Reject unsupported persisted values before they can affect browser behavior.
fn valid_browser_settings(settings: &BrowserSettings) -> bool {
    ["bing", "google", "duckduckgo"].contains(&settings.search_engine.as_str())
        && ["auto", "zh-CN", "en", "ja", "ko", "fr", "de", "es", "ru"]
            .contains(&settings.translation_language.as_str())
        && settings.zoom.is_finite()
        && (0.25..=3.0).contains(&settings.zoom)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageTranslationSnapshot {
    texts: Vec<String>,
    #[serde(rename = "detectedLanguage")]
    detected_language: String,
    #[serde(default)]
    failed: usize,
}

/// Small progress payload emitted only when counts change in the active session.
#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageTranslationProgress {
    tab_id: String,
    token: String,
    queued: usize,
    deferred: usize,
    active: usize,
    completed: usize,
    failed: usize,
    running: bool,
}

#[derive(Deserialize)]
struct PageTranslationPriority {
    valid: Vec<usize>,
    ready: Vec<usize>,
}

/// Model workers send provisional text separately from final validated answers.
enum PageTranslationMessage {
    Partial {
        owner: usize,
        text: String,
    },
    Finished {
        batch: Vec<(usize, String)>,
        result: Result<Vec<String>, String>,
    },
}

/// Wake the native scheduler as soon as model output arrives, while retaining
/// a bounded timeout for discovering newly visible or changed page content.
struct PageTranslationInbox {
    receiver: std::sync::mpsc::Receiver<PageTranslationMessage>,
    next: Option<PageTranslationMessage>,
}

impl PageTranslationInbox {
    fn new(receiver: std::sync::mpsc::Receiver<PageTranslationMessage>) -> Self {
        Self {
            receiver,
            next: None,
        }
    }

    fn take(&mut self) -> Option<PageTranslationMessage> {
        self.next.take().or_else(|| self.receiver.try_recv().ok())
    }

    fn wait(&mut self, timeout: std::time::Duration) {
        self.next = self.receiver.recv_timeout(timeout).ok();
    }
}

/// Several tokens may arrive during one native/page round trip. Render only
/// the newest cumulative text for each slot, including duplicate-text waiters.
fn queue_translation_partial(
    partials: &mut std::collections::BTreeMap<usize, String>,
    owner: usize,
    text: String,
    waiters: &std::collections::HashMap<usize, Vec<usize>>,
) {
    if let Some(ids) = waiters.get(&owner) {
        for &id in ids {
            partials.insert(id, text.clone());
        }
    }
    partials.insert(owner, text);
}

/// Empty sessions back off; an active request or visible queued work keeps
/// scroll response at 100 ms without polling a quiet page ten times a second.
fn translation_poll_delay(idle_rounds: u32, busy: bool) -> std::time::Duration {
    let milliseconds = if busy {
        100
    } else {
        (100 + idle_rounds.min(5) * 50).min(350)
    };
    std::time::Duration::from_millis(u64::from(milliseconds))
}

/// Wait for meaningful body text before fixing automatic direction. A short
/// navigation label in the first snapshot must not decide the whole session.
fn automatic_page_target(
    texts: &[String],
    detected_language: &str,
    elapsed: std::time::Duration,
) -> Option<String> {
    let script_chars = texts
        .iter()
        .flat_map(|text| text.chars())
        .filter(|ch| ch.is_ascii_alphabetic() || matches!(ch, '\u{3400}'..='\u{9fff}'))
        .count();
    let substantial_paragraph = texts.iter().any(|text| {
        text.chars()
            .filter(|ch| ch.is_ascii_alphabetic() || matches!(ch, '\u{3400}'..='\u{9fff}'))
            .take(80)
            .count()
            == 80
    });
    if script_chars == 0
        || (!substantial_paragraph
            && script_chars < 160
            && elapsed < std::time::Duration::from_secs(2))
    {
        return None;
    }
    translator::resolve_page_target_language(texts, "auto", detected_language)
        .ok()
        .map(str::to_owned)
}

/// Repeated visible text joins an in-flight request instead of opening a
/// second model connection. The owner applies its result to every waiting slot.
fn claim_translation_text(
    id: usize,
    text: &str,
    active_texts: &mut std::collections::HashMap<String, usize>,
    waiters: &mut std::collections::HashMap<usize, Vec<usize>>,
) -> bool {
    if let Some(owner) = active_texts.get(text) {
        waiters.entry(*owner).or_default().push(id);
        false
    } else {
        active_texts.insert(text.to_owned(), id);
        true
    }
}

/// Form one model request from visible paragraphs while joining duplicate
/// strings to an existing in-flight owner. Unclaimed IDs remain queued.
fn take_visible_translation_batch(
    ready: &[usize],
    pending: &mut std::collections::BTreeMap<usize, String>,
    active_texts: &mut std::collections::HashMap<String, usize>,
    waiters: &mut std::collections::HashMap<usize, Vec<usize>>,
    max_items: usize,
    can_start: bool,
) -> Vec<(usize, String)> {
    let mut batch = Vec::new();
    let mut characters = 0;
    for &id in ready {
        let Some(text) = pending.get(&id) else {
            continue;
        };
        if active_texts.contains_key(text) {
            let text = pending.remove(&id).expect("checked pending entry");
            claim_translation_text(id, &text, active_texts, waiters);
            continue;
        }
        let length = text.chars().count();
        if !can_start
            || batch.len() >= max_items
            || (!batch.is_empty() && characters + length > translator::PAGE_BATCH_CHARACTER_LIMIT)
        {
            continue;
        }
        let text = pending.remove(&id).expect("checked pending entry");
        characters += length;
        claim_translation_text(id, &text, active_texts, waiters);
        batch.push((id, text));
    }
    batch
}

/// Cache hits bypass the model pool; misses and offscreen work remain pending.
fn take_cached_translation_entries(
    ready: &[usize],
    pending: &mut std::collections::BTreeMap<usize, String>,
    cached: &[Option<String>],
) -> Vec<(usize, String)> {
    ready
        .iter()
        .zip(cached)
        .filter_map(|(&id, value)| {
            let value = value.as_ref()?;
            pending.remove(&id)?;
            Some((id, value.clone()))
        })
        .collect()
}

/// Resolve only paths previously assigned by the native download handler.
fn download_path(profile: &BrowserProfile, id: i64) -> Result<String, String> {
    let detail: String = profile
        .inner
        .lock()
        .map_err(|e| e.to_string())?
        .database
        .query_row(
            "SELECT detail FROM records WHERE kind='download' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| "下载记录不存在")?;
    let (state, path) = detail.split_once('\n').ok_or("下载路径无效")?;
    if state != "complete" || path.is_empty() {
        return Err("下载尚未完成".into());
    }
    Ok(path.into())
}

fn eval(view: &tauri::Webview, script: String) -> Result<Value, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    view.eval_with_callback(script, move |value| {
        let _ = tx.send(value);
    })
    .map_err(|e| e.to_string())?;
    let raw = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| "页面暂时未响应，请重试")?;
    serde_json::from_str(&raw).map_err(|_| "页面返回无效结果".into())
}

/// Apply results only to the still-current reader session.
fn apply_page_translation_entries(
    view: &tauri::Webview,
    token: &str,
    target_language: &str,
    entries: &[(usize, String)],
    error: Option<&str>,
) -> Result<u64, String> {
    let value = eval(view, format!(
        "(() => {{ const s=globalThis.__topicDeskPageTranslation; return s?.token === {} ? s.applyEntries({}, {}, {}) : 0; }})()",
        json!(token), json!(entries), json!(target_language), json!(error)
    ))?;
    Ok(value.as_u64().unwrap_or(0))
}

/// A single reader evaluation updates all in-flight visible slots together.
fn apply_page_translation_partials(
    view: &tauri::Webview,
    token: &str,
    target_language: &str,
    entries: &[(usize, String)],
) -> Result<(), String> {
    eval(view, format!(
        "(() => {{ const s=globalThis.__topicDeskPageTranslation; return s?.token === {} ? s.applyPartial({}, {}) : null; }})()",
        json!(token), json!(entries), json!(target_language)
    ))?;
    Ok(())
}

/// Execute only from the privileged main view and outside the native event-loop thread.
#[tauri::command]
pub async fn browser_control(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
    request: BrowserAction,
) -> Result<Value, String> {
    if webview.label() != "main" {
        return Err("无权访问浏览器数据".into());
    }
    let profile = app.state::<BrowserProfile>();
    let view = || {
        let tab_id = profile
            .inner
            .lock()
            .map_err(|error| error.to_string())?
            .active_tab
            .clone()
            .ok_or_else(|| "请先打开网页".to_string())?;
        app.get_webview(&crate::desktop::tab_label(&tab_id)?)
            .ok_or_else(|| "请先打开网页".to_string())
    };
    match request {
        BrowserAction::Library => {
            serde_json::to_value(browser_profile::library(&app)?).map_err(|e| e.to_string())
        }
        BrowserAction::Navigate { input } => {
            let engine = profile
                .inner
                .lock()
                .map_err(|e| e.to_string())?
                .settings
                .search_engine
                .clone();
            let url = browser_profile::resolve_address(&input, &engine)?;
            view()?.navigate(url.clone()).map_err(|e| e.to_string())?;
            Ok(json!({"url":url.as_str()}))
        }
        BrowserAction::Find {
            text,
            backwards,
            sensitive,
        } => {
            if text.len() > 1000 {
                return Err("查找词过长".into());
            }
            eval(
                &view()?,
                format!(
                    "window.find({}, {}, {}, true, false, false, false)",
                    json!(text),
                    sensitive,
                    backwards
                ),
            )
        }
        BrowserAction::Zoom { factor } => {
            if !factor.is_finite() || !(0.25..=3.0).contains(&factor) {
                return Err("缩放范围为 25%–300%".into());
            }
            view()?.set_zoom(factor).map_err(|e| e.to_string())?;
            let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
            session.settings.zoom = factor;
            session
                .database
                .execute(
                    "INSERT OR REPLACE INTO preferences VALUES(1,?1)",
                    [json!(session.settings).to_string()],
                )
                .map_err(|e| e.to_string())?;
            Ok(json!(factor))
        }
        BrowserAction::Overlay { visible, capture } => {
            let mut preview = None;
            if let Ok(view) = view() {
                if visible {
                    if capture {
                        use base64::Engine;
                        preview = crate::desktop::capture_screenshot(&view).ok().map(|bytes| {
                            format!(
                                "data:image/png;base64,{}",
                                base64::engine::general_purpose::STANDARD.encode(bytes)
                            )
                        });
                    }
                    view.hide().map_err(|e| e.to_string())?;
                    webview.set_focus().map_err(|e| e.to_string())?;
                } else {
                    view.show().map_err(|e| e.to_string())?;
                }
            }
            Ok(json!({"preview":preview}))
        }
        BrowserAction::Print => {
            view()?.print().map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        BrowserAction::Screenshot => {
            let path = app
                .path()
                .download_dir()
                .map_err(|e| e.to_string())?
                .join(format!("Topic-Desk-{}.png", browser_profile::now()));
            crate::desktop::save_screenshot(&view()?, &path)?;
            let url = view()?.url().map_err(|e| e.to_string())?;
            browser_profile::record_download(&app, url.as_str(), &path, "complete");
            Ok(json!({"path":path.to_string_lossy()}))
        }
        BrowserAction::Settings { settings } => {
            if !valid_browser_settings(&settings) {
                return Err("浏览器设置无效".into());
            }
            if let Ok(view) = view() {
                view.set_zoom(settings.zoom).map_err(|e| e.to_string())?;
            }
            let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
            session
                .database
                .execute(
                    "INSERT OR REPLACE INTO preferences VALUES(1,?1)",
                    [json!(settings).to_string()],
                )
                .map_err(|e| e.to_string())?;
            session.settings = settings;
            Ok(Value::Null)
        }
        BrowserAction::Clear {
            history,
            cookies,
            downloads,
        } => {
            if cookies {
                view()?
                    .clear_all_browsing_data()
                    .map_err(|e| e.to_string())?;
            }
            let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
            // Clear selected metadata atomically; downloads themselves are retained.
            let tx = session.database.transaction().map_err(|e| e.to_string())?;
            for (selected, kind) in [(history, "history"), (downloads, "download")] {
                if selected {
                    tx.execute("DELETE FROM records WHERE kind=?1", [kind])
                        .map_err(|e| e.to_string())?;
                }
            }
            tx.commit().map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        BrowserAction::ImportCookies { content } => {
            if content.len() > 2_000_000 {
                return Err("导入内容不能超过 2 MB".into());
            }
            let values: Vec<Value> =
                serde_json::from_str(&content).map_err(|_| "请粘贴 Cookie JSON 数组")?;
            if values.len() > 2000 {
                return Err("一次最多导入 2000 个 Cookie".into());
            }
            let mut cookies = vec![];
            for value in &values {
                let name = value["name"].as_str().ok_or("Cookie 缺少 name")?;
                let content = value["value"].as_str().ok_or("Cookie 缺少 value")?;
                let domain = value["domain"].as_str().ok_or("Cookie 缺少 domain")?;
                if name.is_empty()
                    || name.contains(['\r', '\n', ';', '='])
                    || content.contains(['\r', '\n'])
                    || domain.contains(['/', ':', ' ', '\r', '\n'])
                    || domain.trim_start_matches('.').is_empty()
                {
                    return Err("Cookie 字段无效".into());
                }
                let mut cookie = Cookie::build((name.to_owned(), content.to_owned()))
                    .domain(domain.to_owned())
                    .path(value["path"].as_str().unwrap_or("/").to_owned())
                    .secure(value["secure"].as_bool().unwrap_or(false))
                    .http_only(value["httpOnly"].as_bool().unwrap_or(false))
                    .build();
                if let Some(expiry) = value["expirationDate"].as_f64() {
                    if !expiry.is_finite() {
                        return Err("Cookie 过期时间无效".into());
                    }
                    cookie.set_expires(
                        time::OffsetDateTime::from_unix_timestamp(expiry as i64)
                            .map_err(|_| "Cookie 过期时间无效")?,
                    );
                }
                cookies.push(cookie);
            }
            let view = view()?;
            for cookie in cookies {
                view.set_cookie(cookie).map_err(|e| e.to_string())?;
            }
            Ok(json!({"count":values.len()}))
        }
        BrowserAction::RevealDownload { id } => {
            let path = download_path(&profile, id)?;
            app.opener()
                .reveal_item_in_dir(&path)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        BrowserAction::OpenDownload { id } => {
            let path = download_path(&profile, id)?;
            app.opener()
                .open_path(path, None::<&str>)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        BrowserAction::CancelDownload { id } => {
            crate::desktop::cancel_download(&app, id)?;
            Ok(Value::Null)
        }
        BrowserAction::TranslatePage { target_language } => {
            let page = view()?;
            let tab_id = profile
                .inner
                .lock()
                .map_err(|error| error.to_string())?
                .active_tab
                .clone()
                .ok_or_else(|| "请先打开网页".to_string())?;
            let token = format!("{:?}-{}", std::time::SystemTime::now(), page.label());
            let (settings, encrypted_api_key, network_settings) = {
                let database = state.database.lock().map_err(|error| error.to_string())?;
                let repository = TopicRepository::new(&database);
                (
                    repository
                        .model_settings()
                        .map_err(|error| error.to_string())?,
                    repository
                        .encrypted_api_key()
                        .map_err(|error| error.to_string())?,
                    repository
                        .network_settings()
                        .map_err(|error| error.to_string())?,
                )
            };
            let api_key = encrypted_api_key
                .clone()
                .map(|encrypted| {
                    CredentialCipher::load_existing(
                        &state.database_path.with_file_name(MASTER_KEY_FILE),
                    )?
                    .ok_or_else(|| AppError::Credential("模型凭据主密钥不存在".into()))?
                    .decrypt(&encrypted)
                })
                .transpose()
                .map_err(|error: AppError| error.to_string())?;
            translator::validate_page_translation_start(
                &settings,
                api_key.as_ref().is_some_and(|key| !key.trim().is_empty()),
                &target_language,
                network_settings.proxy_url.as_deref(),
            )
            .map_err(|error| error.to_string())?;
            drop(api_key);
            let initial_target = (target_language != "auto").then(|| target_language.clone());
            let snapshot: PageTranslationSnapshot = serde_json::from_value(eval(
                &page,
                format!("{}({})", include_str!("page_translation.js"), json!(token)),
            )?)
            .map_err(|_| "无法读取当前页面文本".to_string())?;
            let key_path = state.database_path.with_file_name(MASTER_KEY_FILE);
            let session_token = token.clone();
            let initial_count = snapshot.texts.len();
            let (max_items, max_parallel) = translator::page_request_plan(&settings.model);
            // Polling and model work run off the async/native event loops. The
            // isolated page owns only text and progress, never an IPC capability.
            tauri::async_runtime::spawn_blocking(move || {
                let (sender, receiver) = std::sync::mpsc::channel::<PageTranslationMessage>();
                let mut inbox = PageTranslationInbox::new(receiver);
                let mut pending = std::collections::BTreeMap::new();
                let mut active =
                    std::collections::HashMap::<usize, tauri::async_runtime::JoinHandle<()>>::new();
                let mut active_texts = std::collections::HashMap::new();
                let mut waiters = std::collections::HashMap::<usize, Vec<usize>>::new();
                let mut next_index = 0;
                let mut snapshot = snapshot;
                let mut resolved_target = initial_target;
                let started = std::time::Instant::now();
                let mut completed = 0;
                let mut idle_rounds: u32 = 0;
                let mut last_progress: Option<PageTranslationProgress> = None;
                loop {
                    // Native collection continues while HTTP tasks are pending.
                    for text in snapshot.texts.drain(..) {
                        pending.insert(next_index, text);
                        next_index += 1;
                    }
                    if resolved_target.is_none() {
                        let samples = pending.values().cloned().collect::<Vec<_>>();
                        resolved_target = automatic_page_target(
                            &samples,
                            &snapshot.detected_language,
                            started.elapsed(),
                        );
                    }
                    let mut partials = std::collections::BTreeMap::new();
                    while let Some(message) = inbox.take() {
                        let (batch, result) = match message {
                            PageTranslationMessage::Partial { owner, text } => {
                                queue_translation_partial(&mut partials, owner, text, &waiters);
                                continue;
                            }
                            PageTranslationMessage::Finished { batch, result } => (batch, result),
                        };
                        if let Some((first_id, _)) = batch.first() {
                            active.remove(first_id);
                        }
                        let mut entries = Vec::new();
                        for (offset, (id, text)) in batch.into_iter().enumerate() {
                            active_texts.remove(&text);
                            let ids =
                                std::iter::once(id).chain(waiters.remove(&id).unwrap_or_default());
                            for slot_id in ids {
                                // A final answer/error supersedes any tokens
                                // accumulated earlier in this dispatch.
                                partials.remove(&slot_id);
                                let value = result
                                    .as_ref()
                                    .ok()
                                    .and_then(|values| values.get(offset))
                                    .cloned()
                                    .unwrap_or_default();
                                entries.push((slot_id, value));
                            }
                        }
                        let error = result.as_ref().err().map(String::as_str);
                        if let Ok(count) = apply_page_translation_entries(
                            &page,
                            &token,
                            resolved_target.as_deref().unwrap_or_default(),
                            &entries,
                            error,
                        ) {
                            if error.is_none() {
                                completed += count as usize;
                            }
                        }
                    }
                    if !partials.is_empty() {
                        let partials = partials.into_iter().collect::<Vec<_>>();
                        let _ = apply_page_translation_partials(
                            &page,
                            &token,
                            resolved_target.as_deref().unwrap_or_default(),
                            &partials,
                        );
                    }
                    let priority = if pending.is_empty() {
                        Ok(PageTranslationPriority {
                            valid: vec![],
                            ready: vec![],
                        })
                    } else {
                        eval(&page, format!(
                            "(() => {{const s=globalThis.__topicDeskPageTranslation; return s?.token === {} ? s.prioritize({}) : null;}})()",
                            json!(token), json!(pending.keys().collect::<Vec<_>>())
                        )).and_then(|v| serde_json::from_value::<PageTranslationPriority>(v).map_err(|e| e.to_string()))
                    };
                    let Ok(priority) = priority else { break };
                    let valid = priority
                        .valid
                        .into_iter()
                        .collect::<std::collections::HashSet<_>>();
                    pending.retain(|id, _| valid.contains(id));
                    let visible_work = !priority.ready.is_empty();
                    let ready_ids = priority.ready;
                    if let Some(target) = resolved_target.as_ref() {
                        let cache_ids = ready_ids
                            .iter()
                            .copied()
                            .filter(|id| pending.contains_key(id))
                            .collect::<Vec<_>>();
                        let texts = cache_ids
                            .iter()
                            .map(|id| pending[id].clone())
                            .collect::<Vec<_>>();
                        if let Ok(cached) =
                            translator::cached_page_translations(&texts, target, &settings)
                        {
                            let entries =
                                take_cached_translation_entries(&cache_ids, &mut pending, &cached);
                            if !entries.is_empty() {
                                if let Ok(count) = apply_page_translation_entries(
                                    &page, &token, target, &entries, None,
                                ) {
                                    completed += count as usize;
                                }
                            }
                        }
                        while active.len() < max_parallel {
                            let batch = take_visible_translation_batch(
                                &ready_ids,
                                &mut pending,
                                &mut active_texts,
                                &mut waiters,
                                max_items,
                                true,
                            );
                            let Some((first_id, _)) = batch.first() else {
                                break;
                            };
                            let owner = *first_id;
                            let sender = sender.clone();
                            let target = target.clone();
                            let settings = settings.clone();
                            let encrypted = encrypted_api_key.clone();
                            let key_path = key_path.clone();
                            let proxy = network_settings.proxy_url.clone();
                            active.insert(
                                owner,
                                tauri::async_runtime::spawn(async move {
                                    let key = encrypted
                                        .map(|encrypted| {
                                            CredentialCipher::load_existing(&key_path)?
                                                .ok_or_else(|| {
                                                    AppError::Credential(
                                                        "模型凭据主密钥不存在".into(),
                                                    )
                                                })?
                                                .decrypt(&encrypted)
                                        })
                                        .transpose();
                                    let texts = batch
                                        .iter()
                                        .map(|(_, text)| text.clone())
                                        .collect::<Vec<_>>();
                                    let result = match key {
                                        Ok(key) => {
                                            let partial_sender = sender.clone();
                                            let mut progress = |text: &str| {
                                                let _ = partial_sender.send(
                                                    PageTranslationMessage::Partial {
                                                        owner,
                                                        text: text.to_owned(),
                                                    },
                                                );
                                            };
                                            translator::translate_page_batch_texts_with_progress(
                                                &texts,
                                                &target,
                                                &settings,
                                                key,
                                                proxy.as_deref(),
                                                Some(&mut progress),
                                            )
                                            .await
                                            .map_err(|e| e.to_string())
                                        }
                                        Err(error) => Err(error.to_string()),
                                    };
                                    let _ = sender
                                        .send(PageTranslationMessage::Finished { batch, result });
                                }),
                            );
                        }
                        if active.len() >= max_parallel {
                            take_visible_translation_batch(
                                &ready_ids,
                                &mut pending,
                                &mut active_texts,
                                &mut waiters,
                                max_items,
                                false,
                            );
                        }
                    }
                    let busy = visible_work || !active.is_empty() || !snapshot.texts.is_empty();
                    idle_rounds = if busy {
                        0
                    } else {
                        idle_rounds.saturating_add(1)
                    };
                    inbox.wait(translation_poll_delay(idle_rounds, busy));
                    let next = eval(&page, format!(
                        "(() => {{const s=globalThis.__topicDeskPageTranslation; return s?.token === {} ? s.collect() : null;}})()", json!(token)
                    )).and_then(|v| serde_json::from_value::<PageTranslationSnapshot>(v).map_err(|e| e.to_string()));
                    match next {
                        Ok(next) => snapshot = next,
                        Err(_) => break,
                    }
                    let progress = PageTranslationProgress {
                        tab_id: tab_id.clone(),
                        token: token.clone(),
                        queued: ready_ids
                            .iter()
                            .filter(|id| pending.contains_key(id))
                            .count(),
                        deferred: pending.len()
                            - ready_ids
                                .iter()
                                .filter(|id| pending.contains_key(id))
                                .count(),
                        active: active_texts.len() + waiters.values().map(Vec::len).sum::<usize>(),
                        completed,
                        failed: snapshot.failed,
                        running: true,
                    };
                    if last_progress.as_ref() != Some(&progress) {
                        let _ = app.emit_to(
                            EventTarget::webview("main"),
                            "browser-translation-progress",
                            &progress,
                        );
                        last_progress = Some(progress);
                    }
                }
                // Dropping a future stops local HTTP handling and all later
                // fallback attempts; a provider may already have started work.
                for (_, handle) in active {
                    handle.abort();
                }
                let _ = app.emit_to(
                    EventTarget::webview("main"),
                    "browser-translation-progress",
                    PageTranslationProgress {
                        tab_id,
                        token,
                        queued: 0,
                        deferred: 0,
                        active: 0,
                        completed,
                        failed: 0,
                        running: false,
                    },
                );
            });
            Ok(json!({"started": true, "token":session_token, "queued":initial_count}))
        }
        BrowserAction::RestorePageTranslation => {
            let restored = eval(
                &view()?,
                r#"(() => {
                  globalThis.__topicDeskPageTranslation?.stop?.();
                  return true;
                })()"#
                    .into(),
            )?;
            Ok(json!({"restored":restored == Value::Bool(true)}))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_messages_wake_the_scheduler_without_waiting_for_the_poll_deadline() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut inbox = PageTranslationInbox::new(receiver);
        let (ready, observed) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            inbox.wait(std::time::Duration::from_secs(5));
            let message = inbox.take();
            ready.send(message).unwrap();
            inbox
        });
        // This also covers output arriving before wait starts. A fixed sleep
        // would miss the one-second deadline despite an available model answer.
        sender
            .send(PageTranslationMessage::Partial {
                owner: 7,
                text: "首段译文".into(),
            })
            .unwrap();
        match observed.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(Some(PageTranslationMessage::Partial { owner, text })) => {
                assert_eq!(owner, 7);
                assert_eq!(text, "首段译文");
            }
            _ => panic!("available model output must wake the scheduler"),
        }
        let mut inbox = worker.join().unwrap();
        assert!(inbox.take().is_none());
        // A quiet viewport and a closed worker channel must remain safe.
        inbox.wait(std::time::Duration::ZERO);
        assert!(inbox.take().is_none());
        drop(sender);
        inbox.wait(std::time::Duration::from_secs(5));
        assert!(inbox.take().is_none());
    }

    #[test]
    fn streamed_tokens_coalesce_per_slot_without_mixing_independent_paragraphs() {
        let mut partials = std::collections::BTreeMap::new();
        let waiters = std::collections::HashMap::from([(1, vec![4, 8])]);
        queue_translation_partial(&mut partials, 1, "译".into(), &waiters);
        queue_translation_partial(&mut partials, 2, "另一段".into(), &waiters);
        queue_translation_partial(&mut partials, 1, "译文完成".into(), &waiters);
        assert_eq!(
            partials.into_iter().collect::<Vec<_>>(),
            vec![
                (1, "译文完成".into()),
                (2, "另一段".into()),
                (4, "译文完成".into()),
                (8, "译文完成".into()),
            ]
        );
    }

    #[test]
    fn visible_cache_hits_bypass_a_full_model_pool_and_keep_misses_queued() {
        let mut pending = std::collections::BTreeMap::from([
            (0, "Uncached paragraph".into()),
            (1, "Cached visible paragraph".into()),
            (2, "Offscreen paragraph".into()),
            (3, "Second cached visible paragraph".into()),
        ]);
        let entries = take_cached_translation_entries(
            &[0, 1, 3],
            &mut pending,
            &[None, Some("已缓存的可见段落".into()), Some("第二段".into())],
        );
        assert_eq!(
            entries,
            vec![(1, "已缓存的可见段落".into()), (3, "第二段".into())]
        );
        assert_eq!(pending.keys().copied().collect::<Vec<_>>(), vec![0, 2]);
        // Repeated cache delivery cannot consume another entry or inflate counts.
        assert!(
            take_cached_translation_entries(&[1], &mut pending, &[Some("第二次".into())])
                .is_empty()
        );
    }
    use rusqlite::params;

    #[test]
    fn accepts_camel_case_page_translation_request() {
        let action: BrowserAction = serde_json::from_value(json!({
            "kind": "translatePage",
            "targetLanguage": "zh-CN"
        }))
        .expect("frontend browser action should deserialize");
        match action {
            BrowserAction::TranslatePage { target_language } => {
                assert_eq!(target_language, "zh-CN");
            }
            _ => panic!("wrong browser action variant"),
        }
    }

    #[test]
    fn validates_saved_page_translation_language() {
        let valid = BrowserSettings {
            translation_language: "en".into(),
            ..BrowserSettings::default()
        };
        assert!(valid_browser_settings(&valid));

        let invalid = BrowserSettings {
            translation_language: "unsupported".into(),
            ..BrowserSettings::default()
        };
        assert!(!valid_browser_settings(&invalid));
    }

    #[test]
    fn reader_snapshot_accepts_empty_viewports_and_rejects_invalid_payloads() {
        let snapshot: PageTranslationSnapshot = serde_json::from_value(json!({
            "texts": [], "detectedLanguage": "en"
        }))
        .unwrap();
        assert!(snapshot.texts.is_empty());
        assert!(serde_json::from_value::<PageTranslationSnapshot>(Value::Null).is_err());
        assert!(serde_json::from_value::<PageTranslationSnapshot>(json!({
            "texts": [42], "detectedLanguage": "en"
        }))
        .is_err());
    }

    #[test]
    fn translation_session_progress_and_idle_delay_are_bounded() {
        let priority: PageTranslationPriority = serde_json::from_value(json!({
            "valid": [2, 4], "ready": [4]
        }))
        .unwrap();
        assert_eq!(priority.valid, vec![2, 4]);
        assert_eq!(priority.ready, vec![4]);
        assert!(serde_json::from_value::<PageTranslationPriority>(Value::Null).is_err());
        let progress = PageTranslationProgress {
            tab_id: "article".into(),
            token: "session".into(),
            queued: 2,
            deferred: 4,
            active: 1,
            completed: 3,
            failed: 1,
            running: true,
        };
        let wire = serde_json::to_value(progress).unwrap();
        assert_eq!(wire["tabId"], "article");
        assert_eq!(wire["queued"], 2);
        assert_eq!(wire["deferred"], 4);
        assert_eq!(wire["completed"], 3);
        assert_eq!(translation_poll_delay(0, true).as_millis(), 100);
        assert_eq!(translation_poll_delay(1, false).as_millis(), 150);
        assert_eq!(translation_poll_delay(100, false).as_millis(), 350);
    }

    #[test]
    fn automatic_target_waits_for_late_article_text() {
        let first = vec!["Menu Home".to_owned()];
        assert_eq!(
            automatic_page_target(&first, "en", std::time::Duration::from_millis(100)),
            None
        );
        let mut loaded = first;
        loaded.push("这是一篇关于人工智能应用的中文文章，正文稍后加载，内容包括技术背景、研究过程、实际应用和未来的发展方向。".repeat(4));
        assert_eq!(
            automatic_page_target(&loaded, "en", std::time::Duration::from_millis(300)),
            Some("en".to_owned())
        );
    }

    #[test]
    fn automatic_target_uses_short_page_after_bounded_wait_but_not_empty_page() {
        let elapsed = std::time::Duration::from_secs(2);
        assert_eq!(automatic_page_target(&[], "en", elapsed), None);
        assert_eq!(
            automatic_page_target(&["A short English article".to_owned()], "en", elapsed),
            Some("zh-CN".to_owned())
        );
    }

    #[test]
    fn automatic_target_starts_immediately_for_one_substantial_paragraph() {
        let article = "An English article explains how a desktop browser translates visible paragraphs efficiently and shows each completed result.";
        assert_eq!(
            automatic_page_target(
                &[article.to_owned()],
                "en",
                std::time::Duration::from_millis(10),
            ),
            Some("zh-CN".to_owned())
        );
        assert_eq!(
            automatic_page_target(
                &["Home Menu Search".to_owned()],
                "en",
                std::time::Duration::from_millis(10),
            ),
            None
        );
    }

    #[test]
    fn repeated_visible_paragraphs_share_one_active_model_request() {
        let mut active = std::collections::HashMap::new();
        let mut waiters = std::collections::HashMap::new();
        assert!(claim_translation_text(
            1,
            "Same paragraph",
            &mut active,
            &mut waiters
        ));
        assert!(!claim_translation_text(
            2,
            "Same paragraph",
            &mut active,
            &mut waiters
        ));
        assert!(!claim_translation_text(
            3,
            "Same paragraph",
            &mut active,
            &mut waiters
        ));
        assert!(claim_translation_text(
            4,
            "Different paragraph",
            &mut active,
            &mut waiters
        ));
        assert_eq!(active.len(), 2);
        assert_eq!(waiters.remove(&1), Some(vec![2, 3]));
        active.remove("Same paragraph");
        assert!(claim_translation_text(
            5,
            "Same paragraph",
            &mut active,
            &mut waiters
        ));
    }

    #[test]
    fn visible_paragraphs_are_batched_and_duplicates_join_the_owner() {
        let mut pending = std::collections::BTreeMap::from([
            (0, "first".to_owned()),
            (1, "second".to_owned()),
            (2, "first".to_owned()),
            (3, "third".to_owned()),
        ]);
        let mut active_texts = std::collections::HashMap::new();
        let mut waiters = std::collections::HashMap::new();
        let batch = take_visible_translation_batch(
            &[0, 1, 2, 3],
            &mut pending,
            &mut active_texts,
            &mut waiters,
            translator::PAGE_BATCH_SIZE,
            true,
        );
        assert_eq!(
            batch,
            vec![
                (0, "first".into()),
                (1, "second".into()),
                (3, "third".into())
            ]
        );
        assert_eq!(waiters.get(&0), Some(&vec![2]));
        assert!(pending.is_empty());
    }

    #[test]
    fn visible_batch_respects_character_limit_and_full_parallel_pool() {
        let long = "a".repeat(translator::PAGE_BATCH_CHARACTER_LIMIT - 10);
        let mut pending = std::collections::BTreeMap::from([
            (0, long.clone()),
            (1, "more than ten chars".to_owned()),
        ]);
        let mut active_texts = std::collections::HashMap::new();
        let mut waiters = std::collections::HashMap::new();
        let batch = take_visible_translation_batch(
            &[0, 1],
            &mut pending,
            &mut active_texts,
            &mut waiters,
            translator::PAGE_BATCH_SIZE,
            true,
        );
        assert_eq!(batch, vec![(0, long)]);
        assert!(pending.contains_key(&1));
        active_texts.insert("more than ten chars".into(), 9);
        let none = take_visible_translation_batch(
            &[1],
            &mut pending,
            &mut active_texts,
            &mut waiters,
            translator::PAGE_BATCH_SIZE,
            false,
        );
        assert!(none.is_empty());
        assert_eq!(waiters.get(&9), Some(&vec![1]));
    }

    #[test]
    fn twenty_four_short_paragraphs_need_three_model_requests() {
        let mut pending = (0..24)
            .map(|id| (id, format!("visible paragraph {id}")))
            .collect::<std::collections::BTreeMap<_, _>>();
        let ready = (0..24).collect::<Vec<_>>();
        let mut active_texts = std::collections::HashMap::new();
        let mut waiters = std::collections::HashMap::new();
        let batches = (0..3)
            .map(|_| {
                take_visible_translation_batch(
                    &ready,
                    &mut pending,
                    &mut active_texts,
                    &mut waiters,
                    translator::PAGE_BATCH_SIZE,
                    true,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), [8, 8, 8]);
        assert!(pending.is_empty());
    }

    #[test]
    fn flash_plan_dispatches_distinct_visible_paragraphs_independently() {
        let (max_items, max_parallel) = translator::page_request_plan("deepseek-flash");
        let mut pending = (0..5)
            .map(|id| (id, format!("paragraph {id}")))
            .collect::<std::collections::BTreeMap<_, _>>();
        let ready = (0..5).collect::<Vec<_>>();
        let mut active_texts = std::collections::HashMap::new();
        let mut waiters = std::collections::HashMap::new();
        for _ in 0..max_parallel {
            let batch = take_visible_translation_batch(
                &ready,
                &mut pending,
                &mut active_texts,
                &mut waiters,
                max_items,
                true,
            );
            assert_eq!(batch.len(), 1);
        }
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn exposes_only_completed_native_download_paths() {
        let profile = browser_profile::open(std::path::PathBuf::from(":memory:"))
            .expect("in-memory browser profile should open");
        {
            let session = profile
                .inner
                .lock()
                .expect("profile lock should be available");
            for (state, path) in [
                ("complete", "/trusted/download.pdf"),
                ("downloading", "/trusted/partial.dmg"),
                ("cancelled", "/trusted/cancelled.zip"),
            ] {
                session
                    .database
                    .execute(
                        "INSERT INTO records(kind,url,title,detail,time) VALUES('download','https://example.com','','' || ?1 || char(10) || ?2,1)",
                        params![state, path],
                    )
                    .expect("download fixture should insert");
            }
        }

        assert_eq!(
            download_path(&profile, 1).expect("completed download should be available"),
            "/trusted/download.pdf"
        );
        assert!(download_path(&profile, 2).is_err());
        assert!(download_path(&profile, 3).is_err());
        assert!(download_path(&profile, 999).is_err());
    }
}
