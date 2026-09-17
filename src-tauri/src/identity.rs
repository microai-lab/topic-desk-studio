//! Stable topic identity normalization, kept compatible with the original plugin.

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;
use url::Url;

use crate::error::{AppError, AppResult};
use crate::models::{CollectedTopic, TopicIdentity};

const DEDUPE_VERSION: u8 = 1;
const TRACKING_PARAMETERS: &[&str] = &["fbclid", "gclid", "spm", "from", "source"];

/// Normalize user-visible identity text using NFKC, collapsed whitespace and lowercase.
fn normalized_text(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Normalize an HTTP(S) URL while removing known tracking parameters and fragments.
pub fn normalize_url(value: &str) -> AppResult<String> {
    let mut url = Url::parse(value)
        .map_err(|error| AppError::InvalidInput(format!("话题 URL 无效：{error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::InvalidInput("话题 URL 必须使用 HTTP(S)".into()));
    }

    url.set_fragment(None);
    let mut parameters = url
        .query_pairs()
        .filter(|(key, _)| {
            let normalized = key.to_ascii_lowercase();
            !normalized.starts_with("utm_") && !TRACKING_PARAMETERS.contains(&normalized.as_str())
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    parameters.sort();
    url.set_query(None);
    if !parameters.is_empty() {
        url.query_pairs_mut().extend_pairs(parameters);
    }
    if url.path() != "/" {
        let trimmed = url.path().trim_end_matches('/').to_owned();
        url.set_path(&trimmed);
    }
    Ok(url.to_string())
}

/// Build the versioned, platform-scoped SHA-256 identity used by SQLite deduplication.
pub fn identify_topic(topic: &CollectedTopic) -> AppResult<TopicIdentity> {
    let stable_id = normalized_text(topic.stable_id.as_deref().unwrap_or_default());
    let (kind, source_key) = if stable_id.is_empty() {
        match normalize_url(&topic.url) {
            Ok(url) => ("url", url),
            Err(_) => ("title", normalized_text(&topic.title)),
        }
    } else {
        ("stable_id", stable_id)
    };
    if source_key.is_empty() {
        return Err(AppError::InvalidInput("话题稳定身份不能为空".into()));
    }

    let input = format!(
        "v{DEDUPE_VERSION}\0{}\0{kind}\0{source_key}",
        topic.platform_code
    );
    let hash: [u8; 32] = Sha256::digest(input.as_bytes()).into();
    Ok(TopicIdentity {
        kind,
        source_key,
        version: DEDUPE_VERSION,
        hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tracking keys and fragments must not produce distinct topic identities.
    #[test]
    fn normalizes_tracking_parameters() {
        let normalized = normalize_url("https://Example.com/story/?utm_source=x&b=2&a=1#top")
            .expect("URL should be valid");
        assert_eq!(normalized, "https://example.com/story?a=1&b=2");
    }

    /// Stable source IDs take precedence over mutable URLs and titles.
    #[test]
    fn stable_id_has_priority() {
        let identity = identify_topic(&CollectedTopic {
            platform_code: "example".into(),
            stable_id: Some("  ITEM-1 ".into()),
            title: "A title".into(),
            url: "https://example.com/changed".into(),
            published_time: None,
            rank: 1,
            heat: None,
        })
        .expect("identity should be valid");
        assert_eq!(identity.kind, "stable_id");
        assert_eq!(identity.source_key, "item-1");
        assert_eq!(identity.hash.len(), 32);
    }
}
