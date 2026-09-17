//! SQLite connection initialization, schema versioning and transaction boundary.

use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::catalog::PLATFORM_CATALOG;
use crate::error::{AppError, AppResult};

const SCHEMA_VERSION: i64 = 3;
const INITIAL_SCHEMA: &str = include_str!("../migrations/001_initial.sql");

/// Open the application database and migrate compatible older schemas transactionally.
pub fn open_database(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "busy_timeout", 5_000_i64)?;
    initialize(&mut connection)?;
    ensure_platforms(&connection)?;
    Ok(connection)
}

/// Seed catalog metadata without overwriting future user choices such as enabled state or feed URL.
fn ensure_platforms(connection: &Connection) -> AppResult<()> {
    let mut statement = connection.prepare(
        "INSERT INTO platform (code, display_name, home_url, feed_url, enabled, update_time)
         VALUES (?, ?, ?, ?, 1, CURRENT_TIMESTAMP)
         ON CONFLICT(code) DO UPDATE SET display_name = excluded.display_name,
           home_url = excluded.home_url, deleted = 0, update_time = CURRENT_TIMESTAMP",
    )?;
    for platform in PLATFORM_CATALOG {
        statement.execute((
            platform.code,
            platform.display_name,
            platform.home_url,
            platform.endpoint_url,
        ))?;
    }
    Ok(())
}

/// Apply a fresh schema or a known forward-only migration.
fn initialize(connection: &mut Connection) -> AppResult<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        SCHEMA_VERSION => Ok(()),
        0 => {
            let existing: i64 = connection.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )?;
            if existing != 0 {
                return Err(AppError::Initialization(
                    "数据库版本为 0，但已经包含业务表".into(),
                ));
            }
            connection.execute_batch("BEGIN IMMEDIATE")?;
            match connection.execute_batch(INITIAL_SCHEMA) {
                Ok(()) => connection.execute_batch("COMMIT").map_err(AppError::from),
                Err(error) => {
                    let _ = connection.execute_batch("ROLLBACK");
                    Err(AppError::Database(error))
                }
            }
        }
        1 => migrate_from_v1(connection),
        2 => migrate_from_v2(connection),
        unsupported => Err(AppError::Initialization(format!(
            "不支持的 Topic Desk 数据库版本：{unsupported}"
        ))),
    }
}

/// Version 1 lacked both the persistent creation queue and standalone model settings.
fn migrate_from_v1(connection: &mut Connection) -> AppResult<()> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE creation_queue (
           id INTEGER PRIMARY KEY,
           deleted INTEGER NOT NULL DEFAULT 0,
           create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           topic_id INTEGER NOT NULL,
           UNIQUE (topic_id)
         );
         CREATE INDEX idx_creation_queue_active_time
           ON creation_queue (deleted, create_time DESC, id DESC);
         CREATE TABLE app_setting (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL,
           update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         PRAGMA user_version = 3;
         COMMIT;",
    )?;
    Ok(())
}

/// Version 2 is the plugin-compatible schema and needs only standalone application settings.
fn migrate_from_v2(connection: &mut Connection) -> AppResult<()> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE app_setting (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL,
           update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         PRAGMA user_version = 3;
         COMMIT;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new database must be initialized exactly to the supported schema version.
    #[test]
    fn initializes_memory_database() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        initialize(&mut connection).expect("schema should initialize");
        ensure_platforms(&connection).expect("catalog should seed");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version should be readable");
        assert_eq!(version, SCHEMA_VERSION);
        let platform_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM platform", [], |row| row.get(0))
            .expect("catalog count should be readable");
        assert_eq!(platform_count, PLATFORM_CATALOG.len() as i64);
    }
}
