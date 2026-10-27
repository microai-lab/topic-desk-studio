//! Native media polling for every reading tab, including hidden/background views.
use crate::{
    browser_profile::{self, BrowserProfile, BrowserStatus},
    desktop_sessions::BrowserLoginSessions,
};
use serde::Deserialize;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tauri::Manager;

/// Terminate playback before cookie capture or native removal. Wry retains closed
/// WKWebViews on macOS, so removing a view alone does not stop its media process.
pub(crate) fn stop(view: &tauri::Webview) {
    let _ = view.eval("globalThis.__topicDeskMedia?.shutdown()");
    #[cfg(target_os = "macos")]
    let _ = view.with_webview(|platform| {
        use objc2_foundation::NSObjectProtocol;
        use objc2_web_kit::WKWebView;
        // Tauri runs this callback on the UI thread with a live native view.
        let native = unsafe { &*platform.inner().cast::<WKWebView>() };
        unsafe { native.stopLoading() };
        // Public WebKit suspension covers cross-origin frames and prevents a
        // retained player from resuming itself after its tab has disappeared.
        if native.respondsToSelector(objc2::sel!(setAllMediaPlaybackSuspended:completionHandler:)) {
            unsafe { native.setAllMediaPlaybackSuspended_completionHandler(true, None) };
        }
        if native.respondsToSelector(objc2::sel!(closeAllMediaPresentationsWithCompletionHandler:))
        {
            unsafe { native.closeAllMediaPresentationsWithCompletionHandler(None) };
        }
    });
}

/// Only a bounded boolean and matching navigation identity cross from remote pages.
#[derive(Deserialize)]
struct MediaSnapshot {
    url: String,
    playing: bool,
}

/// Bound remote output before deserialization; no page can send arbitrary metadata.
fn decode(raw: &str) -> Option<MediaSnapshot> {
    (raw.len() <= 8192)
        .then(|| serde_json::from_str(raw).ok())
        .flatten()
}

/// Drop stale callbacks after navigation or closure; publish only actual state changes.
fn update(
    status: &mut BrowserStatus,
    generation: u64,
    expected: u64,
    snapshot: &MediaSnapshot,
) -> bool {
    if generation != expected
        || status.url != snapshot.url
        || status.media_playing == snapshot.playing
    {
        return false;
    }
    status.media_playing = snapshot.playing;
    true
}

/// One native scheduler bounds work independently of page visibility or JavaScript timers.
pub(crate) fn install(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        if app
            .state::<BrowserLoginSessions>()
            .exiting
            .load(Ordering::Acquire)
        {
            break;
        }
        let requests = app
            .state::<BrowserProfile>()
            .inner
            .lock()
            .map(|session| {
                session
                    .tabs
                    .iter()
                    .map(|(id, tab)| (id.clone(), tab.load_generation, tab.status.muted))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut pending = 0;
        for (id, generation, muted) in requests {
            let Some(view) = app.get_webview(&format!("article-{id}")) else {
                continue;
            };
            let sender = sender.clone();
            let script = format!("(() => {{ const s = globalThis.__topicDeskMedia; if (!s) return null; s.setMuted({muted}); return s.sample(); }})()");
            if view
                .eval_with_callback(script, move |raw| {
                    let _ = sender.send((id.clone(), generation, raw));
                })
                .is_ok()
            {
                pending += 1;
            }
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        for _ in 0..pending {
            let Ok((id, generation, raw)) =
                receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            else {
                break;
            };
            let Some(snapshot) = decode(&raw) else {
                continue;
            };
            let changed =
                app.state::<BrowserProfile>()
                    .inner
                    .lock()
                    .ok()
                    .and_then(|mut session| {
                        let tab = session.tabs.get_mut(&id)?;
                        update(&mut tab.status, tab.load_generation, generation, &snapshot)
                            .then(|| tab.status.clone())
                    });
            if let Some(status) = changed {
                browser_profile::publish(&app, &status);
            }
        }
        std::thread::sleep(Duration::from_millis(750));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_updates_require_current_navigation_and_publish_transitions_only() {
        let mut status = BrowserStatus {
            url: "https://example.com/article".into(),
            ..Default::default()
        };
        let mut snapshot = MediaSnapshot {
            url: status.url.clone(),
            playing: true,
        };
        assert!(!update(&mut status, 2, 1, &snapshot));
        snapshot.url = "https://other.example/".into();
        assert!(!update(&mut status, 2, 2, &snapshot));
        snapshot.url = status.url.clone();
        assert!(update(&mut status, 2, 2, &snapshot));
        assert!(!update(&mut status, 2, 2, &snapshot));
        snapshot.playing = false;
        assert!(update(&mut status, 2, 2, &snapshot));
        assert!(!status.media_playing);
    }
    #[test]
    fn malformed_page_reports_are_rejected() {
        assert!(decode(&"x".repeat(8193)).is_none());
        for raw in ["null", "{}", r#"{"url":"x","playing":"yes"}"#] {
            assert!(decode(raw).is_none());
        }
    }
}
