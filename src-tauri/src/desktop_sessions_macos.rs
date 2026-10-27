//! WebKit cookie bridge preserves host-only/subdomain scope, which the generic cookie type normalizes.
use crate::browser_sessions::SavedCookie;
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_foundation::{
    ns_string, NSArray, NSDate, NSHTTPCookie, NSHTTPCookieDomain, NSHTTPCookieExpires,
    NSHTTPCookieName, NSHTTPCookiePath, NSHTTPCookiePropertyKey, NSHTTPCookieSecure,
    NSHTTPCookieValue, NSMutableDictionary, NSString,
};
use objc2_web_kit::{WKWebView, WKWebsiteDataStore};
use std::cell::RefCell;
thread_local! {
    static ARTICLE_CONTEXT: RefCell<crate::session_context::SessionContext<Retained<WKWebsiteDataStore>>> = RefCell::default();
}

/// Each view gets its own configuration/controller but shares one temporary store
/// and its attached ephemeral WebCrypto delegate for the application lifetime.
pub(crate) fn shared_store(
    create: impl FnOnce() -> Result<Retained<WKWebsiteDataStore>, String>,
) -> Result<Retained<WKWebsiteDataStore>, String> {
    if objc2::MainThreadMarker::new().is_none() {
        return Err("浏览器会话必须在主线程访问".into());
    }
    ARTICLE_CONTEXT.with(|state| state.borrow_mut().get_or_try_init(create))
}

/// Dropping the retained shared store ensures new tabs cannot reuse cleared site data.
pub(crate) fn reset_shared(app: &tauri::AppHandle) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        ARTICLE_CONTEXT.with(|state| state.borrow_mut().reset());
        let _ = tx.send(());
    })
    .map_err(|_| "无法重置浏览器临时会话")?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| "重置浏览器会话超时".into())
}

/// Restore disk cookies only for the first view of a new temporary context.
/// Subsequent tabs already share live site data, including fresh verification results.
pub(crate) fn restore_shared(
    view: &tauri::Webview,
    cookies: Vec<SavedCookie>,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    view.with_webview(move |_| {
        let needed = ARTICLE_CONTEXT.with(|state| state.borrow().needs_restore());
        let _ = tx.send(needed);
    })
    .map_err(|_| "无法读取浏览器会话状态")?;
    let needed = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| "读取浏览器会话状态超时")?;
    if needed {
        restore(view, cookies)?;
        let (tx, rx) = std::sync::mpsc::channel();
        view.with_webview(move |_| {
            ARTICLE_CONTEXT.with(|state| state.borrow_mut().mark_restored());
            let _ = tx.send(());
        })
        .map_err(|_| "无法更新浏览器会话状态")?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| "更新浏览器会话状态超时")?;
    }
    Ok(())
}

fn saved(cookie: &NSHTTPCookie) -> SavedCookie {
    let same_site = cookie
        .sameSitePolicy()
        .map(|value| value.to_string().to_lowercase())
        .and_then(|value| match value.as_str() {
            "lax" => Some("Lax".into()),
            "strict" => Some("Strict".into()),
            "none" => Some("None".into()),
            _ => None,
        });
    SavedCookie {
        name: cookie.name().to_string(),
        value: cookie.value().to_string(),
        domain: cookie.domain().to_string(),
        path: cookie.path().to_string(),
        secure: cookie.isSecure(),
        http_only: cookie.isHTTPOnly(),
        same_site,
        expires: cookie
            .expiresDate()
            .map(|date| date.timeIntervalSince1970() as i64),
        remember_until: 0,
    }
}
fn native(cookie: &SavedCookie) -> Result<Retained<NSHTTPCookie>, String> {
    let name = NSString::from_str(&cookie.name);
    let value = NSString::from_str(&cookie.value);
    let domain = NSString::from_str(&cookie.domain);
    let path = NSString::from_str(&cookie.path);
    let properties: Retained<NSMutableDictionary<NSHTTPCookiePropertyKey, AnyObject>> = unsafe {
        NSMutableDictionary::from_slices(
            &[
                NSHTTPCookieName,
                NSHTTPCookieValue,
                NSHTTPCookieDomain,
                NSHTTPCookiePath,
            ],
            &[&name, &value, &domain, &path],
        )
    };
    // Foundation treats even the string "FALSE" as a present Secure/HttpOnly
    // attribute. Omit false flags instead of accidentally upgrading every cookie.
    if cookie.secure {
        unsafe {
            properties.insert(NSHTTPCookieSecure, ns_string!("TRUE"));
        }
    }
    if cookie.http_only {
        properties.insert(ns_string!("HttpOnly"), ns_string!("TRUE"));
    }
    if let Some(expiry) = cookie.expires {
        let date = NSDate::dateWithTimeIntervalSince1970(expiry as f64);
        unsafe {
            properties.insert(NSHTTPCookieExpires, &date);
        }
    }
    if let Some(site) = &cookie.same_site {
        properties.insert(
            ns_string!("SameSite"),
            &NSString::from_str(&site.to_lowercase()),
        );
    }
    unsafe { NSHTTPCookie::cookieWithProperties(&properties) }
        .ok_or_else(|| "网站 Cookie 属性无效，无法恢复登录".into())
}

/// Read actual WebKit domains without widening or narrowing their leading-dot scope.
pub(crate) fn snapshot(view: &tauri::Webview) -> Result<Vec<SavedCookie>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    view.with_webview(move |platform| {
        let block =
            block2::RcBlock::new(move |cookies: std::ptr::NonNull<NSArray<NSHTTPCookie>>| {
                // WebKit owns the array during its completion callback.
                let records = unsafe { cookies.as_ref() }
                    .to_vec()
                    .iter()
                    .map(|cookie| saved(cookie))
                    .collect::<Vec<_>>();
                let _ = tx.send(records);
            });
        unsafe {
            (&*platform.inner().cast::<WKWebView>())
                .configuration()
                .websiteDataStore()
                .httpCookieStore()
                .getAllCookies(&block);
        }
    })
    .map_err(|_| "无法读取网站 Cookie")?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| "读取网站 Cookie 超时".into())
}
/// Install one batch and wait for every native completion before navigating.
pub(crate) fn restore(view: &tauri::Webview, cookies: Vec<SavedCookie>) -> Result<(), String> {
    if cookies.is_empty() {
        return Ok(());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let count = cookies.len();
    view.with_webview(move |platform| {
        for cookie in &cookies {
            let cookie = match native(cookie) {
                Ok(cookie) => cookie,
                Err(error) => {
                    let _ = tx.send(Err(error));
                    continue;
                }
            };
            let tx = tx.clone();
            let block = block2::RcBlock::new(move || {
                let _ = tx.send(Ok(()));
            });
            unsafe {
                (&*platform.inner().cast::<WKWebView>())
                    .configuration()
                    .websiteDataStore()
                    .httpCookieStore()
                    .setCookie_completionHandler(&cookie, Some(&block));
            }
        }
    })
    .map_err(|_| "无法恢复网站 Cookie")?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    for _ in 0..count {
        rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|_| "恢复网站 Cookie 超时")??;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn webkit_round_trip_preserves_cookie_scope_and_flags() {
        for (domain, flag) in [
            ("example.org", false),
            ("example.org", true),
            (".example.org", false),
            (".example.org", true),
        ] {
            let original = SavedCookie {
                name: "login".into(),
                value: "fake-token".into(),
                domain: domain.into(),
                path: "/account".into(),
                secure: flag,
                http_only: flag,
                same_site: Some("Lax".into()),
                // WebKit caps far-future cookies; use a near-future expiry for this native fixture.
                expires: Some((crate::browser_profile::now() / 1000) as i64 + 86_400),
                remember_until: 0,
            };
            let restored = saved(&native(&original).unwrap());
            assert_eq!(restored.domain, domain);
            assert_eq!(restored.value, "fake-token");
            assert_eq!(restored.path, "/account");
            assert_eq!(restored.secure, flag);
            assert_eq!(restored.http_only, flag);
            assert_eq!(restored.same_site.as_deref(), Some("Lax"));
            assert_eq!(restored.expires, original.expires);
        }
    }
}
