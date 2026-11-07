//! Backup, restore, health reporting and bounded maintenance for local application data.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, MAIN_DB};

use crate::credential_cipher::MASTER_KEY_FILE;
use crate::error::{AppError, AppResult};
use crate::models::{StorageOperationResult, StorageStatus};

const BACKUP_LIMIT: usize = 3;
const TOPIC_DATABASE: &str = "topic-desk.sqlite";
const BROWSER_DATABASE: &str = "browser.sqlite";

/// Cookie stores and filesystem backups cannot join SQLite's transaction. Run
/// selected groups independently and explicitly report any partial completion.
pub(crate) fn clear_selected(
    selection: crate::models::ClearDataSelection,
    records: impl FnOnce() -> Result<(), String>,
    logins: impl FnOnce() -> Result<(), String>,
    backups: impl FnOnce() -> Result<(), String>,
) -> Result<StorageOperationResult, String> {
    selection.validate().map_err(|error| error.to_string())?;
    let results = [
        (
            "所选记录",
            (selection.collected_data || selection.browser_history || selection.download_records)
                .then(records),
        ),
        ("网站登录数据", selection.website_logins.then(logins)),
        ("本地备份", selection.local_backups.then(backups)),
    ];
    let mut completed = Vec::new();
    let mut failed = Vec::new();
    for (name, result) in results {
        match result {
            Some(Ok(())) => completed.push(name),
            Some(Err(error)) => failed.push(format!("{name}：{error}")),
            None => {}
        }
    }
    if !failed.is_empty() {
        return Err(format!(
            "清除未全部完成。已完成：{}；未完成：{}",
            if completed.is_empty() {
                "无".into()
            } else {
                completed.join("、")
            },
            failed.join("；")
        ));
    }
    Ok(StorageOperationResult {
        message: "所选本地数据已清除".into(),
        backup_name: None,
    })
}

/// Match only backup names produced by this application, including migration
/// snapshots and their copied keys; unrelated files in the directory are retained.
fn managed_backup(name: &str, directory: bool) -> bool {
    let numeric = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if directory {
        return name.strip_prefix("backup-").is_some_and(numeric);
    }
    name.strip_prefix("topic-desk-pre-v")
        .and_then(|value| value.strip_suffix(".sqlite"))
        .is_some_and(numeric)
        || name
            .strip_prefix("model-credential-pre-v")
            .and_then(|value| value.strip_suffix(".key"))
            .is_some_and(numeric)
}

/// Delete application-owned backups only. Never follow a symlinked backup root
/// or entries, and never touch live databases, cookie vaults or live master keys.
pub fn clear_backups(data_directory: &Path) -> AppResult<()> {
    let root = data_directory.join("backups");
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "备份目录不是普通目录，未执行删除".into(),
        ));
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if !kind.is_symlink()
            && (kind.is_dir() || kind.is_file())
            && managed_backup(&entry.file_name().to_string_lossy(), kind.is_dir())
        {
            candidates.push((entry.path(), kind.is_dir()));
        }
    }
    for (path, directory) in candidates {
        if directory {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Summarize disk use and health without returning any stored content to the WebView.
pub fn status(
    data_directory: &Path,
    topic: &Connection,
    browser: &Connection,
) -> AppResult<StorageStatus> {
    Ok(StorageStatus {
        data_directory: data_directory.to_string_lossy().into_owned(),
        topic_database_bytes: sqlite_size(&data_directory.join(TOPIC_DATABASE)),
        browser_database_bytes: sqlite_size(&data_directory.join(BROWSER_DATABASE)),
        topic_count: topic.query_row("SELECT COUNT(*) FROM topic", [], |row| row.get(0))?,
        observation_count: topic.query_row(
            "SELECT (SELECT COUNT(*) FROM topic_observation)
                  + (SELECT COUNT(*) FROM topic_observation_hourly)
                  + (SELECT COUNT(*) FROM topic_observation_daily)",
            [],
            |row| row.get(0),
        )?,
        collection_run_count: topic.query_row(
            "SELECT COUNT(*) FROM collection_run",
            [],
            |row| row.get(0),
        )?,
        browser_record_count: browser
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?,
        integrity_ok: crate::database::integrity_ok(topic)? && quick_check(browser)?,
        latest_backup: latest_backup(data_directory).map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        }),
    })
}

/// Create a consistent two-database snapshot and retain only the newest three user backups.
pub fn backup(
    data_directory: &Path,
    topic: &Connection,
    browser: &Connection,
) -> AppResult<StorageOperationResult> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = format!("backup-{seconds}");
    let directory = data_directory.join("backups").join(&name);
    fs::create_dir_all(&directory)?;
    topic.backup(MAIN_DB, directory.join(TOPIC_DATABASE), None)?;
    browser.backup(MAIN_DB, directory.join(BROWSER_DATABASE), None)?;
    let key = data_directory.join(MASTER_KEY_FILE);
    if key.is_file() {
        fs::copy(key, directory.join(MASTER_KEY_FILE))?;
    }
    fs::write(
        directory.join("manifest.json"),
        format!("{{\"formatVersion\":1,\"createdAtUnix\":{seconds}}}"),
    )?;
    rotate_backups(data_directory)?;
    Ok(StorageOperationResult {
        message: "本地数据备份已创建".into(),
        backup_name: Some(name),
    })
}

/// Restore the newest application-created snapshot after validating both SQLite files.
pub fn restore_latest(
    data_directory: &Path,
    topic: &mut Connection,
    browser: &mut Connection,
) -> AppResult<StorageOperationResult> {
    let directory = latest_backup(data_directory)
        .ok_or_else(|| AppError::InvalidInput("没有可恢复的本地备份".into()))?;
    let topic_source = directory.join(TOPIC_DATABASE);
    let browser_source = directory.join(BROWSER_DATABASE);
    let source_topic =
        Connection::open_with_flags(&topic_source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let source_browser =
        Connection::open_with_flags(&browser_source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if !crate::database::integrity_ok(&source_topic)? || !quick_check(&source_browser)? {
        return Err(AppError::Initialization("备份数据库完整性检查失败".into()));
    }
    let credential_count: i64 =
        source_topic.query_row("SELECT COUNT(*) FROM model_credential", [], |row| {
            row.get(0)
        })?;
    let backup_key = directory.join(MASTER_KEY_FILE);
    if credential_count > 0 && !backup_key.is_file() {
        return Err(AppError::Credential("备份缺少模型凭据主密钥".into()));
    }
    drop(source_topic);
    drop(source_browser);

    // The two databases cannot share one SQLite transaction, so keep a local
    // rollback pair until both restores and the credential-key swap succeed.
    let rollback_topic = data_directory.join(".restore-topic.rollback.sqlite");
    let rollback_browser = data_directory.join(".restore-browser.rollback.sqlite");
    let rollback_key = data_directory.join(".restore-key.rollback");
    topic.backup(MAIN_DB, &rollback_topic, None)?;
    browser.backup(MAIN_DB, &rollback_browser, None)?;
    let live_key = data_directory.join(MASTER_KEY_FILE);
    let had_live_key = live_key.is_file();
    if had_live_key {
        fs::copy(&live_key, &rollback_key)?;
    }
    let restored = (|| -> AppResult<()> {
        topic.restore(
            MAIN_DB,
            &topic_source,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        browser.restore(
            MAIN_DB,
            &browser_source,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        if backup_key.is_file() {
            fs::copy(&backup_key, &live_key)?;
        } else if live_key.is_file() {
            fs::remove_file(&live_key)?;
        }
        // Older backups must gain current reading-link support before queries
        // resume; a failed migration still rolls both databases and the key back.
        crate::database::upgrade_restored(topic, &live_key)?;
        Ok(())
    })();
    if let Err(error) = restored {
        let _ = topic.restore(
            MAIN_DB,
            &rollback_topic,
            None::<fn(rusqlite::backup::Progress)>,
        );
        let _ = browser.restore(
            MAIN_DB,
            &rollback_browser,
            None::<fn(rusqlite::backup::Progress)>,
        );
        if had_live_key {
            let _ = fs::copy(&rollback_key, &live_key);
        } else if live_key.is_file() {
            let _ = fs::remove_file(&live_key);
        }
        let _ = fs::remove_file(&rollback_topic);
        let _ = fs::remove_file(&rollback_browser);
        let _ = fs::remove_file(&rollback_key);
        return Err(error);
    }
    fs::remove_file(rollback_topic)?;
    fs::remove_file(rollback_browser)?;
    if rollback_key.is_file() {
        fs::remove_file(rollback_key)?;
    }
    let name = directory
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(StorageOperationResult {
        message: "已从最近备份恢复本地数据".into(),
        backup_name: Some(name),
    })
}

/// Include SQLite sidecars while reporting the space occupied by one logical database.
fn sqlite_size(path: &Path) -> u64 {
    [
        path.to_path_buf(),
        suffix(path, "-wal"),
        suffix(path, "-shm"),
    ]
    .iter()
    .filter_map(|candidate| fs::metadata(candidate).ok())
    .map(|metadata| metadata.len())
    .sum()
}

fn suffix(path: &Path, value: &str) -> PathBuf {
    PathBuf::from(format!("{}{value}", path.to_string_lossy()))
}

fn quick_check(connection: &Connection) -> AppResult<bool> {
    let result: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    Ok(result == "ok")
}

fn backup_directories(data_directory: &Path) -> AppResult<Vec<PathBuf>> {
    let root = data_directory.join("backups");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            fs::symlink_metadata(path)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                && path
                    .file_name()
                    .is_some_and(|name| managed_backup(&name.to_string_lossy(), true))
        })
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries)
}

fn latest_backup(data_directory: &Path) -> Option<PathBuf> {
    backup_directories(data_directory).ok()?.pop()
}

fn rotate_backups(data_directory: &Path) -> AppResult<()> {
    let entries = backup_directories(data_directory)?;
    let remove_count = entries.len().saturating_sub(BACKUP_LIMIT);
    for path in entries.into_iter().take(remove_count) {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_groups_respect_all_selections_and_report_partial_failures() {
        use std::cell::Cell;
        for mask in 0..32 {
            let selected = crate::models::ClearDataSelection {
                collected_data: mask & 1 != 0,
                browser_history: mask & 2 != 0,
                download_records: mask & 4 != 0,
                website_logins: mask & 8 != 0,
                local_backups: mask & 16 != 0,
            };
            let records = Cell::new(0);
            let logins = Cell::new(0);
            let backups = Cell::new(0);
            let result = clear_selected(
                selected,
                || {
                    records.set(records.get() + 1);
                    Ok(())
                },
                || {
                    logins.set(logins.get() + 1);
                    Ok(())
                },
                || {
                    backups.set(backups.get() + 1);
                    Ok(())
                },
            );
            assert_eq!(result.is_ok(), mask != 0);
            assert_eq!(records.get(), i32::from(mask & 7 != 0));
            assert_eq!(logins.get(), i32::from(selected.website_logins));
            assert_eq!(backups.get(), i32::from(selected.local_backups));
            let encoded = serde_json::to_string(&selected).unwrap();
            assert!(serde_json::from_str::<crate::models::ClearDataSelection>(&encoded).is_ok());
        }
        let finished = Cell::new(false);
        let error = clear_selected(
            crate::models::ClearDataSelection {
                browser_history: true,
                website_logins: true,
                local_backups: true,
                ..Default::default()
            },
            || Ok(()),
            || Err("fixture session failure".into()),
            || {
                finished.set(true);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(finished.get());
        assert!(error.contains("已完成：所选记录、本地备份"));
        assert!(error.contains("未完成：网站登录数据：fixture session failure"));
    }

    #[test]
    fn clearing_backups_keeps_live_data_and_unrelated_files() {
        let root = std::env::temp_dir().join(format!(
            "topic-desk-clear-backups-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        clear_backups(&root).unwrap();
        fs::create_dir_all(root.join("backups/backup-123")).unwrap();
        fs::write(
            root.join("backups/backup-123/topic-desk.sqlite"),
            b"snapshot",
        )
        .unwrap();
        for name in ["topic-desk-pre-v5.sqlite", "model-credential-pre-v5.key"] {
            fs::write(root.join("backups").join(name), b"snapshot").unwrap();
        }
        for name in [
            "topic-desk.sqlite",
            "browser.sqlite",
            MASTER_KEY_FILE,
            "browser-sessions.sqlite",
        ] {
            fs::write(root.join(name), b"live fixture").unwrap();
        }
        fs::create_dir_all(root.join("backups/backup-notes")).unwrap();
        fs::write(root.join("backups/notes.txt"), b"unrelated").unwrap();
        clear_backups(&root).unwrap();
        clear_backups(&root).unwrap();
        assert!(!root.join("backups/backup-123").exists());
        assert!(!root.join("backups/topic-desk-pre-v5.sqlite").exists());
        assert!(!root.join("backups/model-credential-pre-v5.key").exists());
        assert!(latest_backup(&root).is_none());
        for name in [
            "topic-desk.sqlite",
            "browser.sqlite",
            MASTER_KEY_FILE,
            "browser-sessions.sqlite",
        ] {
            assert_eq!(fs::read(root.join(name)).unwrap(), b"live fixture");
        }
        assert!(root.join("backups/backup-notes").is_dir());
        assert_eq!(
            fs::read(root.join("backups/notes.txt")).unwrap(),
            b"unrelated"
        );
        fs::remove_dir_all(&root).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("backups"), b"not a directory").unwrap();
        assert!(clear_backups(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn clearing_backups_never_follows_a_symlinked_root() {
        let root = std::env::temp_dir().join(format!(
            "topic-desk-clear-links-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("outside/backup-123")).unwrap();
        fs::write(root.join("outside/backup-123/data"), b"keep").unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("backups")).unwrap();
        assert!(clear_backups(&root).is_err());
        assert_eq!(
            fs::read(root.join("outside/backup-123/data")).unwrap(),
            b"keep"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_directory_order_uses_timestamp_names() {
        let root = std::env::temp_dir().join(format!(
            "topic-desk-storage-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("backups/backup-2")).unwrap();
        fs::create_dir_all(root.join("backups/backup-10")).unwrap();
        fs::create_dir_all(root.join("backups/ignored")).unwrap();
        let entries = backup_directories(&root).unwrap();
        assert_eq!(entries.len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_and_restore_round_trip_both_databases() {
        let root = std::env::temp_dir().join(format!(
            "topic-desk-backup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let mut topic = Connection::open(root.join(TOPIC_DATABASE)).unwrap();
        topic
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        topic
            .execute(
                "INSERT INTO app_setting(key, value) VALUES('test-value', 'before')",
                [],
            )
            .unwrap();
        let mut browser = Connection::open(root.join(BROWSER_DATABASE)).unwrap();
        browser
            .execute_batch(
                "CREATE TABLE records(id INTEGER PRIMARY KEY, kind TEXT, url TEXT, title TEXT, detail TEXT, time INTEGER);
                 CREATE TABLE preferences(id INTEGER PRIMARY KEY, json TEXT);
                 INSERT INTO records(kind, url, title, detail, time)
                 VALUES('history', 'https://example.com', 'Before', '', 1);",
            )
            .unwrap();

        // Regression: restore a pre-reading-link snapshot into the current app.
        topic
            .execute_batch("DROP TABLE topic_reading_link; PRAGMA user_version=11;")
            .unwrap();
        backup(&root, &topic, &browser).expect("backup should succeed");
        crate::database::upgrade_restored(&mut topic, &root.join(MASTER_KEY_FILE)).unwrap();
        topic
            .execute(
                "UPDATE app_setting SET value = 'after' WHERE key = 'test-value'",
                [],
            )
            .unwrap();
        browser.execute("DELETE FROM records", []).unwrap();
        restore_latest(&root, &mut topic, &mut browser).expect("restore should succeed");

        let value: String = topic
            .query_row(
                "SELECT value FROM app_setting WHERE key = 'test-value'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let records: i64 = browser
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "before");
        assert_eq!(records, 1);
        assert!(topic.prepare("SELECT url FROM topic_reading_link").is_ok());
        let version: i64 = topic
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 12);
        drop(topic);
        drop(browser);
        fs::remove_dir_all(root).unwrap();
    }
}
