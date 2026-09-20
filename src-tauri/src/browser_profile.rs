//! Browser-owned SQLite metadata and OS-vault credentials, isolated from topic identity.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, EventTarget, Manager};

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
}

/// Persisted browser preferences are validated before affecting navigation or storage.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSettings {
    pub search_engine: String,
    pub zoom: f64,
    pub remember_history: bool,
}
impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            search_engine: "bing".into(),
            zoom: 1.0,
            remember_history: true,
        }
    }
}

/// A browser history, download or credential metadata row. Secrets never leave Rust.
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
    pub passwords: Vec<BrowserRecord>,
    pub settings: BrowserSettings,
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
}

/// Use one browser database in the application data directory, never in the repository.
pub fn open(path: PathBuf) -> Result<BrowserProfile, String> {
    let database = Connection::open(path).map_err(|e| e.to_string())?;
    database.execute_batch("CREATE TABLE IF NOT EXISTS records (id INTEGER PRIMARY KEY, kind TEXT NOT NULL, url TEXT NOT NULL, title TEXT NOT NULL, detail TEXT NOT NULL, time INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS preferences (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL);").map_err(|e| e.to_string())?;
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
pub fn navigated(app: &tauri::AppHandle, tab_id: &str, url: &str) {
    let profile = app.state::<BrowserProfile>();
    let Ok(mut session) = profile.inner.lock() else {
        return;
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
    tab.status.can_back = tab.index > 0;
    tab.status.can_forward = tab.index + 1 < tab.trail.len();
    publish(app, &tab.status);
}

/// Persist a successful page visit, limiting retained history to 2,000 entries.
pub fn page_finished(app: &tauri::AppHandle, tab_id: &str, url: &str) {
    let profile = app.state::<BrowserProfile>();
    let Ok(mut session) = profile.inner.lock() else {
        return;
    };
    let remember_history = session.settings.remember_history;
    let Some(tab) = session.tabs.get_mut(tab_id) else {
        return;
    };
    if tab.status.url != url {
        return;
    }
    tab.status.loading = false;
    let status = tab.status.clone();
    if remember_history {
        let title = status.title.clone();
        // Update the visit and prune in one transaction so interruption cannot
        // leave partially updated history or an unbounded collection.
        if let Ok(tx) = session.database.transaction() {
            let result = tx.execute("DELETE FROM records WHERE kind='history' AND url=?1", [url])
                .and_then(|_| tx.execute("INSERT INTO records(kind,url,title,detail,time) VALUES('history',?1,?2,'',?3)", params![url, title, now()]))
                .and_then(|_| tx.execute("DELETE FROM records WHERE kind='history' AND id NOT IN (SELECT id FROM records WHERE kind='history' ORDER BY time DESC LIMIT 2000)", []));
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

/// Return browser metadata; password values stay in the system credential vault.
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
        passwords: read("password")?,
        settings: session.settings.clone(),
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
    let _ = app.emit_to(EventTarget::webview("main"), "browser-library-changed", ());
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
