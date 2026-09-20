//! Serializable domain contracts shared with the React command client.

use serde::{Deserialize, Serialize};

/// Region groups sources by organization origin rather than feed language.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceRegion {
    Domestic,
    International,
}

/// Product-facing source categories used by filters and status summaries.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TopicCategory {
    General,
    Technology,
    Finance,
    Developer,
}

/// Supported deterministic ordering choices for topic pages.
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TopicSort {
    #[default]
    Rank,
    Updated,
}

/// Validated query request received from the WebView.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TopicQuery {
    pub source: Option<String>,
    pub region: Option<SourceRegion>,
    pub category: Option<TopicCategory>,
    pub search: Option<String>,
    pub sort: Option<TopicSort>,
    pub queued_only: Option<bool>,
    pub topic_ids: Option<Vec<i64>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Topic row enriched with ranking history and queue state for presentation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicView {
    pub id: i64,
    pub platform_code: String,
    pub platform_name: String,
    pub category: TopicCategory,
    pub title: String,
    pub url: String,
    pub published_time: Option<String>,
    pub global_rank: i64,
    pub rank: i64,
    pub heat: Option<f64>,
    pub first_seen_at: String,
    pub updated_at: String,
    pub rank_delta: Option<i64>,
    pub consecutive_runs: u32,
    pub trend: Vec<i64>,
    pub queued: bool,
    pub queued_at: Option<String>,
}

/// Latest health information for one configured source.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformStatusView {
    pub code: String,
    pub display_name: String,
    pub region: SourceRegion,
    pub category: TopicCategory,
    pub enabled: bool,
    pub status: Option<String>,
    pub last_run_at: Option<String>,
    pub error: Option<String>,
    pub topic_count: i64,
}

/// Paginated topic response returned to the React application.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicPage {
    pub topics: Vec<TopicView>,
    pub total: i64,
    pub queued_total: i64,
    pub statuses: Vec<PlatformStatusView>,
    pub history_enabled: bool,
}

/// Summary returned after a controlled collection request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub accepted: bool,
    pub message: String,
    pub inserted: u32,
    pub updated: u32,
    pub inserted_topic_ids: Vec<i64>,
}

/// Normalized topic input used by identity and persistence services.
#[derive(Debug, Clone)]
pub struct CollectedTopic {
    pub platform_code: String,
    pub stable_id: Option<String>,
    pub title: String,
    pub url: String,
    pub published_time: Option<String>,
    pub rank: u32,
    pub heat: Option<f64>,
}

/// Parser output plus diagnostics persisted on the collection run.
#[derive(Debug, Clone)]
pub struct ParsedFeed {
    pub topics: Vec<CollectedTopic>,
    pub fetched_count: u32,
    pub invalid_count: u32,
}

/// Enabled platform values loaded from SQLite before network work begins.
#[derive(Debug, Clone)]
pub struct PlatformSource {
    pub id: i64,
    pub code: String,
    pub endpoint_url: String,
}

/// Counters accumulated across independently isolated platform collections.
#[derive(Debug, Clone, Default)]
pub struct CollectionStats {
    pub inserted: u32,
    pub updated: u32,
    pub inserted_topic_ids: Vec<i64>,
}

/// Model routing plus a boolean credential-presence indicator for the settings UI.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSettings {
    pub endpoint: String,
    pub model: String,
    pub has_api_key: bool,
}

/// Settings update; an omitted API key preserves the credential already stored in SQLite.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveModelSettings {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// Supported interface languages stored as native application settings.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UiLocale {
    Zh,
    En,
}

impl UiLocale {
    /// Return the stable SQLite representation shared across application versions.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }
}

/// Supported appearance choices stored outside the temporary main WebView.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UiTheme {
    Light,
    Dark,
    System,
}

impl UiTheme {
    /// Return the stable SQLite representation shared across application versions.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

/// Optional persisted choices let a first run retain browser-language defaults.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferences {
    pub locale: Option<UiLocale>,
    pub theme: Option<UiTheme>,
}

/// Complete validated preference update received from the trusted main interface.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveUiPreferences {
    pub locale: UiLocale,
    pub theme: UiTheme,
}

/// Optional application-wide proxy used only by native network clients.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettings {
    pub proxy_url: Option<String>,
}

/// A blank proxy value explicitly restores direct network access.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveNetworkSettings {
    pub proxy_url: String,
}

/// One generated translation returned to the topic card.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResult {
    pub topic_id: i64,
    pub translation: String,
}

/// Auditable identity material and digest for a collected topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicIdentity {
    pub kind: &'static str,
    pub source_key: String,
    pub version: u8,
    pub hash: [u8; 32],
}
