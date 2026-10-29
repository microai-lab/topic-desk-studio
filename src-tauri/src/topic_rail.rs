//! Unprivileged article overlays: validate display data and route only HTTP(S) popup requests.
use serde::{Deserialize, Serialize};

/// Public topic display data only; no settings, credentials or main-WebView APIs enter the page.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RailTopic {
    pub id: i64,
    pub title: String,
    pub platform_name: String,
    pub url: String,
}

fn valid_topic(topic: &RailTopic) -> bool {
    topic.id > 0
        && !topic.title.trim().is_empty()
        && topic.title.len() <= 8192
        && topic.platform_name.len() <= 512
        && valid_url(&topic.url)
}

fn valid_url(value: &str) -> bool {
    value.len() <= 16384
        && url::Url::parse(value).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && !matches!(url.host_str(), Some("tauri.localhost" | "ipc.localhost"))
                && url.username().is_empty()
                && url.password().is_none()
        })
}

/// Build a fixed, audited script from JSON data; callers cannot supply JavaScript.
pub(crate) fn script(
    topics: &[RailTopic],
    active: Option<i64>,
    locale: &str,
    theme: &str,
    visible: bool,
) -> Result<String, String> {
    if topics.len() > 200
        || topics.iter().any(|topic| !valid_topic(topic))
        || !matches!(locale, "zh" | "en")
        || !matches!(theme, "light" | "dark" | "system")
    {
        return Err("话题预览数据无效".into());
    }
    let data = serde_json::json!({ "topics": topics, "active": active, "locale": locale, "theme": theme, "visible": visible });
    Ok(format!("({})({data})", include_str!("page_topic_rail.js")))
}

/// Dispatch display-only work without requesting or waiting for a JavaScript result.
/// A navigating or busy page cannot hold the shared browser command queue for ten seconds.
pub(crate) fn update(
    topics: &[RailTopic],
    active: Option<i64>,
    locale: &str,
    theme: &str,
    visible: bool,
    evaluate: impl FnOnce(String) -> Result<(), String>,
) -> Result<(), String> {
    evaluate(script(topics, active, locale, theme, visible)?)
}

/// Pages may spoof popup requests. Treat them as untrusted navigation only, never as privileged commands.
pub(crate) fn popup(url: &url::Url) -> Option<RailTopic> {
    if url.scheme() != "topic-desk-topic"
        || url.host_str() != Some("open")
        || url.as_str().len() > 32768
    {
        return None;
    }
    let data = url.query_pairs().find(|(key, _)| key == "topic")?.1;
    let topic: RailTopic = serde_json::from_str(&data).ok()?;
    valid_topic(&topic).then_some(topic)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn topic() -> RailTopic {
        RailTopic {
            id: 1,
            title: "<script>untrusted</script>".into(),
            platform_name: "AI News".into(),
            url: "https://example.com/a#original".into(),
        }
    }
    #[test]
    fn display_updates_finish_without_any_page_response_and_keep_dispatch_failures_visible() {
        // The fake native queue accepts the script but never delivers a page callback.
        let (reply, pending) = std::sync::mpsc::channel::<()>();
        let mut queued = Vec::new();
        update(&[topic()], Some(1), "zh", "light", true, |script| {
            queued.push(script);
            Ok(())
        })
        .unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(
            pending.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        );
        drop(reply);
        assert_eq!(
            update(&[], None, "en", "dark", false, |_| Err(
                "native dispatch failed".into()
            )),
            Err("native dispatch failed".into())
        );
        let mut dispatched = false;
        assert!(update(&[], None, "invalid", "light", true, |_| {
            dispatched = true;
            Ok(())
        })
        .is_err());
        assert!(!dispatched);
    }

    #[test]
    fn display_data_is_json_and_navigation_preserves_original_urls() {
        let value = topic();
        let script = script(std::slice::from_ref(&value), Some(1), "zh", "system", true).unwrap();
        assert!(script.contains("textContent"));
        let mut request = url::Url::parse("topic-desk-topic://open").unwrap();
        request
            .query_pairs_mut()
            .append_pair("topic", &serde_json::to_string(&value).unwrap());
        let opened = popup(&request).unwrap();
        assert_eq!(opened.id, 1);
        assert_eq!(opened.url, value.url);
        assert_eq!(opened.title, value.title);
    }
    #[test]
    fn rejects_invalid_display_and_privileged_or_non_web_navigation() {
        for target in [
            "javascript:alert(1)",
            "file:///tmp/private",
            "https://tauri.localhost",
            "http://ipc.localhost",
            "https://name:secret@example.com",
        ] {
            let mut value = topic();
            value.url = target.into();
            assert!(script(std::slice::from_ref(&value), None, "zh", "light", true).is_err());
            let mut request = url::Url::parse("topic-desk-topic://open").unwrap();
            request
                .query_pairs_mut()
                .append_pair("topic", &serde_json::to_string(&value).unwrap());
            assert!(popup(&request).is_none());
        }
        assert!(script(&vec![topic(); 201], None, "zh", "light", true).is_err());
        assert!(script(&[], None, "invalid", "light", false).is_err());
        assert!(script(&[], None, "en", "invalid", false).is_err());
        assert!(popup(&url::Url::parse("https://example.com").unwrap()).is_none());
        assert!(popup(&url::Url::parse("topic-desk-topic://open?topic=bad").unwrap()).is_none());
        let mut value = topic();
        value.id = 0;
        assert!(script(&[value], None, "en", "dark", true).is_err());
    }
}
