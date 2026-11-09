//! Trusted local popover WebView above article views. External pages never receive its IPC privileges.
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, EventTarget, Manager};

const LABEL: &str = "browser-popover";

/// Public display data only; executable handlers remain in the main UI.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopoverItem {
    id: String,
    text: String,
    enabled: bool,
    separator: bool,
    children: Vec<PopoverItem>,
    #[serde(default)]
    layout: String,
    #[serde(default)]
    keep_open: bool,
}

/// Session-scoped presentation with bounds in main-window logical pixels.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopoverData {
    token: String,
    title: String,
    theme: String,
    items: Vec<PopoverItem>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Only one popover can be active, preventing stale close/action messages from affecting a newer popup.
#[derive(Default)]
pub struct PopoverState(Mutex<Option<PopoverData>>);

fn valid_items(items: &[PopoverItem], depth: usize, count: &mut usize) -> bool {
    if depth > 2 {
        return false;
    }
    items.iter().all(|item| {
        *count += 1;
        *count <= 120
            && item.id.len() <= 64
            && item.text.len() <= 8192
            && matches!(item.layout.as_str(), "" | "zoom")
            && valid_items(&item.children, depth + 1, count)
    })
}

fn validate(data: &PopoverData) -> Result<(), String> {
    let mut count = 0;
    if data.token.is_empty()
        || data.token.len() > 128
        || data.title.len() > 256
        || !matches!(data.theme.as_str(), "light" | "dark")
        || ![data.x, data.y, data.width, data.height]
            .iter()
            .all(|v| v.is_finite())
        || data.x < 0.0
        || data.y < 0.0
        || !(200.0..=480.0).contains(&data.width)
        || !(48.0..=900.0).contains(&data.height)
        || !valid_items(&data.items, 0, &mut count)
    {
        return Err("弹层数据无效".into());
    }
    Ok(())
}

/// The child is recreated above the current article tab, including tabs created after a prior popup.
fn close_view(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(view) = app.get_webview(LABEL) {
        view.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn local_popover_url(url: &url::Url) -> bool {
    let origin = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost"))
        || (cfg!(debug_assertions)
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
            && url.port() == Some(1420));
    origin && matches!(url.path(), "/" | "/index.html") && url.query() == Some("browser-popover")
}

fn enabled_action(items: &[PopoverItem], id: &str) -> bool {
    items.iter().any(|item| {
        item.enabled
            && !item.separator
            && ((item.id == id && item.children.is_empty()) || enabled_action(&item.children, id))
    })
}

fn keeps_open(items: &[PopoverItem], id: &str) -> bool {
    items
        .iter()
        .any(|item| (item.id == id && item.keep_open) || keeps_open(&item.children, id))
}

/// Main chrome refreshes saved zoom display in place without affecting the article or popup session.
#[tauri::command]
pub fn browser_popover_update(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, PopoverState>,
    data: PopoverData,
) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("无权更新弹层".into());
    }
    validate(&data)?;
    let mut saved = state.0.lock().map_err(|e| e.to_string())?;
    if saved
        .as_ref()
        .is_none_or(|current| current.token != data.token)
    {
        return Ok(());
    }
    *saved = Some(data.clone());
    drop(saved);
    app.emit_to(EventTarget::webview(LABEL), "browser-popover-data", &data)
        .map_err(|e| e.to_string())
}

/// Main-only creation; an offscreen initial position prevents showing an empty loading WebView.
#[tauri::command]
pub async fn browser_popover_open(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, PopoverState>,
    data: PopoverData,
) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("无权打开弹层".into());
    }
    validate(&data)?;
    *state.0.lock().map_err(|e| e.to_string())? = Some(data.clone());
    close_view(&app)?;
    if app.get_webview(LABEL).is_none() {
        let handle = app.clone();
        let (tx, mut rx) = tauri::async_runtime::channel(1);
        app.run_on_main_thread(move || {
            let result = (|| {
                let builder = tauri::webview::WebviewBuilder::new(
                    LABEL,
                    tauri::WebviewUrl::App("index.html?browser-popover".into()),
                )
                .focused(false)
                .incognito(true)
                // A local chrome surface must never navigate to arbitrary websites.
                .on_navigation(local_popover_url)
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny);
                handle
                    .get_window("main")
                    .ok_or("主窗口不存在")?
                    .add_child(
                        builder,
                        tauri::LogicalPosition::new(-10000.0, -10000.0),
                        tauri::LogicalSize::new(360.0, 100.0),
                    )
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>(())
            })();
            let _ = tx.try_send(result);
        })
        .map_err(|e| e.to_string())?;
        rx.recv().await.ok_or("弹层创建未完成")??;
    }
    if state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_none_or(|current| current.token != data.token)
    {
        close_view(&app)?;
        return Ok(());
    }
    app.emit_to(EventTarget::webview(LABEL), "browser-popover-data", &data)
        .map_err(|e| e.to_string())
}

/// The local popup reads its latest presentation after mounting. Article views are rejected.
#[tauri::command]
pub fn browser_popover_data(
    webview: tauri::Webview,
    state: tauri::State<'_, PopoverState>,
) -> Result<Option<PopoverData>, String> {
    if webview.label() != LABEL {
        return Err("无权读取弹层".into());
    }
    Ok(state.0.lock().map_err(|e| e.to_string())?.clone())
}

/// Show only after React commits the popup; geometry is bounded by the main-supplied available area.
#[tauri::command]
pub async fn browser_popover_ready(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, PopoverState>,
    token: String,
    height: f64,
) -> Result<(), String> {
    if webview.label() != LABEL || !height.is_finite() {
        return Err("弹层尺寸无效".into());
    }
    let data = state.0.lock().map_err(|e| e.to_string())?.clone();
    if let Some(data) = data.filter(|data| data.token == token) {
        webview
            .set_bounds(tauri::Rect {
                position: tauri::LogicalPosition::new(data.x, data.y).into(),
                size: tauri::LogicalSize::new(data.width, height.clamp(48.0, data.height)).into(),
            })
            .map_err(|e| e.to_string())?;
        webview.show().map_err(|e| e.to_string())?;
        webview.set_focus().map_err(|e| e.to_string())?;
        app.emit_to(
            EventTarget::webview("main"),
            "browser-popover-event",
            serde_json::json!({"token": token, "kind":"ready"}),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Forward only bounded UI events from trusted chrome, never raw commands or external URLs.
#[tauri::command]
pub async fn browser_popover_event(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, PopoverState>,
    token: String,
    kind: String,
    id: Option<String>,
) -> Result<(), String> {
    let label = webview.label();
    if !matches!(label, "main" | LABEL)
        || !matches!(kind.as_str(), "enter" | "leave" | "close" | "action")
        || id.as_ref().is_some_and(|id| id.len() > 64)
    {
        return Err("弹层事件无效".into());
    }
    let mut saved = state.0.lock().map_err(|e| e.to_string())?;
    if saved.as_ref().is_none_or(|data| data.token != token) {
        return Ok(());
    }
    if kind == "action"
        && !id.as_deref().is_some_and(|id| {
            saved
                .as_ref()
                .is_some_and(|data| enabled_action(&data.items, id))
        })
    {
        return Err("弹层操作无效".into());
    }
    let keep_open = kind == "action"
        && id.as_deref().is_some_and(|id| {
            saved
                .as_ref()
                .is_some_and(|data| keeps_open(&data.items, id))
        });
    if kind == "close" || (kind == "action" && !keep_open) {
        close_view(&app)?;
        *saved = None;
    }
    drop(saved);
    app.emit_to(
        EventTarget::webview("main"),
        "browser-popover-event",
        serde_json::json!({"token": token,"kind":kind,"id":id}),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_navigation_never_accepts_article_or_other_local_services() {
        assert!(local_popover_url(
            &url::Url::parse("tauri://localhost/index.html?browser-popover").unwrap()
        ));
        assert!(local_popover_url(
            &url::Url::parse("http://tauri.localhost/index.html?browser-popover").unwrap()
        ));
        for value in [
            "https://example.org/index.html?browser-popover",
            "http://localhost:8080/index.html?browser-popover",
            "tauri://localhost/index.html",
            "tauri://localhost/other?browser-popover",
        ] {
            assert!(!local_popover_url(&url::Url::parse(value).unwrap()));
        }
    }

    #[test]
    fn action_must_be_an_enabled_leaf_in_the_current_display() {
        let mut item = PopoverItem {
            id: "allowed".into(),
            text: "Title".into(),
            enabled: true,
            separator: false,
            children: vec![],
            layout: String::new(),
            keep_open: false,
        };
        assert!(enabled_action(&[item.clone()], "allowed"));
        assert!(!keeps_open(&[item.clone()], "allowed"));
        item.keep_open = true;
        assert!(keeps_open(&[item.clone()], "allowed"));
        assert!(!enabled_action(&[item.clone()], "unknown"));
        item.enabled = false;
        assert!(!enabled_action(&[item], "allowed"));
    }

    #[test]
    fn rejects_invalid_geometry_unbounded_content_and_unknown_themes() {
        let mut data = PopoverData {
            token: "session".into(),
            title: "Menu".into(),
            theme: "light".into(),
            items: vec![],
            x: 10.0,
            y: 40.0,
            width: 360.0,
            height: 500.0,
        };
        assert!(validate(&data).is_ok());
        data.x = f64::NAN;
        assert!(validate(&data).is_err());
        data.x = 0.0;
        data.height = -1.0;
        assert!(validate(&data).is_err());
        data.height = 500.0;
        data.theme = "other".into();
        assert!(validate(&data).is_err());
        data.theme = "dark".into();
        data.items = (0..121)
            .map(|_| PopoverItem {
                id: "id".into(),
                text: "text".into(),
                enabled: true,
                separator: false,
                children: vec![],
                layout: String::new(),
                keep_open: false,
            })
            .collect();
        assert!(validate(&data).is_err());
    }
}
