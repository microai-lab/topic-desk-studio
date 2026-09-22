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
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("backup-"))
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

        backup(&root, &topic, &browser).expect("backup should succeed");
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
        drop(topic);
        drop(browser);
        fs::remove_dir_all(root).unwrap();
    }
}
