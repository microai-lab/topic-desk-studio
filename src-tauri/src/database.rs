//! SQLite connection initialization, schema versioning and transaction boundary.

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension, MAIN_DB};
use zeroize::Zeroizing;

use crate::catalog::PLATFORM_CATALOG;
use crate::credential_cipher::{CredentialCipher, EncryptedCredential, ALGORITHM, MASTER_KEY_FILE};
use crate::error::{AppError, AppResult};

const SCHEMA_VERSION: i64 = 7;
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
            migrate_from_v6(connection)
        }
        2 => {
            migrate_from_v2(connection)?;
            migrate_from_v6(connection)
        }
        3 => {
            migrate_from_v3(connection)?;
            migrate_from_v6(connection)
        }
        4 => {
            migrate_from_v4(connection, master_key_path)?;
            migrate_from_v6(connection)
        }
        5 => {
            migrate_from_v5(connection)?;
            migrate_from_v6(connection)
        }
        6 => migrate_from_v6(connection),
        unsupported => Err(AppError::Initialization(format!(
            "不支持的 Topic Desk 数据库版本：{unsupported}"
        ))),
    }
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
}
