-- Topic Desk Studio SQLite schema version 3, compatible with dsh-topic-desk data exports.
CREATE TABLE platform (
  id INTEGER PRIMARY KEY,
  deleted INTEGER NOT NULL DEFAULT 0,
  create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  code TEXT NOT NULL,
  display_name TEXT NOT NULL,
  home_url TEXT NOT NULL,
  feed_url TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  last_success_run_id INTEGER,
  UNIQUE (code)
);

-- Every attempted source collection gets a durable diagnostic record.
CREATE TABLE collection_run (
  id INTEGER PRIMARY KEY,
  deleted INTEGER NOT NULL DEFAULT 0,
  create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  platform_id INTEGER NOT NULL,
  status TEXT NOT NULL,
  trigger_kind TEXT NOT NULL,
  scheduled_time TEXT NOT NULL,
  start_time TEXT,
  end_time TEXT,
  fetched_count INTEGER NOT NULL DEFAULT 0,
  inserted_count INTEGER NOT NULL DEFAULT 0,
  updated_count INTEGER NOT NULL DEFAULT 0,
  invalid_count INTEGER NOT NULL DEFAULT 0,
  error_message TEXT
);

-- Topic stores the latest state while preserving first-seen facts.
CREATE TABLE topic (
  id INTEGER PRIMARY KEY,
  deleted INTEGER NOT NULL DEFAULT 0,
  create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  platform_id INTEGER NOT NULL,
  source_key TEXT NOT NULL,
  identity_kind TEXT NOT NULL,
  dedupe_version INTEGER NOT NULL DEFAULT 1,
  dedupe_hash BLOB NOT NULL,
  title TEXT NOT NULL,
  canonical_url TEXT NOT NULL,
  published_time TEXT,
  rank INTEGER NOT NULL,
  heat REAL,
  last_collection_run_id INTEGER NOT NULL,
  UNIQUE (platform_id, dedupe_hash)
);

-- Observation keeps per-run ranking history for trends and streaks.
CREATE TABLE topic_observation (
  id INTEGER PRIMARY KEY,
  deleted INTEGER NOT NULL DEFAULT 0,
  create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  topic_id INTEGER NOT NULL,
  collection_run_id INTEGER NOT NULL,
  rank INTEGER NOT NULL,
  heat REAL,
  UNIQUE (collection_run_id, topic_id)
);

-- Creation queue survives a topic dropping from the current source board.
CREATE TABLE creation_queue (
  id INTEGER PRIMARY KEY,
  deleted INTEGER NOT NULL DEFAULT 0,
  create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  topic_id INTEGER NOT NULL,
  UNIQUE (topic_id)
);

-- Non-secret application settings are kept separate from model credentials.
CREATE TABLE app_setting (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- The single configured model credential stays in the local application database.
-- Rust never returns this value to the WebView.
CREATE TABLE model_credential (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  algorithm TEXT NOT NULL CHECK (algorithm = 'AES-256-GCM-file-v1'),
  nonce BLOB NOT NULL CHECK (length(nonce) = 12),
  ciphertext BLOB NOT NULL CHECK (length(ciphertext) > 16),
  create_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_platform_enabled ON platform (enabled, deleted, code);
CREATE INDEX idx_collection_run_platform_time ON collection_run (platform_id, scheduled_time DESC, id DESC);
CREATE INDEX idx_collection_run_status ON collection_run (status, update_time DESC);
CREATE INDEX idx_topic_current_rank ON topic (platform_id, last_collection_run_id, rank, id);
CREATE INDEX idx_topic_heat ON topic (heat DESC, id DESC);
CREATE INDEX idx_topic_update_time ON topic (update_time DESC, id DESC);
CREATE INDEX idx_observation_topic_time ON topic_observation (topic_id, create_time, id);
CREATE INDEX idx_observation_create_time ON topic_observation (create_time, id);
CREATE INDEX idx_creation_queue_active_time ON creation_queue (deleted, create_time DESC, id DESC);

PRAGMA user_version = 6;
