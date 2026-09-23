//! SQLite connection initialization, schema versioning and transaction boundary.

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension, MAIN_DB};
use zeroize::Zeroizing;

use crate::catalog::{
    default_category, default_proxy_mode, default_region, PLATFORM_CATALOG, RETIRED_PLATFORM_CODES,
};
use crate::credential_cipher::{CredentialCipher, EncryptedCredential, ALGORITHM, MASTER_KEY_FILE};
use crate::error::{AppError, AppResult};

const SCHEMA_VERSION: i64 = 11;
const DEFAULT_PROXY_URL: &str = "http://127.0.0.1:7897";
const INITIAL_SCHEMA: &str = include_str!("../migrations/001_initial.sql");

/// Open the application database and migrate compatible older schemas transactionally.
pub fn open_database(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "busy_timeout", 5_000_i64)?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    if !integrity_ok(&connection)? {
        return Err(AppError::Initialization("本地数据库完整性检查失败".into()));
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if (1..SCHEMA_VERSION).contains(&version) {
        backup_before_migration(&connection, path, version)?;
    }
    initialize(&mut connection, &path.with_file_name(MASTER_KEY_FILE))?;
    ensure_platforms(&connection)?;
    Ok(connection)
}

/// Open an independent query-only connection so page reads do not wait on the command writer lock.
pub fn open_reader(path: &Path) -> AppResult<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.pragma_update(None, "busy_timeout", 5_000_i64)?;
    connection.pragma_update(None, "query_only", true)?;
    Ok(connection)
}

/// Run SQLite's bounded startup integrity check without exposing database internals.
pub fn integrity_ok(connection: &Connection) -> AppResult<bool> {
    let result: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    Ok(result == "ok")
}

/// Preserve one online snapshot before a forward migration changes durable state.
fn backup_before_migration(connection: &Connection, path: &Path, version: i64) -> AppResult<()> {
    let directory = path
        .parent()
        .ok_or_else(|| AppError::Initialization("数据库路径没有父目录".into()))?
        .join("backups");
    fs::create_dir_all(&directory)?;
    connection.backup(
        MAIN_DB,
        directory.join(format!("topic-desk-pre-v{version}.sqlite")),
        None,
    )?;
    let key = path.with_file_name(MASTER_KEY_FILE);
    if key.is_file() {
        fs::copy(
            key,
            directory.join(format!("model-credential-pre-v{version}.key")),
        )?;
    }
    Ok(())
}

/// Seed missing catalog rows once; deleted defaults stay deleted until an explicit restore.
fn ensure_platforms(connection: &Connection) -> AppResult<()> {
    let mut statement = connection.prepare(
        "INSERT INTO platform (
           code, display_name, home_url, feed_url, region, category, parser_type,
           proxy_mode, built_in, enabled, create_time, update_time
         ) VALUES (?, ?, ?, ?, ?, ?, 'builtin', ?, 1, 1,
                   datetime('now', 'localtime'), datetime('now', 'localtime'))
         ON CONFLICT(code) DO NOTHING",
    )?;
    for platform in PLATFORM_CATALOG {
        statement.execute((
            platform.code,
            platform.display_name,
            platform.home_url,
            platform.endpoint_url,
            default_region(platform.code),
            default_category(platform.code),
            default_proxy_mode(platform.code),
        ))?;
    }
    // Retired built-ins remain soft-deleted so existing topic history keeps its
    // foreign-key target while the source no longer appears or gets collected.
    for code in RETIRED_PLATFORM_CODES {
        connection.execute(
            "UPDATE platform SET deleted = 1, enabled = 0,
               update_time = datetime('now', 'localtime')
             WHERE code = ? AND built_in = 1",
            [code],
        )?;
    }
    Ok(())
}

/// Apply a fresh schema or a known forward-only migration.
fn initialize(connection: &mut Connection, master_key_path: &Path) -> AppResult<()> {
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
        1 => {
            migrate_from_v1(connection)?;
            migrate_from_v6(connection)?;
            migrate_from_v7(connection)?;
            migrate_from_v8(connection)?;
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        2 => {
            migrate_from_v2(connection)?;
            migrate_from_v6(connection)?;
            migrate_from_v7(connection)?;
            migrate_from_v8(connection)?;
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        3 => {
            migrate_from_v3(connection)?;
            migrate_from_v6(connection)?;
            migrate_from_v7(connection)?;
            migrate_from_v8(connection)?;
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        4 => {
            migrate_from_v4(connection, master_key_path)?;
            migrate_from_v6(connection)?;
            migrate_from_v7(connection)?;
            migrate_from_v8(connection)?;
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        5 => {
            migrate_from_v5(connection)?;
            migrate_from_v6(connection)?;
            migrate_from_v7(connection)?;
            migrate_from_v8(connection)?;
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        6 => {
            migrate_from_v6(connection)?;
            migrate_from_v7(connection)?;
            migrate_from_v8(connection)?;
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        7 => {
            migrate_from_v7(connection)?;
            migrate_from_v8(connection)?;
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        8 => {
            migrate_from_v8(connection)?;
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        9 => {
            migrate_from_v9(connection)?;
            migrate_from_v10(connection)
        }
        10 => migrate_from_v10(connection),
        unsupported => Err(AppError::Initialization(format!(
            "不支持的 Topic Desk 数据库版本：{unsupported}"
        ))),
    }
}

/// Version 11 adds declarative custom-source metadata without changing built-in parsers.
fn migrate_from_v10(connection: &mut Connection) -> AppResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE platform ADD COLUMN region TEXT NOT NULL DEFAULT 'international';
         ALTER TABLE platform ADD COLUMN category TEXT NOT NULL DEFAULT 'general';
         ALTER TABLE platform ADD COLUMN parser_type TEXT NOT NULL DEFAULT 'builtin';
         ALTER TABLE platform ADD COLUMN parser_config TEXT;
         ALTER TABLE platform ADD COLUMN proxy_mode TEXT NOT NULL DEFAULT 'auto';
         ALTER TABLE platform ADD COLUMN built_in INTEGER NOT NULL DEFAULT 1;",
    )?;
    for platform in PLATFORM_CATALOG {
        transaction.execute(
            "UPDATE platform SET region = ?, category = ?, proxy_mode = ?, built_in = 1,
               parser_type = 'builtin' WHERE code = ?",
            params![
                default_region(platform.code),
                default_category(platform.code),
                default_proxy_mode(platform.code),
                platform.code
            ],
        )?;
    }
    transaction.pragma_update(None, "user_version", 11_i64)?;
    transaction.commit()?;
    Ok(())
}

/// Version 10 persists the newest three non-empty addition batches across launches.
fn migrate_from_v9(connection: &mut Connection) -> AppResult<()> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE recent_addition_batch (
           id INTEGER PRIMARY KEY,
           trigger_kind TEXT NOT NULL,
           create_time TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
         );
         CREATE TABLE recent_addition_topic (
           batch_id INTEGER NOT NULL,
           topic_id INTEGER NOT NULL,
           create_time TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
           PRIMARY KEY (batch_id, topic_id)
         );
         CREATE INDEX idx_recent_addition_topic_id
           ON recent_addition_topic (topic_id, batch_id DESC);
         PRAGMA user_version = 10;
         COMMIT;",
    )?;
    Ok(())
}

/// Version 9 seeds a practical loopback proxy once. Deleting the setting later
/// remains an explicit all-direct choice and does not recreate the default.
fn migrate_from_v8(connection: &mut Connection) -> AppResult<()> {
    let transaction = connection.transaction()?;
    let settings_table: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'app_setting'",
        [],
        |row| row.get(0),
    )?;
    if settings_table != 0 {
        transaction.execute(
            "INSERT OR IGNORE INTO app_setting (key, value, update_time)
             VALUES ('network_proxy_url', ?, datetime('now', 'localtime'))",
            [DEFAULT_PROXY_URL],
        )?;
    }
    transaction.pragma_update(None, "user_version", 9_i64)?;
    transaction.commit()?;
    Ok(())
}

/// Version 8 makes application-owned timestamps device-local. Source-provided
/// publication timestamps remain untouched because their offsets are authoritative.
fn migrate_from_v7(connection: &mut Connection) -> AppResult<()> {
    let transaction = connection.transaction()?;
    for (table, columns) in [
        ("platform", &["create_time", "update_time"][..]),
        (
            "collection_run",
            &[
                "create_time",
                "update_time",
                "scheduled_time",
                "start_time",
                "end_time",
            ][..],
        ),
        ("topic", &["create_time", "update_time"][..]),
        ("topic_observation", &["create_time", "update_time"][..]),
        ("creation_queue", &["create_time", "update_time"][..]),
        ("app_setting", &["update_time"][..]),
        ("model_credential", &["create_time", "update_time"][..]),
    ] {
        localize_timestamp_columns(&transaction, table, columns)?;
    }
    transaction.execute("DELETE FROM topic_observation_hourly", [])?;
    transaction.execute("DELETE FROM topic_observation_daily", [])?;
    transaction.pragma_update(None, "user_version", 8_i64)?;
    transaction.commit()?;
    Ok(())
}

/// Convert only columns present in the source schema so every supported legacy
/// version can pass through the local-time migration transactionally.
fn localize_timestamp_columns(
    connection: &Connection,
    table: &str,
    columns: &[&str],
) -> AppResult<()> {
    let mut assignments = Vec::new();
    for column in columns {
        let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?");
        let exists: i64 = connection.query_row(&sql, [column], |row| row.get(0))?;
        if exists != 0 {
            assignments.push(format!(
                "{column} = CASE WHEN {column} IS NULL THEN NULL ELSE datetime({column}, 'localtime') END"
            ));
        }
    }
    if !assignments.is_empty() {
        connection.execute(
            &format!("UPDATE {table} SET {}", assignments.join(", ")),
            [],
        )?;
    }
    Ok(())
}

/// Version 7 adds bounded trend rollups and indexed title search without foreign keys.
fn migrate_from_v6(connection: &mut Connection) -> AppResult<()> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE topic_observation_hourly (
           topic_id INTEGER NOT NULL,
           bucket TEXT NOT NULL,
           rank INTEGER NOT NULL,
           heat REAL,
           sample_count INTEGER NOT NULL,
           PRIMARY KEY (topic_id, bucket)
         );
         CREATE TABLE topic_observation_daily (
           topic_id INTEGER NOT NULL,
           bucket TEXT NOT NULL,
           rank INTEGER NOT NULL,
           heat REAL,
           sample_count INTEGER NOT NULL,
           PRIMARY KEY (topic_id, bucket)
         );
         CREATE VIRTUAL TABLE topic_fts USING fts5(
           title, content = 'topic', content_rowid = 'id', tokenize = 'trigram'
         );
         INSERT INTO topic_fts(rowid, title) SELECT id, title FROM topic;
         CREATE TRIGGER topic_fts_insert AFTER INSERT ON topic BEGIN
           INSERT INTO topic_fts(rowid, title) VALUES (new.id, new.title);
         END;
         CREATE TRIGGER topic_fts_delete AFTER DELETE ON topic BEGIN
           INSERT INTO topic_fts(topic_fts, rowid, title) VALUES ('delete', old.id, old.title);
         END;
         CREATE TRIGGER topic_fts_update AFTER UPDATE OF title ON topic BEGIN
           INSERT INTO topic_fts(topic_fts, rowid, title) VALUES ('delete', old.id, old.title);
           INSERT INTO topic_fts(rowid, title) VALUES (new.id, new.title);
         END;
         CREATE INDEX idx_observation_run_topic
           ON topic_observation (collection_run_id, topic_id, deleted);
         CREATE INDEX idx_collection_run_platform_status_end
           ON collection_run (platform_id, status, end_time DESC, id DESC);
         PRAGMA user_version = 7;
         COMMIT;",
    )?;
    Ok(())
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
         CREATE TABLE model_credential (
           id INTEGER PRIMARY KEY CHECK (id = 1),
           algorithm TEXT NOT NULL CHECK (algorithm = 'AES-256-GCM-file-v1'),
           nonce BLOB NOT NULL CHECK (length(nonce) = 12),
           ciphertext BLOB NOT NULL CHECK (length(ciphertext) > 16),
           create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         PRAGMA user_version = 6;
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
         CREATE TABLE model_credential (
           id INTEGER PRIMARY KEY CHECK (id = 1),
           algorithm TEXT NOT NULL CHECK (algorithm = 'AES-256-GCM-file-v1'),
           nonce BLOB NOT NULL CHECK (length(nonce) = 12),
           ciphertext BLOB NOT NULL CHECK (length(ciphertext) > 16),
           create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         PRAGMA user_version = 6;
         COMMIT;",
    )?;
    Ok(())
}

/// Version 3 stored model credentials outside SQLite and therefore lacks the credential table.
fn migrate_from_v3(connection: &mut Connection) -> AppResult<()> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE model_credential (
           id INTEGER PRIMARY KEY CHECK (id = 1),
           algorithm TEXT NOT NULL CHECK (algorithm = 'AES-256-GCM-file-v1'),
           nonce BLOB NOT NULL CHECK (length(nonce) = 12),
           ciphertext BLOB NOT NULL CHECK (length(ciphertext) > 16),
           create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         PRAGMA user_version = 6;
         COMMIT;",
    )?;
    Ok(())
}

/// Version 4 plaintext is encrypted before its old table is removed in the same transaction.
fn migrate_from_v4(connection: &mut Connection, master_key_path: &Path) -> AppResult<()> {
    let plaintext: Option<Zeroizing<String>> = connection
        .query_row(
            "SELECT api_key FROM model_credential WHERE id = 1",
            [],
            |row| row.get::<_, String>(0).map(Zeroizing::new),
        )
        .optional()?;
    let encrypted = plaintext
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| CredentialCipher::load_or_create(master_key_path)?.encrypt(value))
        .transpose()?;
    replace_v4_credential_table(connection, encrypted.as_ref())
}

/// Replace the plaintext v4 table transactionally after encryption has succeeded.
fn replace_v4_credential_table(
    connection: &mut Connection,
    encrypted: Option<&EncryptedCredential>,
) -> AppResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE model_credential RENAME TO model_credential_v4;
         CREATE TABLE model_credential (
           id INTEGER PRIMARY KEY CHECK (id = 1),
           algorithm TEXT NOT NULL CHECK (algorithm = 'AES-256-GCM-file-v1'),
           nonce BLOB NOT NULL CHECK (length(nonce) = 12),
           ciphertext BLOB NOT NULL CHECK (length(ciphertext) > 16),
           create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );",
    )?;
    if let Some(encrypted) = encrypted {
        transaction.execute(
            "INSERT INTO model_credential (id, algorithm, nonce, ciphertext)
             VALUES (1, ?, ?, ?)",
            params![ALGORITHM, &encrypted.nonce, &encrypted.ciphertext],
        )?;
    }
    transaction.execute_batch(
        "DROP TABLE model_credential_v4;
         PRAGMA user_version = 6;",
    )?;
    transaction.commit()?;
    Ok(())
}

/// Version 5 used a system-vault master key; drop that unreadable payload without accessing it.
fn migrate_from_v5(connection: &mut Connection) -> AppResult<()> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         DROP TABLE model_credential;
         CREATE TABLE model_credential (
           id INTEGER PRIMARY KEY CHECK (id = 1),
           algorithm TEXT NOT NULL CHECK (algorithm = 'AES-256-GCM-file-v1'),
           nonce BLOB NOT NULL CHECK (length(nonce) = 12),
           ciphertext BLOB NOT NULL CHECK (length(ciphertext) > 16),
           create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         PRAGMA user_version = 6;
         COMMIT;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Older-version fixtures retain the domain tables that existed before credential migrations.
    fn create_legacy_domain_tables(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE platform (
                   id INTEGER PRIMARY KEY, code TEXT NOT NULL, display_name TEXT NOT NULL,
                   home_url TEXT NOT NULL, feed_url TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1,
                   deleted INTEGER NOT NULL DEFAULT 0, create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                   update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, last_success_run_id INTEGER
                 );
                 CREATE TABLE collection_run (
                   id INTEGER PRIMARY KEY, platform_id INTEGER NOT NULL, status TEXT NOT NULL,
                   end_time TEXT, deleted INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE topic (id INTEGER PRIMARY KEY, title TEXT NOT NULL);
                 CREATE TABLE topic_observation (
                   id INTEGER PRIMARY KEY, topic_id INTEGER NOT NULL,
                   collection_run_id INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0
                 );",
            )
            .expect("legacy domain tables should initialize");
    }

    /// A new database must be initialized exactly to the supported schema version.
    #[test]
    fn initializes_memory_database() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        initialize(&mut connection, Path::new("unused-test-key"))
            .expect("schema should initialize");
        ensure_platforms(&connection).expect("catalog should seed");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version should be readable");
        assert_eq!(version, SCHEMA_VERSION);
        let platform_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM platform", [], |row| row.get(0))
            .expect("catalog count should be readable");
        assert_eq!(platform_count, PLATFORM_CATALOG.len() as i64);
        let credential_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'model_credential'",
                [],
                |row| row.get(0),
            )
            .expect("credential table should be readable");
        assert_eq!(credential_table, 1);
        let plaintext_column: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('model_credential') WHERE name = 'api_key'",
                [],
                |row| row.get(0),
            )
            .expect("credential columns should be readable");
        assert_eq!(plaintext_column, 0);
    }

    /// Existing version-3 databases gain the encrypted credential table without losing settings.
    #[test]
    fn migrates_version_three_database() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        create_legacy_domain_tables(&connection);
        connection
            .execute_batch(
                "CREATE TABLE app_setting (
                   key TEXT PRIMARY KEY,
                   value TEXT NOT NULL,
                   update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 INSERT INTO app_setting (key, value) VALUES ('model_name', 'existing-model');
                 PRAGMA user_version = 3;",
            )
            .expect("version-three fixture should initialize");

        initialize(&mut connection, Path::new("unused-test-key")).expect("database should migrate");

        let model: String = connection
            .query_row(
                "SELECT value FROM app_setting WHERE key = 'model_name'",
                [],
                |row| row.get(0),
            )
            .expect("existing setting should remain");
        assert_eq!(model, "existing-model");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version should be readable");
        assert_eq!(version, SCHEMA_VERSION);
    }

    /// An empty v4 credential table migrates without requiring access to a system master key.
    #[test]
    fn migrates_empty_version_four_credential_table() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        create_legacy_domain_tables(&connection);
        connection
            .execute_batch(
                "CREATE TABLE model_credential (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   api_key TEXT NOT NULL,
                   create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                   update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 PRAGMA user_version = 4;",
            )
            .expect("version-four fixture should initialize");

        initialize(&mut connection, Path::new("unused-test-key")).expect("database should migrate");

        let encrypted_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('model_credential')
                 WHERE name IN ('algorithm', 'nonce', 'ciphertext')",
                [],
                |row| row.get(0),
            )
            .expect("encrypted columns should be readable");
        assert_eq!(encrypted_columns, 3);
    }

    /// A v4 plaintext row is removed when its encrypted replacement is committed.
    #[test]
    fn replaces_version_four_plaintext_with_ciphertext() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        connection
            .execute_batch(
                "CREATE TABLE model_credential (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   api_key TEXT NOT NULL,
                   create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                   update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 INSERT INTO model_credential (id, api_key) VALUES (1, 'legacy-plaintext');
                 PRAGMA user_version = 4;",
            )
            .expect("version-four fixture should initialize");
        let encrypted = EncryptedCredential {
            nonce: vec![3; 12],
            ciphertext: vec![4; 32],
        };

        replace_v4_credential_table(&mut connection, Some(&encrypted))
            .expect("credential table should be replaced");

        let stored: Vec<u8> = connection
            .query_row(
                "SELECT ciphertext FROM model_credential WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("ciphertext should be stored");
        assert_eq!(stored, encrypted.ciphertext);
        let plaintext_column: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('model_credential') WHERE name = 'api_key'",
                [],
                |row| row.get(0),
            )
            .expect("credential columns should be readable");
        assert_eq!(plaintext_column, 0);
    }

    /// Version 5 is cleared without reading the retired operating-system credential entry.
    #[test]
    fn removes_version_five_system_vault_payload() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        create_legacy_domain_tables(&connection);
        connection
            .execute_batch(
                "CREATE TABLE model_credential (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   algorithm TEXT NOT NULL,
                   nonce BLOB NOT NULL,
                   ciphertext BLOB NOT NULL,
                   create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                   update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 INSERT INTO model_credential (id, algorithm, nonce, ciphertext)
                 VALUES (1, 'AES-256-GCM-v1', zeroblob(12), zeroblob(32));
                 PRAGMA user_version = 5;",
            )
            .expect("version-five fixture should initialize");

        initialize(&mut connection, Path::new("unused-test-key")).expect("database should migrate");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM model_credential", [], |row| {
                row.get(0)
            })
            .expect("credential table should be readable");
        assert_eq!(count, 0);
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version should be readable");
        assert_eq!(version, SCHEMA_VERSION);
    }

    /// Startup catalog seeding must not undo an explicit deletion of a default source.
    #[test]
    fn catalog_seeding_preserves_deleted_defaults() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        initialize(&mut connection, Path::new("unused-test-key"))
            .expect("schema should initialize");
        ensure_platforms(&connection).expect("catalog should seed");
        connection
            .execute("UPDATE platform SET deleted = 1 WHERE code = 'qbitai'", [])
            .expect("default source should delete");

        ensure_platforms(&connection).expect("catalog should seed idempotently");

        let deleted: i64 = connection
            .query_row(
                "SELECT deleted FROM platform WHERE code = 'qbitai'",
                [],
                |row| row.get(0),
            )
            .expect("default source should remain stored");
        assert_eq!(deleted, 1);
    }

    /// Sources retired from the catalog must disappear from upgraded databases.
    #[test]
    fn catalog_seeding_retires_removed_defaults() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        initialize(&mut connection, Path::new("unused-test-key"))
            .expect("schema should initialize");
        connection
            .execute(
                "INSERT INTO platform (
                   code, display_name, home_url, feed_url, region, category,
                   parser_type, proxy_mode, built_in, enabled
                 ) VALUES (
                   'mastodon-zh', 'Mastodon 中文', 'https://m.cmx.im/',
                   'https://m.cmx.im/api/v1/trends/statuses?limit=40',
                   'international', 'general', 'builtin', 'proxy', 1, 1
                 )",
                [],
            )
            .expect("retired fixture should insert");

        ensure_platforms(&connection).expect("catalog should retire removed defaults");

        let state: (i64, i64) = connection
            .query_row(
                "SELECT deleted, enabled FROM platform WHERE code = 'mastodon-zh'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("retired source should remain stored for history");
        assert_eq!(state, (1, 0));
    }
}
