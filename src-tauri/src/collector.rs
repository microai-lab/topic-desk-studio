//! Network collection, protocol parsing and bounded cross-platform coordination.

use std::collections::{HashSet, VecDeque};
use std::error::Error as _;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use encoding_rs::GBK;
use regex::Regex;
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, REFERER, USER_AGENT};
use serde_json::Value;
use time::{Duration as TimeDuration, OffsetDateTime};
use url::Url;

use crate::database::open_database;
use crate::error::{AppError, AppResult};
use crate::models::{CollectedTopic, CollectionStats, ParsedFeed, PlatformSource};
use crate::repository::TopicRepository;

const ITEMS_PER_PLATFORM: usize = 50;
const REQUEST_TIMEOUT_SECONDS: u64 = 15;
const MAX_WORKERS: usize = 6;
const USER_AGENT_VALUE: &str =
    "TopicDeskStudio/0.1 (+https://github.com/dataelement/topic-desk-studio)";

/// Collect every enabled source with bounded concurrency and serial transactional persistence.
pub fn collect_all(database_path: &Path, trigger: &str) -> AppResult<CollectionStats> {
    let connection = open_database(database_path)?;
    let repository = TopicRepository::new(&connection);
    let platforms = repository.enabled_platforms()?;
    if platforms.is_empty() {
        return Err(AppError::Collection("没有启用的数据来源".into()));
    }

    let mut jobs = VecDeque::new();
    for platform in platforms
        .into_iter()
        // Xiaohongshu's signed feed is collected explicitly from the user's
        // ephemeral logged-in browser session, never by replaying credentials.
        .filter(|platform| platform.code != "xiaohongshu")
    {
        let run_id = repository.create_run(platform.id, trigger)?;
        jobs.push_back((platform, run_id));
    }
    let job_count = jobs.len();
    if job_count == 0 {
        return Err(AppError::Collection("没有可自动采集的数据来源".into()));
    }

    let network_settings = repository.network_settings()?;
    let direct_client = Client::builder()
        .connect_timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .redirect(reqwest::redirect::Policy::limited(8))
        .no_proxy()
        .build()
        .map_err(collection_error)?;
    let proxy_client = network_settings
        .proxy_url
        .map(|proxy_url| {
            let proxy = reqwest::Proxy::all(&proxy_url)
                .map_err(|error| AppError::Collection(format!("代理配置无效：{error}")))?;
            Client::builder()
                .connect_timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
                .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
                .redirect(reqwest::redirect::Policy::limited(8))
                .proxy(proxy)
                .build()
                .map_err(collection_error)
        })
        .transpose()?;
    let jobs = Arc::new(Mutex::new(jobs));
    let (sender, receiver) = mpsc::channel();
    let worker_count = MAX_WORKERS.min(job_count);

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let jobs = Arc::clone(&jobs);
            let sender = sender.clone();
            let direct_client = direct_client.clone();
            let proxy_client = proxy_client.clone();
            scope.spawn(move || loop {
                let job = jobs.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some((platform, run_id)) = job else { break };
                let client = if source_requires_proxy(&platform.code) {
                    proxy_client.as_ref().unwrap_or(&direct_client)
                } else {
                    &direct_client
                };
                let result = fetch_source(client, &platform);
                if sender.send((platform, run_id, result)).is_err() {
                    break;
                }
            });
        }
        drop(sender);

        let mut totals = CollectionStats::default();
        for (platform, run_id, result) in receiver {
            match result {
                Ok(feed) => match repository.commit_feed(&platform, run_id, &feed) {
                    Ok(stats) => {
                        totals.inserted += stats.inserted;
                        totals.updated += stats.updated;
                        totals.inserted_topic_ids.extend(stats.inserted_topic_ids);
                    }
                    Err(error) => {
                        let _ = repository.fail_run(run_id, &error.to_string());
                    }
                },
                Err(error) => {
                    let _ = repository.fail_run(run_id, &error.to_string());
                }
            }
        }
        repository.clean_observations(30)?;
        Ok(totals)
    })
}

/// Route only sources proven unreliable on direct mainland connections through
/// the explicit proxy; all others keep their original geographic response.
fn source_requires_proxy(code: &str) -> bool {
    matches!(
        code,
        "binance-square-zh"
            | "binance-square-global"
            | "mastodon-zh"
            | "mastodon-global"
            | "coingecko"
            | "github"
            | "google-trends-zh"
            | "google-trends-global"
            | "hugging-face"
            | "bluesky"
            | "polymarket"
            | "bbc-chinese"
            | "dw-chinese"
            | "rfi-chinese"
            | "bloomberg"
    )
}

/// Fetch one source with known fallbacks and reject an apparently successful empty parse.
fn fetch_source(client: &Client, platform: &PlatformSource) -> AppResult<ParsedFeed> {
    let mut endpoints = vec![platform.endpoint_url.as_str()];
    match platform.code.as_str() {
        "36kr" => endpoints.push("https://news.orz.ai/api/v1/dailynews/?platform=36kr"),
        "xueqiu" => endpoints.push("https://news.orz.ai/api/v1/dailynews/?platform=xueqiu"),
        "zhihu" => endpoints.push("https://news.orz.ai/api/v1/dailynews/?platform=zhihu"),
        _ => {}
    }
    let mut failures = Vec::new();
    for endpoint in endpoints {
        for empty_attempt in 0..2 {
            match fetch_endpoint(client, platform, endpoint) {
                Ok(feed) if !feed.topics.is_empty() => return Ok(feed),
                Ok(_) if empty_attempt == 0 && endpoint.contains("news.orz.ai") => {
                    // The aggregator briefly publishes an empty list while
                    // rotating its cache; retry once before declaring failure.
                    std::thread::sleep(Duration::from_millis(800));
                    continue;
                }
                Ok(_) if platform.code == "xiaohongshu" => failures
                    .push("xiaohongshu 页面已改为动态签名加载，公开 HTML 不含热点数据".into()),
                Ok(_) => failures.push(format!("{} 未解析到有效热点", platform.code)),
                Err(AppError::Collection(message)) => failures.push(message),
                Err(error) => failures.push(error.to_string()),
            }
            break;
        }
    }
    Err(AppError::Collection(failures.join("；")))
}

/// Dispatch protocol-specific endpoints while keeping all output in one normalized model.
fn fetch_endpoint(
    client: &Client,
    platform: &PlatformSource,
    endpoint: &str,
) -> AppResult<ParsedFeed> {
    match platform.code.as_str() {
        "hacker-news" => return fetch_hacker_news(client, platform, endpoint),
        "wikipedia-zh" | "wikipedia-global" => return fetch_wikipedia(client, platform, endpoint),
        _ => {}
    }
    let response = request(client, &platform.code, endpoint)?;
    let final_url = response.url().clone();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = response.bytes().map_err(collection_error)?;

    if matches!(platform.code.as_str(), "arxiv") {
        return parse_arxiv(
            &platform.code,
            decode_text(&bytes, &content_type),
            final_url.as_str(),
        );
    }
    if platform.code == "xiaohongshu" {
        return parse_xiaohongshu(&platform.code, decode_text(&bytes, &content_type));
    }
    if is_html_source(&platform.code) && final_url.host_str() != Some("news.orz.ai") {
        return parse_html_links(
            &platform.code,
            decode_text(&bytes, &content_type),
            final_url.as_str(),
        );
    }
    if content_type.contains("json") || looks_like_json(&bytes) {
        let payload: Value = serde_json::from_slice(&bytes).map_err(|error| {
            AppError::Collection(format!("{} JSON 无效：{error}", platform.code))
        })?;
        return parse_json(&platform.code, &payload, final_url.as_str());
    }
    parse_feed(&platform.code, &bytes)
}

/// Apply source-specific request headers while keeping redirects and timeouts centralized.
fn request(client: &Client, code: &str, endpoint: &str) -> AppResult<Response> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, application/atom+xml, application/rss+xml, application/xml, text/html;q=0.8"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    if matches!(code, "binance-square-zh" | "binance-square-global") {
        let language = if code.ends_with("-zh") { "zh-CN" } else { "en" };
        headers.insert("clienttype", HeaderValue::from_static("web"));
        headers.insert(
            "lang",
            HeaderValue::from_str(language).map_err(collection_error)?,
        );
        headers.insert(
            COOKIE,
            HeaderValue::from_str(&format!("lang={language}")).map_err(collection_error)?,
        );
        headers.insert(
            REFERER,
            HeaderValue::from_str(&format!(
                "https://www.binance.com/{language}/square/trending"
            ))
            .map_err(collection_error)?,
        );
    } else if code == "jin10" {
        headers.insert("x-app-id", HeaderValue::from_static("bVBF4FyRTn5NJF5n"));
        headers.insert("x-version", HeaderValue::from_static("1.0.0"));
    }
    let mut response = None;
    for attempt in 0..2 {
        match client.get(endpoint).headers(headers.clone()).send() {
            Ok(value) => {
                response = Some(value);
                break;
            }
            Err(error) if attempt == 0 && (error.is_timeout() || error.is_connect()) => continue,
            Err(error) => return Err(request_error(code, endpoint, error)),
        }
    }
    let response = response
        .ok_or_else(|| AppError::Collection(format!("{code}：有限重试后仍未获得网络响应")))?;
    response
        .error_for_status()
        .map_err(|error| request_error(code, endpoint, error))
}

/// Reduce reqwest's nested transport errors to actionable, non-sensitive diagnostics.
fn request_error(code: &str, endpoint: &str, error: reqwest::Error) -> AppError {
    let host = Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "未知地址".into());
    let mut chain = error.to_string().to_ascii_lowercase();
    let mut source = error.source();
    while let Some(cause) = source {
        chain.push(' ');
        chain.push_str(&cause.to_string().to_ascii_lowercase());
        source = cause.source();
    }
    let detail = if let Some(status) = error.status() {
        format!("HTTP {status}")
    } else if error.is_timeout() {
        "连接或读取超时".into()
    } else if chain.contains("dns")
        || chain.contains("failed to lookup address")
        || chain.contains("name or service not known")
    {
        "DNS 解析失败".into()
    } else if chain.contains("connection refused") {
        "连接被拒绝".into()
    } else if chain.contains("connection reset") || chain.contains("reset by peer") {
        "连接被远端重置".into()
    } else if chain.contains("certificate") || chain.contains("tls") {
        "TLS 安全连接失败".into()
    } else if error.is_connect() {
        "无法建立网络连接".into()
    } else if error.is_redirect() {
        "重定向次数过多或地址无效".into()
    } else {
        "网络请求失败".into()
    };
    AppError::Collection(format!("{code} · {host}：{detail}"))
}

/// Parse RSS, Atom and RDF feeds using a tolerant standards-aware parser.
fn parse_feed(code: &str, bytes: &[u8]) -> AppResult<ParsedFeed> {
    let feed = feed_rs::parser::parse(bytes)
        .map_err(|error| AppError::Collection(format!("{code} Feed 无效：{error}")))?;
    let fetched_count = feed.entries.len().min(ITEMS_PER_PLATFORM) as u32;
    let mut topics = Vec::new();
    for (index, entry) in feed
        .entries
        .into_iter()
        .take(ITEMS_PER_PLATFORM)
        .enumerate()
    {
        let title = entry.title.map(|value| value.content.trim().to_owned());
        let url = entry.links.first().map(|link| link.href.clone());
        if let (Some(title), Some(url)) = (title, url) {
            if valid_http_url(&url) && !title.is_empty() {
                topics.push(CollectedTopic {
                    platform_code: code.to_owned(),
                    stable_id: (!entry.id.trim().is_empty()).then_some(entry.id),
                    title: truncate_title(&title),
                    url,
                    published_time: entry
                        .published
                        .or(entry.updated)
                        .map(|value| value.to_rfc3339()),
                    rank: (index + 1) as u32,
                    heat: None,
                });
            }
        }
    }
    Ok(ParsedFeed {
        invalid_count: fetched_count.saturating_sub(topics.len() as u32),
        fetched_count,
        topics,
    })
}

/// Parse public JSON APIs whose schemas are stable enough for deterministic extraction.
fn parse_json(code: &str, payload: &Value, endpoint: &str) -> AppResult<ParsedFeed> {
    let items: Vec<&Value> = if Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .as_deref()
        == Some("news.orz.ai")
    {
        array_at(payload, "data")
    } else {
        match code {
            "toutiao" | "zhihu" | "jin10" => array_at(payload, "data"),
            "coingecko" => array_at(payload, "coins"),
            "hugging-face" | "polymarket" | "dev-community" | "mastodon-zh" | "mastodon-global" => {
                payload
                    .as_array()
                    .map(|items| items.iter().collect())
                    .unwrap_or_default()
            }
            "bluesky" => array_at(payload, "topics"),
            "stack-overflow" => array_at(payload, "items"),
            "binance-square-zh" | "binance-square-global" => payload
                .pointer("/data/vos")
                .and_then(Value::as_array)
                .map(|items| items.iter().collect())
                .unwrap_or_default(),
            _ => return Err(AppError::Collection(format!("没有 JSON 解析器：{code}"))),
        }
    };
    let fetched_count = items.len().min(ITEMS_PER_PLATFORM) as u32;
    let mut topics = Vec::new();
    for (index, item) in items.into_iter().take(ITEMS_PER_PLATFORM).enumerate() {
        if let Some(topic) = json_topic(code, item, endpoint, index + 1) {
            topics.push(topic);
        }
    }
    Ok(ParsedFeed {
        invalid_count: fetched_count.saturating_sub(topics.len() as u32),
        fetched_count,
        topics,
    })
}

/// Normalize one JSON object. Returning None makes malformed upstream rows non-fatal.
fn json_topic(code: &str, item: &Value, endpoint: &str, rank: usize) -> Option<CollectedTopic> {
    let host = Url::parse(endpoint).ok()?.host_str()?.to_owned();
    let (stable_id, title, url, published, heat) = if host == "news.orz.ai" {
        (
            text(item.get(if code == "cls" { "title" } else { "url" })),
            text(item.get("title")),
            text(item.get("url")),
            text(item.get("publish_time")),
            number(item.get("score").or_else(|| item.get("rank"))),
        )
    } else {
        match code {
            "toutiao" => (
                text(item.get("ClusterIdStr")),
                text(item.get("Title")),
                text(item.get("Url")),
                None,
                number(item.get("HotValue")),
            ),
            "zhihu" => {
                let target = item.get("target")?;
                (
                    text(target.get("id")),
                    text(target.get("title")),
                    text(target.get("url")),
                    None,
                    number(item.get("detail_text")),
                )
            }
            "coingecko" => {
                let coin = item.get("item")?;
                let id = text(coin.get("id"));
                (
                    id.clone(),
                    Some(format!(
                        "{} ({})",
                        text(coin.get("name"))?,
                        text(coin.get("symbol"))?
                    )),
                    Some(format!(
                        "https://www.coingecko.com/en/coins/{}",
                        id.clone()?
                    )),
                    None,
                    number(coin.get("score")),
                )
            }
            "hugging-face" => {
                let paper = item.get("paper").unwrap_or(item);
                let id = text(paper.get("id"));
                (
                    id.clone(),
                    text(paper.get("title")),
                    Some(format!("https://huggingface.co/papers/{}", id?)),
                    text(paper.get("publishedAt")),
                    number(paper.get("upvotes")),
                )
            }
            "bluesky" => {
                let topic = text(item.get("topic")).or_else(|| text(item.get("displayName")));
                (
                    topic.clone(),
                    text(item.get("displayName")).or_else(|| topic.clone()),
                    text(item.get("link"))
                        .or_else(|| Some(format!("https://bsky.app/search?q={}", topic.clone()?))),
                    None,
                    None,
                )
            }
            "polymarket" => (
                text(item.get("id")).or_else(|| text(item.get("conditionId"))),
                text(item.get("question")),
                Some(format!(
                    "https://polymarket.com/event/{}",
                    text(item.get("slug"))?
                )),
                text(item.get("createdAt")),
                number(item.get("volume24hr")),
            ),
            "stack-overflow" => (
                text(item.get("question_id")),
                text(item.get("title")),
                text(item.get("link")),
                unix_time(item.get("creation_date")),
                number(item.get("score")),
            ),
            "dev-community" => (
                text(item.get("id")),
                text(item.get("title")),
                text(item.get("url")),
                text(item.get("published_at")),
                number(item.get("positive_reactions_count")),
            ),
            "jin10" => {
                let data = item.get("data")?;
                (
                    text(item.get("id")),
                    text(data.get("title")).or_else(|| text(data.get("content"))),
                    text(data.get("link")).or_else(|| Some("https://www.jin10.com/flash".into())),
                    text(item.get("time")),
                    number(item.get("important")),
                )
            }
            "binance-square-zh" | "binance-square-global" => (
                text(item.get("id")),
                text(item.get("title")).or_else(|| text(item.get("content"))),
                text(item.get("webLink")),
                unix_time(item.get("date")),
                number(item.get("viewCount")),
            ),
            "mastodon-zh" => {
                let heat = ["replies_count", "reblogs_count", "favourites_count"]
                    .into_iter()
                    .map(|key| number(item.get(key)).unwrap_or(0.0))
                    .sum();
                (
                    text(item.get("id")),
                    text(item.get("content")).map(|value| plain_text(&value)),
                    text(item.get("url")),
                    text(item.get("created_at")),
                    Some(heat),
                )
            }
            "mastodon-global" => {
                let heat = item
                    .get("history")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|entry| number(entry.get("uses")).unwrap_or(0.0))
                            .sum()
                    });
                (
                    text(item.get("url")),
                    text(item.get("title")),
                    text(item.get("url")),
                    None,
                    heat,
                )
            }
            _ => return None,
        }
    };
    make_topic(code, rank, stable_id, title, url, published, heat, endpoint)
}

/// Extract article-like links from sources without a public structured endpoint.
fn parse_html_links(code: &str, html: String, base: &str) -> AppResult<ParsedFeed> {
    let anchor = Regex::new(r#"(?is)<a\b([^>]*)href=["']([^"']+)["']([^>]*)>(.*?)</a>"#)
        .map_err(collection_error)?;
    let title_attribute = Regex::new(r#"(?i)title=["']([^"']+)["']"#).map_err(collection_error)?;
    let mut seen = HashSet::new();
    let mut topics = Vec::new();
    for captures in anchor.captures_iter(&html) {
        let attributes = format!("{} {}", &captures[1], &captures[3]);
        let candidate_title = title_attribute
            .captures(&attributes)
            .map(|value| value[1].to_owned())
            .unwrap_or_else(|| captures[4].to_owned());
        let title = plain_text(&candidate_title);
        let Some(url) = absolute_url(&captures[2], base) else {
            continue;
        };
        let accepted = Url::parse(&url)
            .ok()
            .is_some_and(|value| accepted_html_path(code, value.path()));
        if title.chars().count() < 6
            || title.chars().count() > 180
            || !accepted
            || !seen.insert(url.clone())
        {
            continue;
        }
        topics.push(CollectedTopic {
            platform_code: code.into(),
            stable_id: Some(url.clone()),
            title,
            url,
            published_time: None,
            rank: (topics.len() + 1) as u32,
            heat: None,
        });
        if topics.len() >= ITEMS_PER_PLATFORM {
            break;
        }
    }
    Ok(ParsedFeed {
        fetched_count: topics.len() as u32,
        invalid_count: 0,
        topics,
    })
}

/// Parse arXiv's compact recent-list HTML when no JSON endpoint is available.
fn parse_arxiv(code: &str, html: String, base: &str) -> AppResult<ParsedFeed> {
    let entry = Regex::new(r#"(?is)<dt>.*?<a\s+href\s*=\s*["'](/abs/[^"']+)["'][^>]*\bid=["']([^"']+)["'].*?</dt>\s*<dd>.*?<div\s+class=["']list-title\s+mathjax["'][^>]*>\s*<span[^>]*>.*?</span>(.*?)</div>"#).map_err(collection_error)?;
    let mut topics = Vec::new();
    for captures in entry.captures_iter(&html).take(ITEMS_PER_PLATFORM) {
        if let Some(topic) = make_topic(
            code,
            topics.len() + 1,
            Some(captures[2].into()),
            Some(captures[3].into()),
            Some(captures[1].into()),
            None,
            None,
            base,
        ) {
            topics.push(topic);
        }
    }
    Ok(ParsedFeed {
        fetched_count: topics.len() as u32,
        invalid_count: 0,
        topics,
    })
}

/// Extract public explore cards from Xiaohongshu's server-rendered page.
fn parse_xiaohongshu(code: &str, html: String) -> AppResult<ParsedFeed> {
    let entry = Regex::new(r#"(?is)<a\b[^>]*href=["'][^"']*/explore/([a-f0-9]{24})[^"']*["'][^>]*>.*?<span[^>]*>(.*?)</span>.*?</a>"#).map_err(collection_error)?;
    let mut seen = HashSet::new();
    let mut topics = Vec::new();
    for captures in entry.captures_iter(&html) {
        let id = captures[1].to_owned();
        if seen.insert(id.clone()) {
            if let Some(topic) = make_topic(
                code,
                topics.len() + 1,
                Some(id.clone()),
                Some(captures[2].into()),
                Some(format!("https://www.xiaohongshu.com/explore/{id}")),
                None,
                None,
                "https://www.xiaohongshu.com/",
            ) {
                topics.push(topic);
            }
        }
        if topics.len() >= ITEMS_PER_PLATFORM {
            break;
        }
    }
    if topics.is_empty()
        && html.contains("window.__INITIAL_STATE__")
        && html.contains(r#"\"feeds\":[]"#)
    {
        return Err(AppError::Collection(
            "xiaohongshu 页面已改为动态签名加载，公开 HTML 不含热点数据".into(),
        ));
    }
    Ok(ParsedFeed {
        fetched_count: topics.len() as u32,
        invalid_count: 0,
        topics,
    })
}

/// Hacker News needs one index request followed by item requests.
fn fetch_hacker_news(
    client: &Client,
    platform: &PlatformSource,
    base: &str,
) -> AppResult<ParsedFeed> {
    let ids: Vec<Value> = request(client, &platform.code, &format!("{base}/topstories.json"))?
        .json()
        .map_err(collection_error)?;
    let mut topics = Vec::new();
    let fetched_count = ids.len().min(ITEMS_PER_PLATFORM) as u32;
    for (index, id) in ids.into_iter().take(ITEMS_PER_PLATFORM).enumerate() {
        let item: Value = request(client, &platform.code, &format!("{base}/item/{id}.json"))?
            .json()
            .map_err(collection_error)?;
        let url = text(item.get("url"))
            .or_else(|| Some(format!("https://news.ycombinator.com/item?id={id}")));
        if let Some(topic) = make_topic(
            &platform.code,
            index + 1,
            text(item.get("id")),
            text(item.get("title")),
            url,
            unix_time(item.get("time")),
            number(item.get("score")),
            base,
        ) {
            topics.push(topic);
        }
    }
    Ok(ParsedFeed {
        invalid_count: fetched_count.saturating_sub(topics.len() as u32),
        fetched_count,
        topics,
    })
}

/// Wikimedia publishes yesterday's complete page-view board by language.
fn fetch_wikipedia(
    client: &Client,
    platform: &PlatformSource,
    base: &str,
) -> AppResult<ParsedFeed> {
    let yesterday = OffsetDateTime::now_utc() - TimeDuration::days(1);
    let endpoint = format!(
        "{base}/{:04}/{:02}/{:02}",
        yesterday.year(),
        u8::from(yesterday.month()),
        yesterday.day()
    );
    let payload: Value = request(client, &platform.code, &endpoint)?
        .json()
        .map_err(collection_error)?;
    let language = if platform.code.ends_with("-zh") {
        "zh"
    } else {
        "en"
    };
    let articles = payload
        .pointer("/items/0/articles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut topics = Vec::new();
    for article in articles.into_iter() {
        let Some(id) = text(article.get("article")) else {
            continue;
        };
        if id == "Main_Page" || id.contains(':') {
            continue;
        }
        if let Some(topic) = make_topic(
            &platform.code,
            topics.len() + 1,
            Some(id.clone()),
            Some(id.replace('_', " ")),
            Some(format!(
                "https://{language}.wikipedia.org/wiki/{}",
                url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>()
            )),
            None,
            number(article.get("views")),
            base,
        ) {
            topics.push(topic);
        }
        if topics.len() >= ITEMS_PER_PLATFORM {
            break;
        }
    }
    Ok(ParsedFeed {
        fetched_count: topics.len() as u32,
        invalid_count: 0,
        topics,
    })
}

/// Create a topic only when title and HTTP(S) URL both survive normalization.
#[allow(clippy::too_many_arguments)]
fn make_topic(
    code: &str,
    rank: usize,
    stable_id: Option<String>,
    title: Option<String>,
    url: Option<String>,
    published_time: Option<String>,
    heat: Option<f64>,
    base: &str,
) -> Option<CollectedTopic> {
    let title = truncate_title(&plain_text(&title?));
    let url = absolute_url(&url?, base)?;
    if title.is_empty() {
        return None;
    }
    Some(CollectedTopic {
        platform_code: code.into(),
        stable_id,
        title,
        url,
        published_time,
        rank: rank as u32,
        heat,
    })
}

fn array_at<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) if !value.trim().is_empty() => Some(value.trim().to_owned()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str()?.replace([',', '+'], "").parse().ok())
    })
}

fn unix_time(value: Option<&Value>) -> Option<String> {
    let raw = value.and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))?;
    let seconds = if raw > 10_000_000_000 {
        raw / 1_000
    } else {
        raw
    };
    OffsetDateTime::from_unix_timestamp(seconds)
        .ok()
        .and_then(|value| {
            value
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
}

fn plain_text(value: &str) -> String {
    let without_tags =
        Regex::new(r"(?is)<script\b[^>]*>.*?</script>|<style\b[^>]*>.*?</style>|<[^>]+>")
            .map_or_else(
                |_| value.to_owned(),
                |regex| regex.replace_all(value, " ").into_owned(),
            );
    without_tags
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_title(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= 180 {
        value.to_owned()
    } else {
        format!("{}…", chars[..177].iter().collect::<String>())
    }
}

fn absolute_url(value: &str, base: &str) -> Option<String> {
    let url = Url::parse(base).ok()?.join(value).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

fn valid_http_url(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn accepted_html_path(code: &str, path: &str) -> bool {
    let pattern = match code {
        "huxiu" => r"^/article/\d+",
        "c114" => r"/a\d+\.html$",
        "wallstreetcn" => r"^/(?:articles|livenews)/\d+",
        "odaily" => r"^/(?:zh-CN/)?post/\d+",
        "thepaper" => r"^/newsDetail_forward_\d+",
        "github" => r"^/[^/?#]+/[^/?#]+/?$",
        _ => return true,
    };
    Regex::new(pattern).is_ok_and(|regex| regex.is_match(path))
}

fn is_html_source(code: &str) -> bool {
    matches!(
        code,
        "36kr" | "huxiu" | "c114" | "wallstreetcn" | "odaily" | "xueqiu" | "thepaper" | "github"
    )
}

fn looks_like_json(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| matches!(byte, b'{' | b'['))
}

fn decode_text(bytes: &[u8], content_type: &str) -> String {
    // Several Chinese sites omit the HTTP charset but still declare GBK in the HTML head.
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(2_048)]).to_ascii_lowercase();
    if content_type.contains("gb2312")
        || content_type.contains("gbk")
        || content_type.contains("gb18030")
        || prefix.contains("charset=gb2312")
        || prefix.contains("charset=gbk")
        || prefix.contains("charset=gb18030")
    {
        return GBK.decode(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(value) => value.to_owned(),
        // A non-UTF-8 Chinese page with no declaration is more usefully decoded as GBK than lossy UTF-8.
        Err(_) => GBK.decode(bytes).0.into_owned(),
    }
}

fn collection_error(error: impl std::fmt::Display) -> AppError {
    AppError::Collection(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped fixtures protect RSS and Atom compatibility independently of network access.
    #[test]
    fn parses_rss_fixture() {
        let bytes = include_bytes!("../../tests/fixtures/qbitai.xml");
        let feed = parse_feed("qbitai", bytes).expect("fixture should parse");
        assert!(!feed.topics.is_empty());
        assert_eq!(feed.topics[0].platform_code, "qbitai");
    }

    /// Tracking or script markup must not leak into titles shown in the desktop UI.
    #[test]
    fn strips_html_from_titles() {
        assert_eq!(plain_text("<b>Hello</b> &amp; world"), "Hello & world");
    }

    #[test]
    fn explains_xiaohongshu_dynamic_empty_shell() {
        let error = parse_xiaohongshu(
            "xiaohongshu",
            r#"<script>window.__INITIAL_STATE__={\"feed\":{\"feeds\":[]}}</script>"#.into(),
        )
        .expect_err("empty signed shell must not look like a valid empty board");
        assert!(error.to_string().contains("动态签名加载"));
    }

    #[test]
    fn proxies_only_sources_that_need_non_direct_routes() {
        for code in [
            "coingecko",
            "github",
            "google-trends-zh",
            "hugging-face",
            "bbc-chinese",
            "binance-square-global",
        ] {
            assert!(source_requires_proxy(code), "{code} should use the proxy");
        }
        for code in ["36kr", "weibo", "baidu", "zhihu", "arxiv"] {
            assert!(!source_requires_proxy(code), "{code} should stay direct");
        }
    }

    /// Legacy Chinese pages commonly advertise GB2312 only in a meta tag.
    #[test]
    fn decodes_gbk_declared_in_document() {
        let html = "<meta charset=gb2312><a>通信行业</a>";
        let encoded = GBK.encode(html).0;
        assert!(decode_text(&encoded, "text/html").contains("通信行业"));
    }
}
