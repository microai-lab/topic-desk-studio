//! Validate note permalinks without stripping publisher-supplied access parameters.
use url::Url;

/// Accept only the exact publisher host and a matching stable note ID. Access
/// parameters affect reading, never topic identity or the first-observed URL.
pub(crate) fn note_link(value: &str, id: &str) -> Option<String> {
    if id.len() != 24 || !id.bytes().all(|v| v.is_ascii_hexdigit()) || value.len() > 4096 {
        return None;
    }
    let mut url = Url::parse(value).ok()?;
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some("xiaohongshu.com" | "www.xiaohongshu.com")
        )
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let path_id = url
        .path()
        .trim_end_matches('/')
        .strip_prefix("/explore/")
        .or_else(|| {
            url.path()
                .trim_end_matches('/')
                .strip_prefix("/discovery/item/")
        })?;
    if !path_id.eq_ignore_ascii_case(id) {
        return None;
    }
    let mut parameters = Vec::new();
    for (key, value) in url.query_pairs() {
        if !matches!(key.as_ref(), "xsec_token" | "xsec_source") {
            continue;
        }
        if value.is_empty()
            || value.len() > 2048
            || value.chars().any(char::is_control)
            || parameters
                .iter()
                .any(|(existing, _): &(String, String)| existing == key.as_ref())
        {
            return None;
        }
        parameters.push((key.into_owned(), value.into_owned()));
    }
    url.set_host(Some("www.xiaohongshu.com")).ok()?;
    url.set_path(&format!("/explore/{}", id.to_ascii_lowercase()));
    url.set_query(None);
    url.set_fragment(None);
    if !parameters.is_empty() {
        url.query_pairs_mut().extend_pairs(parameters);
    }
    Some(url.into())
}
/// Only signed links can replace the separate latest reading address.
pub(crate) fn signed_note_link(value: &str, id: &str) -> Option<String> {
    let link = note_link(value, id)?;
    Url::parse(&link)
        .ok()?
        .query_pairs()
        .any(|(key, _)| key == "xsec_token")
        .then_some(link)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "0123456789abcdef01234567";
    #[test]
    fn preserves_access_parameters_but_not_tracking_or_fragments() {
        let raw = format!("https://www.xiaohongshu.com/explore/{ID}?xsec_token=fake%2Baccess%3D&xsec_source=pc_feed&utm_source=test#x");
        let result = signed_note_link(&raw, ID).unwrap();
        let url = Url::parse(&result).unwrap();
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![
                ("xsec_token".into(), "fake+access=".into()),
                ("xsec_source".into(), "pc_feed".into())
            ]
        );
        assert!(url.fragment().is_none());
        assert!(
            signed_note_link(&format!("https://www.xiaohongshu.com/explore/{ID}"), ID).is_none()
        );
    }
    #[test]
    fn rejects_mismatched_ids_lookalikes_credentials_and_ambiguous_parameters() {
        for raw in [
            format!("http://www.xiaohongshu.com/explore/{ID}"),
            format!("https://www.xiaohongshu.com.evil.org/explore/{ID}"),
            format!("https://user:pass@www.xiaohongshu.com/explore/{ID}"),
            format!("https://www.xiaohongshu.com/explore/{ID}?xsec_token=a&xsec_token=b"),
            format!("https://www.xiaohongshu.com/explore/{ID}?xsec_token=%0A"),
            "javascript:alert(1)".into(),
        ] {
            assert!(note_link(&raw, ID).is_none());
        }
        assert!(note_link(
            &format!("https://www.xiaohongshu.com/explore/{ID}"),
            "ffffffffffffffffffffffff"
        )
        .is_none());
        assert!(note_link(&format!("https://www.xiaohongshu.com/explore/{ID}"), "bad").is_none());
    }
}
