//! Desktop-only cookie capture/restore. Native cookie APIs run off the UI thread on every platform.
use crate::browser_sessions::CookieVault;
#[cfg(any(not(target_os = "macos"), test))]
use crate::browser_sessions::SavedCookie;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
#[cfg(any(not(target_os = "macos"), test))]
use tauri::webview::Cookie;
use tauri::{Emitter, EventTarget, Manager};

/// One lock serializes native reads with restore/clear so pending snapshots cannot undo a clear.
pub(crate) struct BrowserLoginSessions {
    operation: Mutex<()>,
    vault: Mutex<CookieVault>,
    last_error: Mutex<Option<String>>,
    pub exiting: AtomicBool,
    pub closing_window: AtomicBool,
}
fn now() -> i64 {
    (crate::browser_profile::now() / 1_000) as i64
}

/// Start bounded native background snapshots; cookie values are never emitted to the UI.
pub(crate) fn install(app: &tauri::AppHandle, directory: &Path) -> Result<(), String> {
    app.manage(BrowserLoginSessions {
        operation: Mutex::new(()),
        vault: Mutex::new(CookieVault::open(directory)?),
        last_error: Mutex::new(None),
        exiting: AtomicBool::new(false),
        closing_window: AtomicBool::new(false),
    });
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if app
            .state::<BrowserLoginSessions>()
            .exiting
            .load(Ordering::Acquire)
        {
            break;
        }
        if let Err(error) = capture_all(&app) {
            report(&app, error);
        }
    });
    Ok(())
}
/// Surface a deduplicated persistence error without blocking unrelated tab cleanup.
pub(crate) fn report(app: &tauri::AppHandle, error: String) {
    if let Ok(mut last) = app.state::<BrowserLoginSessions>().last_error.lock() {
        if last.as_ref() != Some(&error) {
            let _ = app.emit_to(
                EventTarget::webview("main"),
                "browser-session-error",
                &error,
            );
            *last = Some(error);
        }
    }
}
#[cfg(any(not(target_os = "macos"), test))]
fn saved(cookie: &Cookie<'_>) -> SavedCookie {
    SavedCookie {
        name: cookie.name().into(),
        value: cookie.value().into(),
        domain: cookie.domain().unwrap_or_default().into(),
        path: cookie.path().unwrap_or("/").into(),
        secure: cookie.secure().unwrap_or(false),
        http_only: cookie.http_only().unwrap_or(false),
        same_site: cookie.same_site().map(|value| value.to_string()),
        expires: cookie.expires_datetime().map(|time| time.unix_timestamp()),
        remember_until: 0,
    }
}
#[cfg(any(not(target_os = "macos"), test))]
fn native(cookie: &SavedCookie) -> Result<Cookie<'static>, String> {
    let mut result = Cookie::build((cookie.name.clone(), cookie.value.clone()))
        .domain(cookie.domain.clone())
        .path(cookie.path.clone())
        .secure(cookie.secure)
        .http_only(cookie.http_only)
        .build();
    if let Some(expires) = cookie.expires {
        result.set_expires(
            time::OffsetDateTime::from_unix_timestamp(expires)
                .map_err(|_| "登录 Cookie 到期时间无效")?,
        );
    }
    if let Some(value) = &cookie.same_site {
        result.set_same_site(match value.as_str() {
            "Strict" => cookie::SameSite::Strict,
            "Lax" => cookie::SameSite::Lax,
            _ => cookie::SameSite::None,
        });
    }
    Ok(result)
}
fn capture_view_unlocked(app: &tauri::AppHandle, view: &tauri::Webview) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let records = crate::desktop_sessions_macos::snapshot(view)?;
    #[cfg(not(target_os = "macos"))]
    let records = view
        .cookies()
        .map_err(|_| "无法读取网站 Cookie，登录状态暂未保存")?
        .iter()
        .map(saved)
        .collect();
    app.state::<BrowserLoginSessions>()
        .vault
        .lock()
        .map_err(|_| "登录会话锁已损坏")?
        .capture(view.label(), records, now())
}
/// Snapshot every isolated view before app exit or opening another tab.
pub(crate) fn capture_all(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<BrowserLoginSessions>();
    let _operation = state.operation.lock().map_err(|_| "登录会话锁已损坏")?;
    let mut first_error = None;
    for (label, view) in app.webviews() {
        if label.starts_with("article-") {
            if let Err(error) = capture_view_unlocked(app, &view) {
                first_error.get_or_insert(error);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}
/// A tab must be read before destruction; hidden views continue to be snapshotted.
pub(crate) fn capture_before_close(
    app: &tauri::AppHandle,
    view: &tauri::Webview,
) -> Result<(), String> {
    let state = app.state::<BrowserLoginSessions>();
    let _operation = state.operation.lock().map_err(|_| "登录会话锁已损坏")?;
    capture_view_unlocked(app, view)?;
    state
        .vault
        .lock()
        .map_err(|_| "登录会话锁已损坏")?
        .forget_tab(view.label());
    Ok(())
}
/// Restore into a blank temporary view and only then navigate, so the first HTTP
/// request already has its login cookies. Errors never fall back to plaintext.
pub(crate) fn restore_and_navigate(
    app: &tauri::AppHandle,
    view: &tauri::Webview,
    url: url::Url,
) -> Result<(), String> {
    let state = app.state::<BrowserLoginSessions>();
    let _operation = state.operation.lock().map_err(|_| "登录会话锁已损坏")?;
    let cookies = state
        .vault
        .lock()
        .map_err(|_| "登录会话锁已损坏")?
        .restore(view.label(), now())?;
    #[cfg(target_os = "macos")]
    crate::desktop_sessions_macos::restore_shared(view, cookies)?;
    #[cfg(not(target_os = "macos"))]
    for cookie in &cookies {
        view.set_cookie(native(cookie)?)
            .map_err(|_| "无法恢复网站登录 Cookie")?;
    }
    view.navigate(url).map_err(|_| "无法打开网页".to_owned())
}
/// Clearing applies to all tabs and saved cookies, even without an active page.
pub(crate) fn clear(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<BrowserLoginSessions>();
    let _operation = state.operation.lock().map_err(|_| "登录会话锁已损坏")?;
    // Clear disk first, even when an invalid key prevents loading the old payload.
    state
        .vault
        .lock()
        .map_err(|_| "登录会话锁已损坏")?
        .clear()?;
    // Native "clear browsing data" APIs return before deletion completes on
    // several engines. Destroy the temporary stores instead, so a queued
    // snapshot cannot read old cookies and repopulate the encrypted vault.
    for (label, view) in app.webviews() {
        if label.starts_with("article-") {
            crate::desktop_media::stop(&view);
            view.close().map_err(|_| "无法清除网页临时会话")?;
        }
    }
    #[cfg(target_os = "macos")]
    crate::desktop_sessions_macos::reset_shared(app)?;
    let profile = app.state::<crate::browser_profile::BrowserProfile>();
    let mut browser = profile.inner.lock().map_err(|_| "浏览器状态锁已损坏")?;
    browser.tabs.clear();
    browser.active_tab = None;
    Ok(())
}
/// Snapshot after the page loads without blocking native navigation callbacks.
pub(crate) fn page_finished(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = capture_all(&app) {
            report(&app, error);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_native_cookie_round_trip_retains_http_only_secure_and_same_site() {
        let record = SavedCookie {
            name: "session".into(),
            value: "fake-token".into(),
            domain: "example.org".into(),
            path: "/auth".into(),
            secure: true,
            http_only: true,
            same_site: Some("Strict".into()),
            expires: Some(1_900_000_000),
            remember_until: 0,
        };
        let restored = saved(&native(&record).unwrap());
        assert_eq!(restored.value, record.value);
        assert_eq!(restored.domain, record.domain);
        assert_eq!(restored.path, record.path);
        assert_eq!(restored.same_site, record.same_site);
        assert_eq!(restored.expires, record.expires);
        assert!(restored.http_only && restored.secure);
    }
}
