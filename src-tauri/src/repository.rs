//! SQLite repository for topic pages, source health and persistent creation queue.

use rusqlite::{params, Connection, OptionalExtension, Row, ToSql};

use crate::error::{AppError, AppResult};
use crate::identity::identify_topic;
use crate::models::{
    CollectionStats, ModelSettings, ParsedFeed, PlatformSource, PlatformStatusView, SourceRegion,
    TopicCategory, TopicPage, TopicQuery, TopicSort, TopicView,
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
        if query.queued_only.unwrap_or(false) {
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
            where_parts.push("t.title LIKE ? ESCAPE '\\'");
            let escaped = search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            owned_values.push(Box::new(format!("%{escaped}%")));
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
        for topic in &mut topics {
            topic.trend = self.trend(topic.id)?;
            topic.rank_delta = topic
                .trend
                .iter()
                .rev()
                .nth(1)
                .map(|previous| previous - topic.rank);
            topic.consecutive_runs = self.consecutive_runs(topic.id, &topic.platform_code)?;
        }

        let queued_total: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM creation_queue cq
             JOIN topic t ON t.id = cq.topic_id
             JOIN platform p ON p.id = t.platform_id
             WHERE cq.deleted = 0 AND t.deleted = 0 AND p.deleted = 0",
            [],
            |row| row.get(0),
        )?;

        Ok(TopicPage {
            topics,
            total,
            queued_total,
            statuses: self.statuses()?,
            history_enabled: true,
        })
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
                 VALUES (?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                 ON CONFLICT(topic_id) DO UPDATE SET deleted = 0,
                   create_time = excluded.create_time, update_time = excluded.update_time",
                [topic_id],
            )?;
        } else {
            self.connection.execute(
                "UPDATE creation_queue SET deleted = 1, update_time = CURRENT_TIMESTAMP
                 WHERE topic_id = ? AND deleted = 0",
                [topic_id],
            )?;
        }
        Ok(())
    }

    /// Persist a user's source toggle; catalog synchronization intentionally preserves this value.
    pub fn set_platform_enabled(&self, code: &str, enabled: bool) -> AppResult<()> {
        let changed = self.connection.execute(
            "UPDATE platform SET enabled = ?, update_time = CURRENT_TIMESTAMP
             WHERE code = ? AND deleted = 0",
            params![enabled, code],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(format!("未知来源：{code}")));
        }
        Ok(())
    }

    /// Load non-secret model routing; credential presence is filled by the credential service.
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
            has_api_key: false,
        })
    }

    /// Persist only non-secret routing values; secrets never enter SQLite.
    pub fn save_model_settings(&self, endpoint: &str, model: &str) -> AppResult<()> {
        for (key, value) in [("model_endpoint", endpoint), ("model_name", model)] {
            self.connection.execute(
                "INSERT INTO app_setting (key, value, update_time)
                 VALUES (?, ?, CURRENT_TIMESTAMP)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                   update_time = CURRENT_TIMESTAMP",
                params![key, value],
            )?;
        }
        Ok(())
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
               platform_id, status, trigger_kind, scheduled_time, start_time, update_time
             ) VALUES (?, 'running', ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![platform_id, trigger],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Mark a failed source without affecting successful results from other sources.
    pub fn fail_run(&self, run_id: i64, message: &str) -> AppResult<()> {
        self.connection.execute(
            "UPDATE collection_run SET status = 'failed', end_time = CURRENT_TIMESTAMP,
               error_message = ?, update_time = CURRENT_TIMESTAMP WHERE id = ?",
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
                           deleted = 0, update_time = CURRENT_TIMESTAMP WHERE id = ?",
                        params![topic.title, topic.rank, topic.heat, run_id, id],
                    )?;
                } else {
                    transaction.execute(
                        "UPDATE topic SET rank = ?, heat = ?, last_collection_run_id = ?,
                           deleted = 0, update_time = CURRENT_TIMESTAMP WHERE id = ?",
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
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
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
                 ) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                 ON CONFLICT(collection_run_id, topic_id) DO UPDATE SET
                   rank = excluded.rank, heat = excluded.heat, deleted = 0,
                   update_time = CURRENT_TIMESTAMP",
                params![topic_id, run_id, topic.rank, topic.heat],
            )?;
        }

        transaction.execute(
            "UPDATE collection_run SET status = 'succeeded', end_time = CURRENT_TIMESTAMP,
               fetched_count = ?, inserted_count = ?, updated_count = ?, invalid_count = ?,
               error_message = NULL, update_time = CURRENT_TIMESTAMP WHERE id = ?",
            params![
                feed.fetched_count,
                stats.inserted,
                stats.updated,
                feed.invalid_count,
                run_id
            ],
        )?;
        transaction.execute(
            "UPDATE platform SET last_success_run_id = ?, update_time = CURRENT_TIMESTAMP WHERE id = ?",
            params![run_id, platform.id],
        )?;
        transaction.commit()?;
        Ok(stats)
    }

    /// Bound ranking history so a long-running desktop install stays compact.
    pub fn clean_observations(&self, retention_days: u32) -> AppResult<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        Ok(self.connection.execute(
            "DELETE FROM topic_observation
             WHERE create_time < datetime('now', '-' || ? || ' days')",
            [retention_days],
        )?)
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

    /// Return up to twelve chronological rank observations for a sparkline.
    fn trend(&self, topic_id: i64) -> AppResult<Vec<i64>> {
        let mut statement = self.connection.prepare(
            "SELECT rank FROM topic_observation WHERE topic_id = ? AND deleted = 0
             ORDER BY create_time DESC, id DESC LIMIT 12",
        )?;
        let mut values = statement
            .query_map([topic_id], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        values.reverse();
        Ok(values)
    }

    /// Count the uninterrupted suffix of successful runs containing this topic.
    fn consecutive_runs(&self, topic_id: i64, platform_code: &str) -> AppResult<u32> {
        let mut statement = self.connection.prepare(
            "SELECT EXISTS(
               SELECT 1 FROM topic_observation o
               WHERE o.collection_run_id = cr.id AND o.topic_id = ? AND o.deleted = 0
             )
             FROM collection_run cr JOIN platform p ON p.id = cr.platform_id
             WHERE p.code = ? AND cr.status = 'succeeded' AND cr.deleted = 0
             ORDER BY cr.end_time DESC, cr.id DESC LIMIT 100",
        )?;
        let present = statement
            .query_map(params![topic_id, platform_code], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(present.into_iter().take_while(|value| *value != 0).count() as u32)
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
        let statuses = statement
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
        Ok(statuses)
    }
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
