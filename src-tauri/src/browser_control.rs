//! Typed browser chrome actions; every native/profile operation requires the main webview.

use crate::browser_profile::{self, BrowserProfile, BrowserSettings};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{webview::Cookie, Manager};
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

/// Execute only from the privileged main view and outside the native event-loop thread.
#[tauri::command]
pub async fn browser_control(
    app: tauri::AppHandle,
    webview: tauri::Webview,
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
            if !["bing", "google", "duckduckgo"].contains(&settings.search_engine.as_str())
                || !settings.zoom.is_finite()
                || !(0.25..=3.0).contains(&settings.zoom)
            {
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
            let (_, path) = detail.split_once('\n').ok_or("下载路径无效")?;
            app.opener()
                .reveal_item_in_dir(path)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
    }
}
