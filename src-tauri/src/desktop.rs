//! Desktop adapter for an isolated article webview embedded in the main window.

use crate::browser_profile::{self, BrowserProfile};
use serde::Deserialize;
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{
    webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewBuilder},
    Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, WebviewUrl,
};

/// Keep ordinary `target=_blank` links inside the single reading pane. The
/// native new-window handler below remains the security backstop for scripts
/// that call `window.open` directly.
const SINGLE_PANE_LINK_SCRIPT: &str = r#"
document.addEventListener('click', (event) => {
  const path = event.composedPath();
  const link = path.find((node) => node instanceof HTMLAnchorElement);
  if (!link || link.target.toLowerCase() !== '_blank') return;
  try {
    const target = new URL(link.href, document.baseURI);
    if (target.protocol !== 'http:' && target.protocol !== 'https:') return;
    event.preventDefault();
    window.location.assign(target.href);
  } catch (_) {}
}, true);
"#;

/// CSS-pixel rectangle measured by the trusted UI; never supplied by remote pages.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub viewport_height: f64,
}

impl BrowserBounds {
    fn validate(&self) -> Result<(), String> {
        if [
            self.x,
            self.y,
            self.width,
            self.height,
            self.viewport_height,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || self.x < 0.0
            || self.y < 0.0
            || self.width < 1.0
            || self.height < 1.0
            || self.viewport_height < 1.0
        {
            return Err("无效浏览区域".into());
        }
        Ok(())
    }
}

fn is_article_url(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && !matches!(url.host_str(), Some("tauri.localhost" | "ipc.localhost"))
        && !(matches!(url.host_str(), Some("127.0.0.1" | "localhost")) && url.port() == Some(1420))
}

pub(crate) fn tab_label(tab_id: &str) -> Result<String, String> {
    if tab_id.is_empty()
        || tab_id.len() > 64
        || !tab_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_'))
    {
        return Err("无效浏览器标签页".into());
    }
    Ok(format!("article-{tab_id}"))
}

fn hide_article_views(app: &tauri::AppHandle, except: Option<&str>) -> Result<(), String> {
    for (label, view) in app.webviews() {
        if label.starts_with("article-") && except != Some(label.as_str()) {
            view.hide().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Apply serialized UI requests off the UI thread (required by WebView2).
/// Capabilities target only the main webview, not every webview in its window.
pub fn browser_request(
    app: &tauri::AppHandle,
    action: &str,
    tab_id: Option<String>,
    url: Option<String>,
    bounds: Option<BrowserBounds>,
) -> Result<(), String> {
    let result = match action {
        "closeAll" => {
            {
                let profile = app.state::<BrowserProfile>();
                let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
                session.tabs.clear();
                session.active_tab = None;
            }
            for (label, view) in app.webviews() {
                if label.starts_with("article-") {
                    view.close().map_err(|error| error.to_string())?;
                }
            }
            Ok(())
        }
        "hideAll" => {
            app.state::<BrowserProfile>()
                .inner
                .lock()
                .map_err(|e| e.to_string())?
                .active_tab = None;
            hide_article_views(app, None)
        }
        "close" => {
            let tab_id = tab_id.as_deref().ok_or("缺少浏览器标签页")?;
            let label = tab_label(tab_id)?;
            {
                let profile = app.state::<BrowserProfile>();
                let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
                session.tabs.remove(tab_id);
                if session.active_tab.as_deref() == Some(tab_id) {
                    session.active_tab = None;
                }
            }
            app.get_webview(&label).map_or(Ok(()), |view| {
                view.close().map_err(|error| error.to_string())
            })
        }
        "sync" => {
            let tab_id = tab_id.as_deref().ok_or("缺少浏览器标签页")?;
            let label = tab_label(tab_id)?;
            let bounds = bounds.ok_or("缺少浏览区域")?;
            bounds.validate()?;
            let engine = app
                .state::<BrowserProfile>()
                .inner
                .lock()
                .map_err(|e| e.to_string())?
                .settings
                .search_engine
                .clone();
            let parsed = url
                .as_deref()
                .map(|input| browser_profile::resolve_address(input, &engine))
                .transpose()?;
            // The main webview can be inset by native titlebar decoration on macOS.
            // Translate DOM coordinates from that view into its parent window.
            let main = app.get_webview("main").ok_or("主界面不存在")?;
            let scale = main.window().scale_factor().map_err(|e| e.to_string())?;
            let origin = main
                .position()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            // macOS extends the root NSView under the titlebar, but WKWebView's
            // DOM viewport excludes that safe area. Native window sizes include
            // it too, so compare the main webview height with the DOM height.
            #[cfg(target_os = "macos")]
            let titlebar = (main
                .size()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale)
                .height
                - bounds.viewport_height)
                .max(0.0);
            #[cfg(not(target_os = "macos"))]
            let titlebar = 0.0;
            let position =
                LogicalPosition::new(bounds.x + origin.x, bounds.y + origin.y + titlebar);
            let size = LogicalSize::new(bounds.width, bounds.height);
            hide_article_views(app, Some(&label))?;
            app.state::<BrowserProfile>()
                .inner
                .lock()
                .map_err(|e| e.to_string())?
                .active_tab = Some(tab_id.into());
            if let Some(view) = app.get_webview(&label) {
                view.set_bounds(tauri::Rect {
                    position: position.into(),
                    size: size.into(),
                })
                .map_err(|e| e.to_string())?;
                view.show().map_err(|e| e.to_string())?;
                if let Some(url) = parsed {
                    view.navigate(url).map_err(|e| e.to_string())?;
                }
                Ok(())
            } else if let Some(url) = parsed {
                let popup_app = app.clone();
                let title_tab = tab_id.to_string();
                let page_tab = tab_id.to_string();
                let popup_tab = tab_id.to_string();
                let zoom = app
                    .state::<BrowserProfile>()
                    .inner
                    .lock()
                    .map_err(|e| e.to_string())?
                    .settings
                    .zoom;
                if !is_article_url(&url) {
                    return Err("不能在浏览区打开应用内部地址".into());
                }
                // WebKit reports no destination in the finished event on
                // macOS, so retain the native-assigned path by URL until the
                // matching completion arrives.
                let pending_downloads =
                    Arc::new(Mutex::new(HashMap::<String, VecDeque<PathBuf>>::new()));
                let download_paths = pending_downloads.clone();
                let builder = WebviewBuilder::new(label, WebviewUrl::External(url))
                    .data_store_identifier(*b"TopicDeskBrowser")
                    .initialization_script(SINGLE_PANE_LINK_SCRIPT)
                    .data_directory(
                        app.path()
                            .app_data_dir()
                            .map_err(|e| e.to_string())?
                            .join("browser-webview"),
                    )
                    .on_navigation(is_article_url)
                    .on_document_title_changed(move |view, title| {
                        browser_profile::title_changed(view.app_handle(), &title_tab, title)
                    })
                    .on_page_load(move |view, payload| {
                        // WebKit navigation callbacks can include child frames.
                        // Reading the WebView itself yields the canonical top URL.
                        let Ok(current) = view.url() else {
                            return;
                        };
                        browser_profile::navigated(view.app_handle(), &page_tab, current.as_str());
                        if matches!(payload.event(), PageLoadEvent::Finished) {
                            browser_profile::page_finished(
                                view.app_handle(),
                                &page_tab,
                                current.as_str(),
                            );
                        }
                    })
                    .on_download(move |view, event| {
                        match event {
                            DownloadEvent::Requested { url, destination } => {
                                if !is_article_url(&url) {
                                    return false;
                                }
                                let Ok(directory) = view.app_handle().path().download_dir() else {
                                    return false;
                                };
                                let name = destination
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                let name: String = name
                                    .chars()
                                    .filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':'))
                                    .take(160)
                                    .collect();
                                *destination = directory.join(format!(
                                    "{}-{}",
                                    browser_profile::now(),
                                    if name.is_empty() { "download" } else { &name }
                                ));
                                if let Ok(mut pending) = download_paths.lock() {
                                    pending
                                        .entry(url.to_string())
                                        .or_default()
                                        .push_back(destination.clone());
                                }
                            }
                            DownloadEvent::Finished { url, path, success } => {
                                let fallback =
                                    download_paths.lock().ok().and_then(|mut pending| {
                                        let path = pending
                                            .get_mut(url.as_str())
                                            .and_then(VecDeque::pop_front);
                                        if pending.get(url.as_str()).is_some_and(VecDeque::is_empty)
                                        {
                                            pending.remove(url.as_str());
                                        }
                                        path
                                    });
                                if let Some(path) = path.or(fallback) {
                                    browser_profile::record_download(
                                        view.app_handle(),
                                        url.as_str(),
                                        &path,
                                        if success { "complete" } else { "failed" },
                                    );
                                }
                            }
                            _ => (),
                        }
                        true
                    })
                    .on_new_window(move |url, _| {
                        // Returning `Deny` cancels the popup navigation. Route the
                        // validated URL through the trusted main view afterward so
                        // the existing article view can navigate normally.
                        if is_article_url(&url) {
                            let _ = popup_app.emit_to(
                                EventTarget::webview("main"),
                                "browser-open-popup",
                                serde_json::json!({"tabId":popup_tab,"url":url.as_str()}),
                            );
                        }
                        NewWindowResponse::Deny
                    });
                app.get_window("main")
                    .ok_or("主窗口不存在")?
                    .add_child(builder, position, size)
                    .and_then(|view| view.set_zoom(zoom))
                    .map_err(|error| error.to_string())
            } else {
                Ok(())
            }
        }
        "back" | "forward" | "reload" => {
            let tab_id = tab_id.as_deref().ok_or("缺少浏览器标签页")?;
            let label = tab_label(tab_id)?;
            let view = app.get_webview(&label).ok_or("浏览区域未打开")?;
            if action != "reload" {
                let profile = app.state::<BrowserProfile>();
                let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
                session.tabs.entry(tab_id.into()).or_default().traversal = true;
            }
            match action {
                "back" => view.eval("history.back()"),
                "forward" => view.eval("history.forward()"),
                _ => view.reload(),
            }
            .map_err(|error| error.to_string())
        }
        _ => return Err("未知浏览操作".into()),
    };
    result.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_native_bounds() {
        let mut bounds = BrowserBounds {
            x: 220.0,
            y: 100.0,
            width: 600.0,
            height: 700.0,
            viewport_height: 800.0,
        };
        assert!(bounds.validate().is_ok());
        for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            bounds.width = width;
            assert!(bounds.validate().is_err());
        }
        bounds.width = 600.0;
        bounds.x = -10.0;
        assert!(bounds.validate().is_err());
    }

    #[test]
    fn accepts_web_articles_and_rejects_local_or_executable_urls() {
        for value in [
            "https://example.com/article?q=1#section",
            "http://example.com",
        ] {
            assert!(is_article_url(&url::Url::parse(value).unwrap()));
        }
        for value in [
            "not a url",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,test",
            "tauri://localhost",
            "about:blank",
        ] {
            if let Ok(url) = url::Url::parse(value) {
                assert!(!is_article_url(&url), "navigation must also reject {value}");
            }
        }
    }

    #[test]
    fn native_tab_labels_accept_only_safe_local_identifiers() {
        assert_eq!(tab_label("tab-42").unwrap(), "article-tab-42");
        for value in ["", "../main", "tab 1", "tab/1", &"a".repeat(65)] {
            assert!(tab_label(value).is_err(), "label must reject {value:?}");
        }
    }
}

/// Capture only the web content through WKWebView, without desktop screen permission.
#[cfg(target_os = "macos")]
pub fn capture_screenshot(view: &tauri::Webview) -> Result<Vec<u8>, String> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    use objc2_web_kit::WKWebView;
    let (tx, rx) = std::sync::mpsc::channel();
    view.with_webview(move |platform| {
        let block = block2::RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            let result = (|| {
                if !error.is_null() || image.is_null() {
                    return Err("网页截图失败".to_string());
                }
                // WebKit owns these callback pointers for the duration of this call.
                let image = unsafe { &*image };
                let tiff = image.TIFFRepresentation().ok_or("无法编码网页截图")?;
                let bitmap = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)
                    .ok_or("无法读取网页截图")?;
                let png = unsafe {
                    bitmap.representationUsingType_properties(
                        NSBitmapImageFileType::PNG,
                        &NSDictionary::new(),
                    )
                }
                .ok_or("无法生成 PNG")?;
                Ok(png.to_vec())
            })();
            let _ = tx.send(result);
        });
        // Tauri guarantees this callback executes on the UI thread with a live WKWebView.
        unsafe {
            (&*platform.inner().cast::<WKWebView>())
                .takeSnapshotWithConfiguration_completionHandler(None, &block);
        }
    })
    .map_err(|e| e.to_string())?;
    let bytes = rx
        .recv_timeout(std::time::Duration::from_secs(20))
        .map_err(|_| "网页截图超时")??;
    Ok(bytes)
}

/// Other desktop adapters keep the command explicit until their native capture is provided.
#[cfg(not(target_os = "macos"))]
pub fn capture_screenshot(_view: &tauri::Webview) -> Result<Vec<u8>, String> {
    Err("当前平台暂不支持原生网页截图".into())
}

/// Save a native viewport capture into a caller-controlled download destination.
pub fn save_screenshot(view: &tauri::Webview, path: &std::path::Path) -> Result<(), String> {
    std::fs::write(path, capture_screenshot(view)?).map_err(|e| e.to_string())
}
