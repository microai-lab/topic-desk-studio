//! SQLite repository for topic pages, source health and persistent creation queue.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row, ToSql};

use crate::credential_cipher::{EncryptedCredential, ALGORITHM};
use crate::error::{AppError, AppResult};
use crate::identity::identify_topic;
use crate::models::{
    CollectionStats, ModelSettings, NetworkSettings, ParsedFeed, PlatformSource,
    PlatformStatusView, SourceRegion, TopicCategory, TopicPage, TopicQuery, TopicSort, TopicView,
    UiLocale, UiPreferences, UiTheme,
};

/// Resolve a source region without trusting values stored outside the platform catalog.
fn region_for(code: &str) -> SourceRegion {
    const DOMESTIC: &[&str] = &[
        "qbitai",
        "ithome",
        "36kr",
        "huxiu",
        "c114",
        "wallstreetcn",
        "odaily",
        "xueqiu",
        "toutiao",
        "thepaper",
        "zhihu",
        "jin10",
        "cls",
        "xiaohongshu",
        "weibo",
        "douyin",
        "bilibili",
        "baidu",
        "sspai",
        "solidot",
    ];
    if DOMESTIC.contains(&code) {
        SourceRegion::Domestic
    } else {
        SourceRegion::International
    }
}

/// Resolve the stable product category assigned to a platform code.
fn category_for(code: &str) -> TopicCategory {
    match code {
        "qbitai"
        | "ithome"
        | "c114"
        | "hugging-face"
        | "arxiv"
        | "techcrunch"
        | "the-verge"
        | "ars-technica"
        | "mit-technology-review"
        | "infoq"
        | "sspai"
        | "solidot" => TopicCategory::Technology,
        "wallstreetcn"
        | "odaily"
        | "xueqiu"
        | "binance-square-zh"
        | "binance-square-global"
        | "coingecko"
        | "polymarket"
        | "federal-reserve"
        | "sec"
        | "bloomberg"
        | "jin10"
        | "cls" => TopicCategory::Finance,
        "hacker-news" | "github" | "stack-overflow" | "dev-community" | "lobsters" => {
            TopicCategory::Developer
        }
        _ => TopicCategory::General,
    }
}

/// Repository borrows the application-owned connection for one short operation.
pub struct TopicRepository<'connection> {
    connection: &'connection Connection,
}

impl<'connection> TopicRepository<'connection> {
    /// Construct a repository without taking ownership of the SQLite connection.
    pub fn new(connection: &'connection Connection) -> Self {
        Self { connection }
    }

    /// Return one validated, paginated view of current topics and platform health.
    pub fn list(&self, query: &TopicQuery) -> AppResult<TopicPage> {
        let limit = query.limit.unwrap_or(20).clamp(1, 100) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let mut where_parts = vec!["t.deleted = 0", "p.deleted = 0"];
        if query.queued_only.unwrap_or(false) && query.recent_only.unwrap_or(false) {
            return Err(AppError::InvalidInput(
                "待创作与最近新增不能同时筛选".into(),
            ));
        }
        if query.recent_only.unwrap_or(false) {
            where_parts.push(
                "EXISTS (SELECT 1 FROM recent_addition_topic recent WHERE recent.topic_id = t.id)",
            );
        } else if query.queued_only.unwrap_or(false) {
            where_parts.push("cq.id IS NOT NULL");
        } else {
            where_parts.push("p.last_success_run_id = t.last_collection_run_id");
        }

        let mut owned_values: Vec<Box<dyn ToSql>> = Vec::new();
        if let Some(source) = query.source.as_ref() {
            where_parts.push("p.code = ?");
            owned_values.push(Box::new(source.clone()));
        }
        if let Some(search) = query
            .search
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            if search.chars().count() >= 3 {
                where_parts.push("t.id IN (SELECT rowid FROM topic_fts WHERE topic_fts MATCH ?)");
                owned_values.push(Box::new(format!("\"{}\"", search.replace('"', "\"\""))));
            } else {
                where_parts.push("t.title LIKE ? ESCAPE '\\'");
                let escaped = search
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                owned_values.push(Box::new(format!("%{escaped}%")));
            }
        }
        if let Some(topic_ids) = query.topic_ids.as_ref() {
            if topic_ids.len() > 5_000 || topic_ids.iter().any(|id| *id <= 0) {
                return Err(AppError::InvalidInput(
                    "topicIds 最多允许 5000 个正整数".into(),
                ));
            }
            if topic_ids.is_empty() {
                where_parts.push("1 = 0");
            } else {
                where_parts.push("t.id IN (SELECT value FROM json_each(?))");
                owned_values.push(Box::new(serde_json::to_string(topic_ids).map_err(
                    |error| AppError::InvalidInput(format!("话题筛选序列化失败：{error}")),
                )?));
            }
        }

        let candidate_codes = self.filtered_platform_codes(query.region, query.category)?;
        if query.region.is_some() || query.category.is_some() {
            if candidate_codes.is_empty() {
                where_parts.push("1 = 0");
            } else {
                where_parts.push("p.code IN (SELECT value FROM json_each(?))");
                owned_values.push(Box::new(serde_json::to_string(&candidate_codes).map_err(
                    |error| AppError::InvalidInput(format!("平台筛选序列化失败：{error}")),
                )?));
            }
        }

        let clause = where_parts.join(" AND ");
        let values = owned_values
            .iter()
            .map(|value| value.as_ref())
            .collect::<Vec<_>>();
        let total: i64 = self.connection.query_row(
            &format!(
                "SELECT COUNT(*) FROM topic t
                 JOIN platform p ON p.id = t.platform_id
                 LEFT JOIN creation_queue cq ON cq.topic_id = t.id AND cq.deleted = 0
                 WHERE {clause}"
            ),
            values.as_slice(),
            |row| row.get(0),
        )?;

        let order = match query.sort.unwrap_or_default() {
            TopicSort::Rank => "t.rank ASC, p.code ASC, t.id ASC",
            TopicSort::Updated => "t.update_time DESC, t.id DESC",
        };
        let mut page_values = values;
        let limit_value: &dyn ToSql = &limit;
        let offset_value: &dyn ToSql = &offset;
        page_values.push(limit_value);
        page_values.push(offset_value);
        let mut statement = self.connection.prepare(&format!(
            "SELECT t.id, p.code, p.display_name, t.title, t.canonical_url, t.published_time,
                    t.rank, t.heat, t.create_time, t.update_time,
                    ROW_NUMBER() OVER (ORDER BY t.rank ASC, p.code ASC, t.id ASC),
                    CASE WHEN cq.id IS NULL THEN 0 ELSE 1 END, cq.create_time
             FROM topic t
             JOIN platform p ON p.id = t.platform_id
             LEFT JOIN creation_queue cq ON cq.topic_id = t.id AND cq.deleted = 0
             WHERE {clause} ORDER BY {order} LIMIT ? OFFSET ?"
        ))?;
        let mut topics = statement
            .query_map(page_values.as_slice(), |row| self.map_topic(row))?
            .collect::<Result<Vec<_>, _>>()?;
        self.enrich_topics(&mut topics)?;
        for topic in &mut topics {
            topic.rank_delta = topic
                .trend
                .iter()
                .rev()
                .nth(1)
                .map(|previous| previous - topic.rank);
        }

        let queued_total: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM creation_queue cq
             JOIN topic t ON t.id = cq.topic_id
             JOIN platform p ON p.id = t.platform_id
             WHERE cq.deleted = 0 AND t.deleted = 0 AND p.deleted = 0",
            [],
            |row| row.get(0),
        )?;
        let recent_total: i64 = self.connection.query_row(
            "SELECT COUNT(DISTINCT recent.topic_id)
             FROM recent_addition_topic recent
             JOIN topic t ON t.id = recent.topic_id AND t.deleted = 0
             JOIN platform p ON p.id = t.platform_id AND p.deleted = 0",
            [],
            |row| row.get(0),
        )?;

        Ok(TopicPage {
            topics,
            total,
            queued_total,
            recent_total,
            statuses: self.statuses()?,
            history_enabled: true,
        })
    }

    /// Persist one non-empty addition batch and transactionally retain only the newest three.
    pub fn record_recent_additions(&self, trigger: &str, topic_ids: &[i64]) -> AppResult<()> {
        let mut unique = topic_ids
            .iter()
            .copied()
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
        unique.sort_unstable();
        unique.dedup();
        if unique.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO recent_addition_batch (trigger_kind, create_time)
             VALUES (?, datetime('now', 'localtime'))",
            [trigger],
        )?;
        let batch_id = transaction.last_insert_rowid();
        for topic_id in unique {
            transaction.execute(
                "INSERT INTO recent_addition_topic (batch_id, topic_id, create_time)
                 SELECT ?, id, datetime('now', 'localtime') FROM topic
                 WHERE id = ? AND deleted = 0",
                params![batch_id, topic_id],
            )?;
        }
        transaction.execute(
            "DELETE FROM recent_addition_topic WHERE batch_id IN (
               SELECT id FROM recent_addition_batch ORDER BY id DESC LIMIT -1 OFFSET 3
             )",
            [],
        )?;
        transaction.execute(
            "DELETE FROM recent_addition_batch WHERE id IN (
               SELECT id FROM recent_addition_batch ORDER BY id DESC LIMIT -1 OFFSET 3
             )",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Idempotently add or remove an existing topic from the creation queue.
    pub fn set_queued(&self, topic_id: i64, queued: bool) -> AppResult<()> {
        if topic_id <= 0 {
            return Err(AppError::InvalidInput("topicId 必须是正整数".into()));
        }
        let exists = self
            .connection
            .query_row(
                "SELECT 1 FROM topic t JOIN platform p ON p.id = t.platform_id
                 WHERE t.id = ? AND t.deleted = 0 AND p.deleted = 0",
                [topic_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(AppError::InvalidInput("话题不存在或已失效".into()));
        }
        if queued {
            self.connection.execute(
                "INSERT INTO creation_queue (topic_id, deleted, create_time, update_time)
                 VALUES (?, 0, datetime('now', 'localtime'), datetime('now', 'localtime'))
                 ON CONFLICT(topic_id) DO UPDATE SET deleted = 0,
                   create_time = excluded.create_time, update_time = excluded.update_time",
                [topic_id],
            )?;
        } else {
            self.connection.execute(
                "UPDATE creation_queue SET deleted = 1, update_time = datetime('now', 'localtime')
                 WHERE topic_id = ? AND deleted = 0",
                [topic_id],
            )?;
        }
        Ok(())
    }

    /// Persist a user's source toggle; catalog synchronization intentionally preserves this value.
    pub fn set_platform_enabled(&self, code: &str, enabled: bool) -> AppResult<()> {
        let changed = self.connection.execute(
            "UPDATE platform SET enabled = ?, update_time = datetime('now', 'localtime')
             WHERE code = ? AND deleted = 0",
            params![enabled, code],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(format!("未知来源：{code}")));
        }
        Ok(())
    }

    /// Load UI preferences from SQLite so the temporary main WebView remains stateless.
    pub fn ui_preferences(&self) -> AppResult<UiPreferences> {
        let value = |key: &str| -> AppResult<Option<String>> {
            Ok(self
                .connection
                .query_row(
                    "SELECT value FROM app_setting WHERE key = ?",
                    [key],
                    |row| row.get(0),
                )
                .optional()?)
        };
        let locale = match value("ui_locale")?.as_deref() {
            Some("zh") => Some(UiLocale::Zh),
            Some("en") => Some(UiLocale::En),
            Some(other) => {
                return Err(AppError::Initialization(format!(
                    "不支持的界面语言设置：{other}"
                )))
            }
            None => None,
        };
        let theme = match value("ui_theme")?.as_deref() {
            Some("light") => Some(UiTheme::Light),
            Some("dark") => Some(UiTheme::Dark),
            Some("system") => Some(UiTheme::System),
            Some(other) => {
                return Err(AppError::Initialization(format!(
                    "不支持的界面主题设置：{other}"
                )))
            }
            None => None,
        };
        Ok(UiPreferences { locale, theme })
    }

    /// Atomically persist the complete UI preference pair to avoid split state.
    pub fn save_ui_preferences(&self, locale: UiLocale, theme: UiTheme) -> AppResult<()> {
        let transaction = self.connection.unchecked_transaction()?;
        for (key, value) in [("ui_locale", locale.as_str()), ("ui_theme", theme.as_str())] {
            transaction.execute(
                "INSERT INTO app_setting (key, value, update_time)
                 VALUES (?, ?, datetime('now', 'localtime'))
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                   update_time = datetime('now', 'localtime')",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Load the optional native collection proxy without exposing environment state.
    pub fn network_settings(&self) -> AppResult<NetworkSettings> {
        let proxy_url = self
            .connection
            .query_row(
                "SELECT value FROM app_setting WHERE key = 'network_proxy_url'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(NetworkSettings { proxy_url })
    }

    /// Persist or clear the proxy atomically; proxy credentials are rejected by the command layer.
    pub fn save_network_settings(&self, proxy_url: Option<&str>) -> AppResult<()> {
        match proxy_url {
            Some(value) => self.connection.execute(
                "INSERT INTO app_setting (key, value, update_time)
                 VALUES ('network_proxy_url', ?, datetime('now', 'localtime'))
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                   update_time = datetime('now', 'localtime')",
                [value],
            )?,
            None => self.connection.execute(
                "DELETE FROM app_setting WHERE key = 'network_proxy_url'",
                [],
            )?,
        };
        Ok(())
    }

    /// Load model routing while exposing only credential presence to command callers.
    pub fn model_settings(&self) -> AppResult<ModelSettings> {
        let value = |key: &str, fallback: &str| -> AppResult<String> {
            Ok(self
                .connection
                .query_row(
                    "SELECT value FROM app_setting WHERE key = ?",
                    [key],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or_else(|| fallback.to_owned()))
        };
        Ok(ModelSettings {
            endpoint: value("model_endpoint", "https://api.deepseek.com")?,
            model: value("model_name", "deepseek-chat")?,
            has_api_key: self.encrypted_api_key()?.is_some(),
        })
    }

    /// Persist routing and an optional key atomically so request configuration cannot be half-saved.
    pub fn save_model_settings(
        &self,
        endpoint: &str,
        model: &str,
        api_key: Option<&EncryptedCredential>,
    ) -> AppResult<()> {
        let transaction = self.connection.unchecked_transaction()?;
        for (key, value) in [("model_endpoint", endpoint), ("model_name", model)] {
            transaction.execute(
                "INSERT INTO app_setting (key, value, update_time)
                 VALUES (?, ?, datetime('now', 'localtime'))
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                   update_time = datetime('now', 'localtime')",
                params![key, value],
            )?;
        }
        if let Some(api_key) = api_key {
            transaction.execute(
                "INSERT INTO model_credential
                   (id, algorithm, nonce, ciphertext, create_time, update_time)
                 VALUES (1, ?, ?, ?, datetime('now', 'localtime'), datetime('now', 'localtime'))
                 ON CONFLICT(id) DO UPDATE SET algorithm = excluded.algorithm,
                   nonce = excluded.nonce, ciphertext = excluded.ciphertext,
                   update_time = datetime('now', 'localtime')",
                params![ALGORITHM, api_key.nonce, api_key.ciphertext],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Load only encrypted model-key material for later native in-memory decryption.
    pub fn encrypted_api_key(&self) -> AppResult<Option<EncryptedCredential>> {
        let row = self
            .connection
            .query_row(
                "SELECT algorithm, nonce, ciphertext FROM model_credential WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        match row {
            Some((algorithm, nonce, ciphertext)) if algorithm == ALGORITHM => {
                Ok(Some(EncryptedCredential { nonce, ciphertext }))
            }
            Some((algorithm, _, _)) => Err(AppError::Credential(format!(
                "不支持的模型凭据加密算法：{algorithm}"
            ))),
            None => Ok(None),
        }
    }

    /// Read a current-board title by ID so the WebView cannot inject arbitrary model input.
    pub fn topic_title(&self, topic_id: i64) -> AppResult<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT t.title FROM topic t JOIN platform p ON p.id = t.platform_id
                 WHERE t.id = ? AND t.deleted = 0 AND p.deleted = 0
                   AND p.last_success_run_id = t.last_collection_run_id",
                [topic_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Load enabled sources before collection so no database lock is held during HTTP requests.
    pub fn enabled_platforms(&self) -> AppResult<Vec<PlatformSource>> {
        let mut statement = self.connection.prepare(
            "SELECT id, code, feed_url FROM platform
             WHERE deleted = 0 AND enabled != 0 ORDER BY id",
        )?;
        let platforms = statement
            .query_map([], |row| {
                Ok(PlatformSource {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    endpoint_url: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(platforms)
    }

    /// Persist the beginning of one collection attempt before network work starts.
    pub fn create_run(&self, platform_id: i64, trigger: &str) -> AppResult<i64> {
        self.connection.execute(
            "INSERT INTO collection_run (
               platform_id, status, trigger_kind, create_time, scheduled_time, start_time, update_time
             ) VALUES (?, 'running', ?, datetime('now', 'localtime'), datetime('now', 'localtime'),
                       datetime('now', 'localtime'), datetime('now', 'localtime'))",
            params![platform_id, trigger],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Mark a failed source without affecting successful results from other sources.
    pub fn fail_run(&self, run_id: i64, message: &str) -> AppResult<()> {
        self.connection.execute(
            "UPDATE collection_run SET status = 'failed', end_time = datetime('now', 'localtime'),
               error_message = ?, update_time = datetime('now', 'localtime') WHERE id = ?",
            params![message, run_id],
        )?;
        Ok(())
    }

    /// Atomically merge a parsed feed, its observations, run counters and current-board pointer.
    pub fn commit_feed(
        &self,
        platform: &PlatformSource,
        run_id: i64,
        feed: &ParsedFeed,
    ) -> AppResult<CollectionStats> {
        let transaction = self.connection.unchecked_transaction()?;
        let valid_run = transaction
            .query_row(
                "SELECT 1 FROM collection_run
                 WHERE id = ? AND platform_id = ? AND status = 'running' AND deleted = 0",
                params![run_id, platform.id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !valid_run {
            return Err(AppError::InvalidInput(format!(
                "采集运行不属于平台或状态无效：{}/{run_id}",
                platform.code
            )));
        }

        let mut stats = CollectionStats::default();
        for topic in &feed.topics {
            let identity = identify_topic(topic)?;
            let existing = transaction
                .query_row(
                    "SELECT id, source_key, identity_kind, title FROM topic
                     WHERE platform_id = ? AND dedupe_hash = ?",
                    params![platform.id, identity.hash.as_slice()],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            let topic_id = if let Some((id, source_key, identity_kind, stored_title)) = existing {
                if source_key != identity.source_key || identity_kind != identity.kind {
                    return Err(AppError::Initialization(format!(
                        "检测到去重哈希冲突：{}/{}",
                        platform.code, identity.source_key
                    )));
                }
                if should_repair_legacy_title(&platform.code, &stored_title, &topic.title) {
                    transaction.execute(
                        "UPDATE topic SET title = ?, rank = ?, heat = ?, last_collection_run_id = ?,
                           deleted = 0, update_time = datetime('now', 'localtime') WHERE id = ?",
                        params![topic.title, topic.rank, topic.heat, run_id, id],
                    )?;
                } else {
                    transaction.execute(
                        "UPDATE topic SET rank = ?, heat = ?, last_collection_run_id = ?,
                           deleted = 0, update_time = datetime('now', 'localtime') WHERE id = ?",
                        params![topic.rank, topic.heat, run_id, id],
                    )?;
                }
                stats.updated += 1;
                id
            } else {
                transaction.execute(
                    "INSERT INTO topic (
                       platform_id, source_key, identity_kind, dedupe_version, dedupe_hash,
                       title, canonical_url, published_time, rank, heat, last_collection_run_id,
                       create_time, update_time
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                               datetime('now', 'localtime'), datetime('now', 'localtime'))",
                    params![
                        platform.id,
                        identity.source_key,
                        identity.kind,
                        identity.version,
                        identity.hash.as_slice(),
                        topic.title,
                        topic.url,
                        topic.published_time,
                        topic.rank,
                        topic.heat,
                        run_id
                    ],
                )?;
                let id = transaction.last_insert_rowid();
                stats.inserted += 1;
                stats.inserted_topic_ids.push(id);
                id
            };
            transaction.execute(
                "INSERT INTO topic_observation (
                   topic_id, collection_run_id, rank, heat, create_time, update_time
                 ) VALUES (?, ?, ?, ?, datetime('now', 'localtime'), datetime('now', 'localtime'))
                 ON CONFLICT(collection_run_id, topic_id) DO UPDATE SET
                   rank = excluded.rank, heat = excluded.heat, deleted = 0,
                   update_time = datetime('now', 'localtime')",
                params![topic_id, run_id, topic.rank, topic.heat],
            )?;
        }

        transaction.execute(
            "UPDATE collection_run SET status = 'succeeded', end_time = datetime('now', 'localtime'),
               fetched_count = ?, inserted_count = ?, updated_count = ?, invalid_count = ?,
               error_message = NULL, update_time = datetime('now', 'localtime') WHERE id = ?",
            params![
                feed.fetched_count,
                stats.inserted,
                stats.updated,
                feed.invalid_count,
                run_id
            ],
        )?;
        transaction.execute(
            "UPDATE platform SET last_success_run_id = ?, update_time = datetime('now', 'localtime') WHERE id = ?",
            params![run_id, platform.id],
        )?;
        transaction.commit()?;
        Ok(stats)
    }

    /// Roll up trends and prune durable operational data with explicit retention windows.
    pub fn maintain_storage(&self) -> AppResult<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute_batch(
            "INSERT INTO topic_observation_hourly(topic_id, bucket, rank, heat, sample_count)
             SELECT topic_id, strftime('%Y-%m-%d %H:00:00', create_time),
                    CAST(ROUND(AVG(rank)) AS INTEGER), AVG(heat), COUNT(*)
             FROM topic_observation
             WHERE deleted = 0 AND create_time < datetime('now', 'localtime', '-1 day')
             GROUP BY topic_id, strftime('%Y-%m-%d %H:00:00', create_time)
             ON CONFLICT(topic_id, bucket) DO UPDATE SET
               rank = excluded.rank, heat = excluded.heat, sample_count = excluded.sample_count;

             INSERT INTO topic_observation_daily(topic_id, bucket, rank, heat, sample_count)
             SELECT topic_id, substr(bucket, 1, 10),
                    CAST(ROUND(AVG(rank)) AS INTEGER), AVG(heat), SUM(sample_count)
             FROM topic_observation_hourly
             WHERE bucket < strftime('%Y-%m-%d 00:00:00', 'now', 'localtime', '-14 days')
             GROUP BY topic_id, substr(bucket, 1, 10)
             ON CONFLICT(topic_id, bucket) DO UPDATE SET
               rank = excluded.rank, heat = excluded.heat, sample_count = excluded.sample_count;

             DELETE FROM topic_observation WHERE create_time < datetime('now', 'localtime', '-14 days');
             DELETE FROM topic_observation_hourly WHERE bucket < strftime('%Y-%m-%d 00:00:00', 'now', 'localtime', '-90 days');
             DELETE FROM topic_observation_daily WHERE bucket < date('now', 'localtime', '-365 days');

             DELETE FROM collection_run
             WHERE id NOT IN (SELECT last_success_run_id FROM platform WHERE last_success_run_id IS NOT NULL)
               AND ((status = 'succeeded' AND create_time < datetime('now', 'localtime', '-30 days'))
                 OR (status <> 'succeeded' AND create_time < datetime('now', 'localtime', '-90 days')));

             DELETE FROM creation_queue WHERE deleted <> 0 AND update_time < datetime('now', 'localtime', '-30 days');

             DELETE FROM topic_observation_hourly WHERE topic_id IN (
               SELECT t.id FROM topic t
               LEFT JOIN creation_queue q ON q.topic_id = t.id AND q.deleted = 0
               JOIN platform p ON p.id = t.platform_id
               WHERE q.id IS NULL
                 AND NOT EXISTS (SELECT 1 FROM recent_addition_topic recent WHERE recent.topic_id = t.id)
                 AND t.update_time < datetime('now', 'localtime', '-180 days')
                 AND t.last_collection_run_id <> COALESCE(p.last_success_run_id, -1)
             );
             DELETE FROM topic_observation_daily WHERE topic_id IN (
               SELECT t.id FROM topic t
               LEFT JOIN creation_queue q ON q.topic_id = t.id AND q.deleted = 0
               JOIN platform p ON p.id = t.platform_id
               WHERE q.id IS NULL
                 AND NOT EXISTS (SELECT 1 FROM recent_addition_topic recent WHERE recent.topic_id = t.id)
                 AND t.update_time < datetime('now', 'localtime', '-180 days')
                 AND t.last_collection_run_id <> COALESCE(p.last_success_run_id, -1)
             );
             DELETE FROM topic WHERE id IN (
               SELECT t.id FROM topic t
               LEFT JOIN creation_queue q ON q.topic_id = t.id AND q.deleted = 0
               JOIN platform p ON p.id = t.platform_id
               WHERE q.id IS NULL
                 AND NOT EXISTS (SELECT 1 FROM recent_addition_topic recent WHERE recent.topic_id = t.id)
                 AND t.update_time < datetime('now', 'localtime', '-180 days')
                 AND t.last_collection_run_id <> COALESCE(p.last_success_run_id, -1)
             );",
        )?;
        transaction.commit()?;
        self.connection.execute_batch("PRAGMA optimize;")?;
        Ok(())
    }

    /// Read all platform codes and apply in-memory catalog metadata filters.
    fn filtered_platform_codes(
        &self,
        region: Option<SourceRegion>,
        category: Option<TopicCategory>,
    ) -> AppResult<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT code FROM platform WHERE deleted = 0")?;
        let codes = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(codes
            .into_iter()
            .filter(|code| region.is_none_or(|value| region_for(code) == value))
            .filter(|code| category.is_none_or(|value| category_for(code) == value))
            .collect())
    }

    /// Map the stable columns returned by the topic page query.
    fn map_topic(&self, row: &Row<'_>) -> rusqlite::Result<TopicView> {
        let platform_code: String = row.get(1)?;
        Ok(TopicView {
            id: row.get(0)?,
            category: category_for(&platform_code),
            platform_code,
            platform_name: row.get(2)?,
            title: row.get(3)?,
            url: row.get(4)?,
            published_time: row.get(5)?,
            rank: row.get(6)?,
            heat: row.get(7)?,
            first_seen_at: row.get(8)?,
            updated_at: row.get(9)?,
            global_rank: row.get(10)?,
            queued: row.get::<_, i64>(11)? != 0,
            queued_at: row.get(12)?,
            rank_delta: None,
            consecutive_runs: 0,
            trend: Vec::new(),
        })
    }

    /// Batch rank history and streaks for one page, avoiding two queries per topic.
    fn enrich_topics(&self, topics: &mut [TopicView]) -> AppResult<()> {
        if topics.is_empty() {
            return Ok(());
        }
        let ids = serde_json::to_string(&topics.iter().map(|topic| topic.id).collect::<Vec<_>>())
            .map_err(|error| {
            AppError::Initialization(format!("话题批量查询序列化失败：{error}"))
        })?;
        let mut trends = HashMap::<i64, Vec<i64>>::new();
        let mut statement = self.connection.prepare(
            "SELECT topic_id, rank FROM (
               SELECT topic_id, rank,
                      ROW_NUMBER() OVER (PARTITION BY topic_id ORDER BY create_time DESC, id DESC) AS position
               FROM topic_observation
               WHERE deleted = 0 AND topic_id IN (SELECT value FROM json_each(?))
             ) WHERE position <= 12 ORDER BY topic_id, position DESC",
        )?;
        for row in statement.query_map([&ids], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (topic_id, rank) = row?;
            trends.entry(topic_id).or_default().push(rank);
        }

        let mut streaks = HashMap::<i64, (u32, bool)>::new();
        let mut statement = self.connection.prepare(
            "WITH requested(topic_id) AS (SELECT value FROM json_each(?)),
             recent_runs AS (
               SELECT cr.id, cr.platform_id,
                      ROW_NUMBER() OVER (PARTITION BY cr.platform_id ORDER BY cr.end_time DESC, cr.id DESC) AS position
               FROM collection_run cr
               WHERE cr.status = 'succeeded' AND cr.deleted = 0
             )
             SELECT requested.topic_id, recent_runs.position,
                    EXISTS(SELECT 1 FROM topic_observation observation
                           WHERE observation.collection_run_id = recent_runs.id
                             AND observation.topic_id = requested.topic_id
                             AND observation.deleted = 0)
             FROM requested
             JOIN topic ON topic.id = requested.topic_id
             JOIN recent_runs ON recent_runs.platform_id = topic.platform_id
             WHERE recent_runs.position <= 100
             ORDER BY requested.topic_id, recent_runs.position",
        )?;
        for row in statement.query_map([&ids], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(2)? != 0))
        })? {
            let (topic_id, present) = row?;
            let state = streaks.entry(topic_id).or_default();
            if !state.1 && present {
                state.0 += 1;
            } else if !present {
                state.1 = true;
            }
        }
        for topic in topics {
            topic.trend = trends.remove(&topic.id).unwrap_or_default();
            topic.consecutive_runs = streaks.remove(&topic.id).map_or(0, |state| state.0);
        }
        Ok(())
    }

    /// Return the latest run status and current-topic count for every source.
    fn statuses(&self) -> AppResult<Vec<PlatformStatusView>> {
        let mut statement = self.connection.prepare(
            "SELECT p.code, p.display_name, p.enabled,
               (SELECT cr.status FROM collection_run cr WHERE cr.platform_id = p.id AND cr.deleted = 0 ORDER BY cr.id DESC LIMIT 1),
               (SELECT cr.end_time FROM collection_run cr WHERE cr.platform_id = p.id AND cr.deleted = 0 ORDER BY cr.id DESC LIMIT 1),
               (SELECT cr.error_message FROM collection_run cr WHERE cr.platform_id = p.id AND cr.deleted = 0 ORDER BY cr.id DESC LIMIT 1),
               (SELECT COUNT(*) FROM topic t WHERE t.platform_id = p.id AND t.deleted = 0
                 AND p.last_success_run_id = t.last_collection_run_id)
             FROM platform p WHERE p.deleted = 0 ORDER BY p.id",
        )?;
        let mut statuses = statement
            .query_map([], |row| {
                let code: String = row.get(0)?;
                Ok(PlatformStatusView {
                    region: region_for(&code),
                    category: category_for(&code),
                    code,
                    display_name: row.get(1)?,
                    enabled: row.get::<_, i64>(2)? != 0,
                    status: row.get(3)?,
                    last_run_at: row.get(4)?,
                    error: row.get(5)?,
                    topic_count: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // Keep healthy sources easiest to reach in both settings and filters.
        // Within the same connectivity group, a larger current board is more
        // useful; Rust's stable sort preserves catalog order for exact ties.
        statuses.sort_by(|left, right| {
            source_connectivity_rank(left.status.as_deref())
                .cmp(&source_connectivity_rank(right.status.as_deref()))
                .then_with(|| right.topic_count.cmp(&left.topic_count))
        });
        Ok(statuses)
    }
}

/// A completed successful collection is the only positive connectivity proof;
/// unknown, running and failed states stay in the secondary group.
fn source_connectivity_rank(status: Option<&str>) -> u8 {
    u8::from(status != Some("succeeded"))
}

/// Repair titles irreversibly damaged by the early C114 UTF-8 decoder without rewriting valid first-seen text.
fn should_repair_legacy_title(platform_code: &str, stored: &str, incoming: &str) -> bool {
    platform_code == "c114" && stored.contains('\u{fffd}') && !incoming.contains('\u{fffd}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CollectedTopic;

    /// Build the smallest real repository fixture using the production schema.
    fn fixture() -> Connection {
        let connection = Connection::open_in_memory().expect("database should open");
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("schema should initialize");
        connection
            .execute(
                "INSERT INTO platform (code, display_name, home_url, feed_url)
                 VALUES ('qbitai', '量子位', 'https://www.qbitai.com/', 'https://www.qbitai.com/feed')",
                [],
            )
            .expect("platform should insert");
        connection
    }

    #[test]
    fn source_statuses_put_reachable_and_larger_boards_first() {
        let connection = fixture();
        for (code, name) in [
            ("sspai", "少数派"),
            ("hackernews", "Hacker News"),
            ("ithome", "IT之家"),
        ] {
            connection
                .execute(
                    "INSERT INTO platform (code, display_name, home_url, feed_url)
                     VALUES (?, ?, 'https://example.com/', 'https://example.com/feed')",
                    params![code, name],
                )
                .expect("platform should insert");
        }
        connection
            .execute_batch(
                "INSERT INTO collection_run (platform_id, status, trigger_kind, scheduled_time, start_time, end_time)
                 SELECT id, 'succeeded', 'manual', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP FROM platform WHERE code = 'qbitai';
                 INSERT INTO collection_run (platform_id, status, trigger_kind, scheduled_time, start_time, end_time)
                 SELECT id, 'succeeded', 'manual', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP FROM platform WHERE code = 'sspai';
                 INSERT INTO collection_run (platform_id, status, trigger_kind, scheduled_time, start_time, end_time)
                 SELECT id, 'failed', 'manual', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP FROM platform WHERE code = 'hackernews';",
            )
            .expect("runs should insert");

        let repository = TopicRepository::new(&connection);
        let mut statuses = repository.statuses().expect("statuses should load");
        // Supply representative board sizes directly so this unit test focuses
        // on the ordering contract rather than feed-commit mechanics.
        for status in &mut statuses {
            status.topic_count = match status.code.as_str() {
                "sspai" => 30,
                "qbitai" => 10,
                "hackernews" => 100,
                "ithome" => 50,
                _ => 0,
            };
        }
        statuses.sort_by(|left, right| {
            source_connectivity_rank(left.status.as_deref())
                .cmp(&source_connectivity_rank(right.status.as_deref()))
                .then_with(|| right.topic_count.cmp(&left.topic_count))
        });

        assert_eq!(
            statuses
                .iter()
                .map(|status| status.code.as_str())
                .collect::<Vec<_>>(),
            vec!["sspai", "qbitai", "hackernews", "ithome"]
        );
    }

    /// Feed commits must populate current topics, observations and run counters together.
    #[test]
    fn commits_and_lists_feed_atomically() {
        let connection = fixture();
        let repository = TopicRepository::new(&connection);
        let platform = repository
            .enabled_platforms()
            .expect("platforms should load")
            .remove(0);
        let run_id = repository
            .create_run(platform.id, "manual")
            .expect("run should start");
        let stats = repository
            .commit_feed(
                &platform,
                run_id,
                &ParsedFeed {
                    topics: vec![CollectedTopic {
                        platform_code: "qbitai".into(),
                        stable_id: Some("story-1".into()),
                        title: "一个可测试的话题".into(),
                        url: "https://www.qbitai.com/story-1".into(),
                        published_time: None,
                        rank: 1,
                        heat: Some(42.0),
                    }],
                    fetched_count: 1,
                    invalid_count: 0,
                },
            )
            .expect("feed should commit");
        assert_eq!(stats.inserted, 1);
        let page = repository
            .list(&TopicQuery::default())
            .expect("page should load");
        assert_eq!(page.total, 1);
        assert_eq!(page.topics[0].consecutive_runs, 1);
        assert_eq!(page.topics[0].trend, vec![1]);
        let searched = repository
            .list(&TopicQuery {
                search: Some("可测试".into()),
                ..TopicQuery::default()
            })
            .expect("trigram title search should load");
        assert_eq!(searched.total, 1);
    }

    /// A fourth non-empty collection batch evicts only the oldest recent-addition batch.
    #[test]
    fn retains_three_recent_addition_batches() {
        let connection = fixture();
        let repository = TopicRepository::new(&connection);
        let platform = repository.enabled_platforms().unwrap().remove(0);
        let mut inserted_ids = Vec::new();
        for batch in 1..=4 {
            let run_id = repository.create_run(platform.id, "test").unwrap();
            let stats = repository
                .commit_feed(
                    &platform,
                    run_id,
                    &ParsedFeed {
                        topics: vec![CollectedTopic {
                            platform_code: "qbitai".into(),
                            stable_id: Some(format!("recent-{batch}")),
                            title: format!("最近新增 {batch}"),
                            url: format!("https://www.qbitai.com/recent-{batch}"),
                            published_time: None,
                            rank: 1,
                            heat: None,
                        }],
                        fetched_count: 1,
                        invalid_count: 0,
                    },
                )
                .unwrap();
            repository
                .record_recent_additions("test", &stats.inserted_topic_ids)
                .unwrap();
            inserted_ids.extend(stats.inserted_topic_ids);
        }

        let recent = repository
            .list(&TopicQuery {
                recent_only: Some(true),
                limit: Some(100),
                ..TopicQuery::default()
            })
            .unwrap();
        assert_eq!(recent.total, 3);
        assert_eq!(recent.recent_total, 3);
        assert!(!recent
            .topics
            .iter()
            .any(|topic| topic.id == inserted_ids[0]));
        let batch_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM recent_addition_batch", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(batch_count, 3);
    }

    /// Maintenance rolls old raw observations up before applying retention windows.
    #[test]
    fn rolls_up_and_prunes_operational_history() {
        let connection = fixture();
        let repository = TopicRepository::new(&connection);
        let platform = repository.enabled_platforms().unwrap().remove(0);
        let run_id = repository.create_run(platform.id, "manual").unwrap();
        repository
            .commit_feed(
                &platform,
                run_id,
                &ParsedFeed {
                    topics: vec![CollectedTopic {
                        platform_code: "qbitai".into(),
                        stable_id: Some("old-story".into()),
                        title: "历史趋势样本".into(),
                        url: "https://www.qbitai.com/old-story".into(),
                        published_time: None,
                        rank: 3,
                        heat: Some(10.0),
                    }],
                    fetched_count: 1,
                    invalid_count: 0,
                },
            )
            .unwrap();
        connection
            .execute(
                "UPDATE topic_observation SET create_time = datetime('now', '-20 days')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO collection_run(platform_id, status, trigger_kind, scheduled_time,
                   start_time, end_time, create_time, update_time)
                 VALUES(?1, 'succeeded', 'test', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP,
                   CURRENT_TIMESTAMP, datetime('now', '-40 days'), datetime('now', '-40 days'))",
                [platform.id],
            )
            .unwrap();

        repository
            .maintain_storage()
            .expect("maintenance should succeed");

        let raw: i64 = connection
            .query_row("SELECT COUNT(*) FROM topic_observation", [], |row| {
                row.get(0)
            })
            .unwrap();
        let daily: i64 = connection
            .query_row("SELECT COUNT(*) FROM topic_observation_daily", [], |row| {
                row.get(0)
            })
            .unwrap();
        let old_runs: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM collection_run WHERE trigger_kind = 'test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw, 0);
        assert_eq!(daily, 1);
        assert_eq!(old_runs, 0);
    }

    /// An explicit empty ID selection represents an empty new-topic result, never all topics.
    #[test]
    fn empty_topic_id_filter_returns_no_rows() {
        let connection = fixture();
        let repository = TopicRepository::new(&connection);
        let page = repository
            .list(&TopicQuery {
                topic_ids: Some(Vec::new()),
                ..TopicQuery::default()
            })
            .expect("empty filter should be valid");
        assert_eq!(page.total, 0);
    }

    /// Model credentials stay in their dedicated table and only presence reaches settings views.
    #[test]
    fn saves_and_reads_model_credential() {
        let connection = fixture();
        let repository = TopicRepository::new(&connection);

        let encrypted = EncryptedCredential {
            nonce: vec![1; 12],
            ciphertext: vec![2; 32],
        };
        repository
            .save_model_settings("https://api.example.com", "example-model", Some(&encrypted))
            .expect("model settings should save");

        let settings = repository
            .model_settings()
            .expect("model settings should load");
        assert_eq!(settings.endpoint, "https://api.example.com");
        assert_eq!(settings.model, "example-model");
        assert!(settings.has_api_key);
        assert_eq!(
            repository
                .encrypted_api_key()
                .expect("encrypted API key should load")
                .expect("encrypted API key should exist")
                .ciphertext,
            encrypted.ciphertext
        );
    }

    /// UI choices persist in SQLite because the main WebView intentionally has no durable store.
    #[test]
    fn saves_and_reads_ui_preferences() {
        let connection = fixture();
        let repository = TopicRepository::new(&connection);

        assert_eq!(
            repository
                .ui_preferences()
                .expect("empty preferences should load"),
            UiPreferences {
                locale: None,
                theme: None,
            }
        );
        repository
            .save_ui_preferences(UiLocale::En, UiTheme::Dark)
            .expect("UI preferences should save");
        assert_eq!(
            repository
                .ui_preferences()
                .expect("saved preferences should load"),
            UiPreferences {
                locale: Some(UiLocale::En),
                theme: Some(UiTheme::Dark),
            }
        );
    }

    #[test]
    fn saves_and_clears_network_proxy() {
        let connection = fixture();
        let repository = TopicRepository::new(&connection);

        assert_eq!(
            repository.network_settings().expect("settings should load"),
            NetworkSettings {
                proxy_url: Some("http://127.0.0.1:7897".into())
            }
        );
        repository
            .save_network_settings(Some("http://127.0.0.1:7897"))
            .expect("proxy should save");
        assert_eq!(
            repository.network_settings().expect("settings should load"),
            NetworkSettings {
                proxy_url: Some("http://127.0.0.1:7897".into())
            }
        );
        repository
            .save_network_settings(None)
            .expect("proxy should clear");
        assert_eq!(
            repository.network_settings().expect("settings should load"),
            NetworkSettings { proxy_url: None }
        );
    }

    /// Only a clean C114 recollection may replace a legacy title containing decoding loss.
    #[test]
    fn repairs_only_corrupted_c114_titles() {
        assert!(should_repair_legacy_title(
            "c114",
            "�й�ͨ������",
            "中国通信行业"
        ));
        assert!(!should_repair_legacy_title(
            "c114",
            "首次正常标题",
            "后续正常标题"
        ));
        assert!(!should_repair_legacy_title("qbitai", "�Ƽ�����", "科技新闻"));
    }

    /// A clean recollection repairs an existing C114 row while preserving its identity and history.
    #[test]
    fn repairs_corrupted_c114_title_during_commit() {
        let connection = fixture();
        connection
            .execute(
                "UPDATE platform SET code = 'c114', display_name = 'C114通信'",
                [],
            )
            .expect("platform should update");
        let repository = TopicRepository::new(&connection);
        let platform = repository
            .enabled_platforms()
            .expect("platforms should load")
            .remove(0);
        for (title, trigger) in [("�й�ͨ������", "legacy"), ("中国通信行业", "repair")]
        {
            let run_id = repository
                .create_run(platform.id, trigger)
                .expect("run should start");
            repository
                .commit_feed(
                    &platform,
                    run_id,
                    &ParsedFeed {
                        topics: vec![CollectedTopic {
                            platform_code: "c114".into(),
                            stable_id: Some("article-1".into()),
                            title: title.into(),
                            url: "https://www.c114.com.cn/news/16/a1.html".into(),
                            published_time: None,
                            rank: 1,
                            heat: None,
                        }],
                        fetched_count: 1,
                        invalid_count: 0,
                    },
                )
                .expect("feed should commit");
        }
        let repaired: String = connection
            .query_row("SELECT title FROM topic", [], |row| row.get(0))
            .expect("topic should exist");
        assert_eq!(repaired, "中国通信行业");
    }
}
