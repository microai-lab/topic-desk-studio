//! Browser-owned history, download metadata and preferences, isolated from topic identity.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, EventTarget, Manager};

const BROWSER_SCHEMA_VERSION: i64 = 1;
const RECORD_LIMIT: i64 = 2_000;

/// Trusted browser chrome state; remote titles and URLs are displayed only as text.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub muted: bool,
}

/// Persisted browser preferences are validated before affecting navigation or storage.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSettings {
    pub search_engine: String,
    pub zoom: f64,
    pub remember_history: bool,
    #[serde(default = "default_translation_language")]
    pub translation_language: String,
}

fn default_translation_language() -> String {
    "auto".into()
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            search_engine: "bing".into(),
            zoom: 1.0,
            remember_history: true,
            translation_language: default_translation_language(),
        }
    }
}

/// A browser history or download metadata row.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecord {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub detail: String,
    pub time: u64,
}

/// One atomic view of the local browser library and preferences.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLibrary {
    pub history: Vec<BrowserRecord>,
    pub downloads: Vec<BrowserRecord>,
    pub settings: BrowserSettings,
    /// Only platforms with a native cancellable download handle expose the control.
    pub download_cancellation: bool,
}

/// Runtime state and browser metadata share short mutex-protected transactions.
pub struct BrowserProfile {
    pub inner: Mutex<BrowserSession>,
}

/// Native navigation history is scoped to the current embedded view, not the library.
pub struct BrowserSession {
    pub database: Connection,
    pub settings: BrowserSettings,
    pub tabs: HashMap<String, BrowserTabSession>,
    pub active_tab: Option<String>,
}

/// Navigation state belongs to a tab even while its native view is hidden.
#[derive(Default)]
pub struct BrowserTabSession {
    pub status: BrowserStatus,
    pub trail: Vec<String>,
    pub index: usize,
    pub traversal: bool,
    /// Discard readiness probes from older navigations, including same-URL reloads.
    pub load_generation: u64,
    pub document_origin: Option<f64>,
}

impl BrowserTabSession {
    /// Idempotent completion also remembers the document identity for reload probes.
    fn complete_page(&mut self, url: &str, generation: Option<u64>, origin: Option<f64>) -> bool {
        if self.status.url != url || generation.is_some_and(|value| value != self.load_generation) {
            return false;
        }
        if let Some(origin) = origin {
            self.document_origin = Some(origin);
        }
        std::mem::replace(&mut self.status.loading, false)
    }
}

/// Use one browser database in the application data directory, never in the repository.
pub fn open(path: PathBuf) -> Result<BrowserProfile, String> {
    let database = Connection::open(path).map_err(|e| e.to_string())?;
    database
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    database
        .pragma_update(None, "busy_timeout", 5_000_i64)
        .map_err(|e| e.to_string())?;
    database
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| e.to_string())?;
    let integrity: String = database
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        return Err("浏览器数据库完整性检查失败".into());
    }
    let version: i64 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    match version {
        0 => database.execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE IF NOT EXISTS records (
               id INTEGER PRIMARY KEY,
               kind TEXT NOT NULL CHECK(kind IN ('history', 'download')),
               url TEXT NOT NULL,
               title TEXT NOT NULL,
               detail TEXT NOT NULL,
               time INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS preferences (
               id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_browser_records_kind_time
               ON records(kind, time DESC, id DESC);
             PRAGMA user_version = 1;
             COMMIT;",
        ),
        BROWSER_SCHEMA_VERSION => Ok(()),
        unsupported => Err(rusqlite::Error::InvalidParameterName(format!(
            "不支持的浏览器数据库版本：{unsupported}"
        ))),
    }
    .map_err(|e| e.to_string())?;
    // A process exit interrupts native downloads without a final callback.
    // Convert those stale rows once so the UI never shows an endless transfer.
    database
        .execute(
            "UPDATE records
             SET detail='failed' || substr(detail, length('downloading') + 1)
             WHERE kind='download' AND detail LIKE 'downloading\n%'",
            [],
        )
        .map_err(|e| e.to_string())?;
    prune_records(&database).map_err(|e| e.to_string())?;
    let settings = database
        .query_row("SELECT json FROM preferences WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .and_then(|value| serde_json::from_str::<BrowserSettings>(&value).ok())
        .unwrap_or_default();
    Ok(BrowserProfile {
        inner: Mutex::new(BrowserSession {
            database,
            settings,
            tabs: HashMap::new(),
            active_tab: None,
        }),
    })
}

/// Keep both metadata collections bounded independently of their UI page size.
pub(crate) fn prune_records(database: &Connection) -> rusqlite::Result<()> {
    for kind in ["history", "download"] {
        database.execute(
            "DELETE FROM records WHERE kind = ?1 AND id NOT IN (
               SELECT id FROM records WHERE kind = ?1 ORDER BY time DESC, id DESC LIMIT ?2
             )",
            params![kind, RECORD_LIMIT],
        )?;
    }
    Ok(())
}

/// Milliseconds are used only for browser ordering and unique download names.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Emit only to the privileged main view; external pages receive no profile data.
pub fn publish(app: &tauri::AppHandle, status: &BrowserStatus) {
    let _ = app.emit_to(EventTarget::webview("main"), "browser-status", status);
}

/// Reflect top-level native navigation, including redirects and links clicked in-page.
pub fn navigated(app: &tauri::AppHandle, tab_id: &str, url: &str) -> Option<(u64, Option<f64>)> {
    let profile = app.state::<BrowserProfile>();
    let Ok(mut session) = profile.inner.lock() else {
        return None;
    };
    let tab = session.tabs.entry(tab_id.into()).or_default();
    tab.status.tab_id = tab_id.into();
    if tab.status.url != url {
        if tab.traversal {
            if let Some(index) = tab.trail.iter().position(|item| item == url) {
                tab.index = index;
            }
            tab.traversal = false;
        } else {
            let keep = tab.index + 1;
            tab.trail.truncate(keep);
            tab.trail.push(url.into());
            tab.index = tab.trail.len() - 1;
        }
    }
    tab.status.url = url.into();
    tab.status.title.clear();
    tab.status.loading = true;
    tab.load_generation = tab.load_generation.wrapping_add(1);
    tab.status.can_back = tab.index > 0;
    tab.status.can_forward = tab.index + 1 < tab.trail.len();
    publish(app, &tab.status);
    Some((tab.load_generation, tab.document_origin))
}

/// Persist a successful page visit, limiting retained history to 2,000 entries.
pub fn page_finished(app: &tauri::AppHandle, tab_id: &str, url: &str) {
    finish_page(app, tab_id, url, None, None);
}

/// Main document readiness may finish the chrome before slow subresources.
pub fn page_ready(app: &tauri::AppHandle, tab_id: &str, url: &str, generation: u64, origin: f64) {
    finish_page(app, tab_id, url, Some(generation), Some(origin));
}

/// Complete each navigation once; stale callbacks must not affect a new load.
fn finish_page(
    app: &tauri::AppHandle,
    tab_id: &str,
    url: &str,
    generation: Option<u64>,
    origin: Option<f64>,
) {
    let profile = app.state::<BrowserProfile>();
    let Ok(mut session) = profile.inner.lock() else {
        return;
    };
    let remember_history = session.settings.remember_history;
    let Some(tab) = session.tabs.get_mut(tab_id) else {
        return;
    };
    if !tab.complete_page(url, generation, origin) {
        return;
    }
    let status = tab.status.clone();
    if remember_history {
        let title = status.title.clone();
        // Update the visit and prune in one transaction so interruption cannot
        // leave partially updated history or an unbounded collection.
        if let Ok(tx) = session.database.transaction() {
            let result = tx.execute("DELETE FROM records WHERE kind='history' AND url=?1", [url])
                .and_then(|_| tx.execute("INSERT INTO records(kind,url,title,detail,time) VALUES('history',?1,?2,'',?3)", params![url, title, now()]))
                .and_then(|_| tx.execute("DELETE FROM records WHERE kind='history' AND id NOT IN (SELECT id FROM records WHERE kind='history' ORDER BY time DESC, id DESC LIMIT ?1)", [RECORD_LIMIT]));
            if result.is_ok() {
                let _ = tx.commit();
            }
        }
    }
    publish(app, &status);
}

/// Keep the address bar title synchronized without trusting page-provided markup.
pub fn title_changed(app: &tauri::AppHandle, tab_id: &str, title: String) {
    let profile = app.state::<BrowserProfile>();
    let Ok(mut session) = profile.inner.lock() else {
        return;
    };
    let Some(tab) = session.tabs.get_mut(tab_id) else {
        return;
    };
    tab.status.title = title.chars().take(500).collect();
    let status = tab.status.clone();
    let _ = session.database.execute(
        "UPDATE records SET title=?1 WHERE kind='history' AND url=?2",
        params![status.title, status.url],
    );
    publish(app, &status);
}

/// Return browser history and download metadata without any credential-storage surface.
pub fn library(app: &tauri::AppHandle) -> Result<BrowserLibrary, String> {
    let profile = app.state::<BrowserProfile>();
    let session = profile.inner.lock().map_err(|e| e.to_string())?;
    let read = |kind: &str| -> Result<Vec<BrowserRecord>, String> {
        let mut statement = session.database.prepare("SELECT id,url,title,detail,time FROM records WHERE kind=?1 ORDER BY time DESC LIMIT 2000").map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([kind], |r| {
                Ok(BrowserRecord {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    detail: r.get(3)?,
                    time: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    };
    Ok(BrowserLibrary {
        history: read("history")?,
        downloads: read("download")?,
        settings: session.settings.clone(),
        download_cancellation: cfg!(target_os = "macos"),
    })
}

/// Validate both explicit URLs and omnibox search terms in the Rust boundary.
pub fn resolve_address(input: &str, engine: &str) -> Result<url::Url, String> {
    let input = input.trim();
    if input.is_empty() || input.len() > 8192 {
        return Err("请输入网址或搜索关键词".into());
    }
    let candidate = if input.contains("://") {
        input.into()
    } else {
        format!("https://{input}")
    };
    if !input.contains(char::is_whitespace)
        && (input.contains('.') || input.contains("://") || input.starts_with("localhost"))
    {
        let url = url::Url::parse(&candidate).map_err(|_| "网址格式无效")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("仅支持不含凭据的 HTTP(S) 网址".into());
        }
        return Ok(url);
    }
    if ["javascript:", "data:", "file:", "tauri:"]
        .iter()
        .any(|p| input.to_ascii_lowercase().starts_with(p))
    {
        return Err("不支持此网址协议".into());
    }
    let base = match engine {
        "google" => "https://www.google.com/search",
        "duckduckgo" => "https://duckduckgo.com/",
        _ => "https://www.bing.com/search",
    };
    let mut url = url::Url::parse(base).map_err(|e| e.to_string())?;
    url.query_pairs_mut().append_pair("q", input);
    Ok(url)
}

/// Record downloads by native-assigned path; callers cannot supply arbitrary file paths.
pub fn record_download(app: &tauri::AppHandle, url: &str, path: &std::path::Path, detail: &str) {
    let profile = app.state::<BrowserProfile>();
    let Ok(session) = profile.inner.lock() else {
        return;
    };
    let title = path.file_name().unwrap_or_default().to_string_lossy();
    let _ = session.database.execute(
        "INSERT INTO records(kind,url,title,detail,time) VALUES('download',?1,?2,?3,?4)",
        params![url, title, format!("{detail}\n{}", path.display()), now()],
    );
    let _ = prune_records(&session.database);
    let _ = app.emit_to(EventTarget::webview("main"), "browser-library-changed", ());
}

/// Insert a native download before transfer starts so browser chrome can show it immediately.
pub fn start_download(
    app: &tauri::AppHandle,
    url: &str,
    path: &std::path::Path,
) -> Result<i64, String> {
    let profile = app.state::<BrowserProfile>();
    let session = profile.inner.lock().map_err(|e| e.to_string())?;
    let title = path.file_name().unwrap_or_default().to_string_lossy();
    session
        .database
        .execute(
            "INSERT INTO records(kind,url,title,detail,time) VALUES('download',?1,?2,?3,?4)",
            params![
                url,
                title,
                format!("downloading\n{}", path.display()),
                now()
            ],
        )
        .map_err(|e| e.to_string())?;
    let id = session.database.last_insert_rowid();
    let _ = prune_records(&session.database);
    let _ = app.emit_to(EventTarget::webview("main"), "browser-library-changed", ());
    crate::desktop::bind_current_native_download(id);
    Ok(id)
}

/// Finish the exact row created by `start_download`, preserving its trusted native path.
pub fn finish_download(app: &tauri::AppHandle, id: i64, path: &std::path::Path, success: bool) {
    let cancelled = crate::desktop::download_was_cancelled(id);
    let profile = app.state::<BrowserProfile>();
    let Ok(session) = profile.inner.lock() else {
        return;
    };
    let state = if success {
        "complete"
    } else if cancelled {
        "cancelled"
    } else {
        "failed"
    };
    let _ = session.database.execute(
        "UPDATE records SET detail=?1 WHERE kind='download' AND id=?2",
        params![format!("{state}\n{}", path.display()), id],
    );
    if cancelled {
        let _ = std::fs::remove_file(path);
    }
    let _ = app.emit_to(EventTarget::webview("main"), "browser-library-changed", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_ready_completes_once_and_stale_reload_probes_are_ignored() {
        let mut tab = BrowserTabSession {
            status: BrowserStatus {
                url: "https://example.com/article".into(),
                loading: true,
                title: "Article".into(),
                ..Default::default()
            },
            load_generation: 2,
            ..Default::default()
        };
        assert!(!tab.complete_page("https://example.com/article", Some(1), Some(100.0)));
        assert!(tab.status.loading);
        assert!(!tab.complete_page("https://example.com/old", Some(2), Some(100.0)));
        assert!(tab.complete_page("https://example.com/article", Some(2), Some(200.0)));
        assert!(!tab.status.loading);
        assert_eq!(tab.document_origin, Some(200.0));
        assert_eq!(tab.status.title, "Article");
        // A later all-resources-finished callback neither restarts loading nor records another visit.
        assert!(!tab.complete_page("https://example.com/article", None, None));
    }

    #[test]
    fn legacy_browser_settings_default_to_automatic_page_translation() {
        let settings: BrowserSettings =
            serde_json::from_str(r#"{"searchEngine":"bing","zoom":1.0,"rememberHistory":true}"#)
                .expect("legacy browser settings should remain readable");
        assert_eq!(settings.translation_language, "auto");
    }

    #[test]
    fn omnibox_preserves_urls_encodes_search_and_rejects_active_schemes() {
        assert_eq!(
            resolve_address("example.com/a?q=1", "bing")
                .unwrap()
                .as_str(),
            "https://example.com/a?q=1"
        );
        assert!(resolve_address("Rust & Tauri 中文", "google")
            .unwrap()
            .as_str()
            .contains("q=Rust+%26+Tauri"));
        for value in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://user:secret@example.com",
        ] {
            assert!(resolve_address(value, "bing").is_err());
        }
    }

    #[test]
    fn pruning_bounds_history_and_downloads_independently() {
        let database = Connection::open_in_memory().expect("in-memory database should open");
        database
            .execute_batch(
                "CREATE TABLE records (
                   id INTEGER PRIMARY KEY,
                   kind TEXT NOT NULL,
                   url TEXT NOT NULL,
                   title TEXT NOT NULL,
                   detail TEXT NOT NULL,
                   time INTEGER NOT NULL
                 );",
            )
            .expect("test schema should be valid");
        for index in 0..=RECORD_LIMIT {
            for kind in ["history", "download"] {
                database
                    .execute(
                        "INSERT INTO records(kind,url,title,detail,time) VALUES(?1,?2,'','',?3)",
                        params![kind, format!("https://example.com/{kind}/{index}"), index],
                    )
                    .expect("test record should insert");
            }
        }

        prune_records(&database).expect("retention should succeed");
        for kind in ["history", "download"] {
            let count: i64 = database
                .query_row(
                    "SELECT count(*) FROM records WHERE kind=?1",
                    [kind],
                    |row| row.get(0),
                )
                .expect("count should be readable");
            assert_eq!(count, RECORD_LIMIT);
            let oldest: i64 = database
                .query_row(
                    "SELECT min(time) FROM records WHERE kind=?1",
                    [kind],
                    |row| row.get(0),
                )
                .expect("oldest retained row should exist");
            assert_eq!(oldest, 1);
        }
    }
}
