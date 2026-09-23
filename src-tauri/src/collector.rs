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
use scraper::{Html, Selector};
use serde_json::Value;
use time::{Duration as TimeDuration, OffsetDateTime};
use url::Url;

use crate::database::open_database;
use crate::error::{AppError, AppResult};
use crate::models::{
    CollectedTopic, CollectionStats, ParsedFeed, PlatformSource, SourceParserType, SourceProxyMode,
};
use crate::repository::TopicRepository;

const ITEMS_PER_PLATFORM: usize = 50;
const REQUEST_TIMEOUT_SECONDS: u64 = 15;
const MAX_WORKERS: usize = 6;
const HUGGING_FACE_TRENDING_MODELS_ENDPOINT: &str = "https://huggingface.co/api/models?sort=trendingScore&direction=-1&limit=50&expand=trendingScore,likes,downloads,createdAt,lastModified";
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
                let client = if source_requires_proxy(&platform) {
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
        repository.record_recent_additions(trigger, &totals.inserted_topic_ids)?;
        repository.maintain_storage()?;
        Ok(totals)
    })
}

/// Route only sources proven unreliable on direct mainland connections through
/// the explicit proxy; all others keep their original geographic response.
fn source_requires_proxy(platform: &PlatformSource) -> bool {
    match platform.proxy_mode {
        SourceProxyMode::Proxy => true,
        SourceProxyMode::Direct => false,
        SourceProxyMode::Auto => crate::catalog::default_proxy_mode(&platform.code) == "proxy",
    }
}

/// Fetch one source with known fallbacks and reject an apparently successful empty parse.
fn fetch_source(client: &Client, platform: &PlatformSource) -> AppResult<ParsedFeed> {
    // Upgrade only the endpoint shipped by old installations. Once the user
    // edits this field, their configured endpoint must remain authoritative.
    let mut endpoints =
        if platform.code == "hugging-face" && platform.endpoint_url.contains("/api/daily_papers") {
            vec![HUGGING_FACE_TRENDING_MODELS_ENDPOINT]
        } else {
            vec![platform.endpoint_url.as_str()]
        };
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

    match platform.parser_type {
        SourceParserType::Rss => return parse_feed(&platform.code, &bytes),
        SourceParserType::Json => {
            let payload: Value = serde_json::from_slice(&bytes).map_err(|error| {
                AppError::Collection(format!("{} JSON 无效：{error}", platform.code))
            })?;
            return parse_custom_json(platform, &payload, final_url.as_str());
        }
        SourceParserType::Html => {
            return parse_custom_html(
                platform,
                decode_text(&bytes, &content_type),
                final_url.as_str(),
            )
        }
        SourceParserType::Builtin => {}
    }

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
    if platform.code == "github" {
        return parse_github_trending(
            &platform.code,
            decode_text(&bytes, &content_type),
            final_url.as_str(),
        );
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
            "hugging-face" | "polymarket" | "dev-community" | "mastodon-global" => payload
                .as_array()
                .map(|items| items.iter().collect())
                .unwrap_or_default(),
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

/// Parse a configured JSON array using bounded dot paths or JSON pointers.
fn parse_custom_json(
    platform: &PlatformSource,
    payload: &Value,
    endpoint: &str,
) -> AppResult<ParsedFeed> {
    let config = &platform.parser_config;
    let title_path = required_config(&config.title_path, "标题字段")?;
    let url_path = required_config(&config.url_path, "链接字段")?;
    let items = value_at_path(payload, config.items_path.as_deref().unwrap_or(""))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::Collection(format!("{} 列表路径未指向 JSON 数组", platform.code))
        })?;
    let fetched_count = items.len().min(ITEMS_PER_PLATFORM) as u32;
    let mut topics = Vec::new();
    for (index, item) in items.iter().take(ITEMS_PER_PLATFORM).enumerate() {
        let configured_rank = config
            .rank_path
            .as_deref()
            .and_then(|path| number(value_at_path(item, path)))
            .and_then(|value| (value >= 1.0).then_some(value as usize));
        if let Some(topic) = make_topic(
            &platform.code,
            configured_rank.unwrap_or(index + 1),
            config
                .id_path
                .as_deref()
                .and_then(|path| text(value_at_path(item, path))),
            text(value_at_path(item, title_path)),
            text(value_at_path(item, url_path)),
            config
                .published_path
                .as_deref()
                .and_then(|path| text(value_at_path(item, path))),
            config
                .heat_path
                .as_deref()
                .and_then(|path| number(value_at_path(item, path))),
            endpoint,
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

/// Parse configured HTML using CSS selectors without evaluating page scripts.
fn parse_custom_html(
    platform: &PlatformSource,
    html: String,
    endpoint: &str,
) -> AppResult<ParsedFeed> {
    let config = &platform.parser_config;
    let item_selector = parse_selector(config.item_selector.as_deref(), "条目选择器")?;
    let title_selector = parse_selector(config.title_selector.as_deref(), "标题选择器")?;
    let link_selector = parse_selector(
        config
            .link_selector
            .as_deref()
            .or(config.title_selector.as_deref()),
        "链接选择器",
    )?;
    let document = Html::parse_document(&html);
    let items = document
        .select(&item_selector)
        .take(ITEMS_PER_PLATFORM)
        .collect::<Vec<_>>();
    let fetched_count = items.len() as u32;
    let mut topics = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        let title = item
            .select(&title_selector)
            .next()
            .map(|node| node.text().collect::<Vec<_>>().join(" "));
        let link = item
            .select(&link_selector)
            .next()
            .and_then(|node| node.value().attr("href"))
            .map(str::to_owned);
        if let Some(topic) = make_topic(
            &platform.code,
            index + 1,
            link.clone(),
            title,
            link,
            None,
            None,
            endpoint,
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

fn required_config<'a>(value: &'a Option<String>, label: &str) -> AppResult<&'a str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Collection(format!("缺少{label}")))
}

fn parse_selector(value: Option<&str>, label: &str) -> AppResult<Selector> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Collection(format!("缺少{label}")))?;
    Selector::parse(value)
        .map_err(|_| AppError::Collection(format!("{label}不是有效的 CSS 选择器")))
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim();
    if path.is_empty() {
        return Some(value);
    }
    if path.starts_with('/') {
        return value.pointer(path);
    }
    path.split('.').try_fold(value, |current, segment| {
        if let Ok(index) = segment.parse::<usize>() {
            current.get(index)
        } else {
            current.get(segment)
        }
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
                let id = text(item.get("modelId")).or_else(|| text(item.get("id")));
                (
                    id.clone(),
                    id.clone(),
                    Some(format!("https://huggingface.co/{}", id?)),
                    text(item.get("createdAt")).or_else(|| text(item.get("lastModified"))),
                    number(item.get("trendingScore")).or_else(|| number(item.get("likes"))),
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

/// Parse GitHub's daily Trending board as repositories ranked by stars gained today.
fn parse_github_trending(code: &str, html: String, base: &str) -> AppResult<ParsedFeed> {
    let article = Regex::new(
        r#"(?is)<article\b[^>]*class=["'][^"']*\bbox-row\b[^"']*["'][^>]*>(.*?)</article>"#,
    )
    .map_err(collection_error)?;
    let repository = Regex::new(r#"(?is)<h2\b.*?<a\b[^>]*href=["'](/[^/"'#?]+/[^/"'#?]+)["']"#)
        .map_err(collection_error)?;
    let daily_stars =
        Regex::new(r"(?i)([\d,.]+(?:\.\d+)?[km]?)\s+stars?\s+today").map_err(collection_error)?;
    let mut topics = Vec::new();
    let mut seen = HashSet::new();
    let fetched_count = article.captures_iter(&html).count().min(ITEMS_PER_PLATFORM) as u32;
    for card in article.captures_iter(&html).take(ITEMS_PER_PLATFORM) {
        let Some(path) = repository
            .captures(&card[1])
            .map(|captures| captures[1].to_owned())
        else {
            continue;
        };
        let title = path.trim_matches('/').to_owned();
        let card_text = plain_text(&card[1]);
        let Some(heat) = daily_stars
            .captures(&card_text)
            .and_then(|captures| compact_number(&captures[1]))
        else {
            continue;
        };
        if !seen.insert(title.clone()) {
            continue;
        }
        if let Some(topic) = make_topic(
            code,
            topics.len() + 1,
            Some(title.clone()),
            Some(title),
            Some(path),
            None,
            Some(heat),
            base,
        ) {
            topics.push(topic);
        }
    }
    // GitHub's page order mixes several signals; the product promise is the
    // fastest daily star growth, so make that metric the deterministic rank.
    topics.sort_by(|left, right| {
        right
            .heat
            .partial_cmp(&left.heat)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (index, topic) in topics.iter_mut().enumerate() {
        topic.rank = (index + 1) as u32;
    }
    Ok(ParsedFeed {
        invalid_count: fetched_count.saturating_sub(topics.len() as u32),
        fetched_count,
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

/// Decode compact counters used by project boards, such as `1,240` or `2.3k`.
fn compact_number(value: &str) -> Option<f64> {
    let normalized = value.trim().to_ascii_lowercase().replace(',', "");
    let (number, multiplier) = match normalized.chars().last()? {
        'k' => (&normalized[..normalized.len() - 1], 1_000.0),
        'm' => (&normalized[..normalized.len() - 1], 1_000_000.0),
        _ => (normalized.as_str(), 1.0),
    };
    number.parse::<f64>().ok().map(|value| value * multiplier)
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
    use crate::models::CustomParserConfig;

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

    /// GitHub entries must be repositories ordered by today's star gain, not article links.
    #[test]
    fn parses_github_projects_by_daily_star_growth() {
        let html = r#"
          <article class="Box-row"><h2><a href="/slow/project">slow / project</a></h2><span>124 stars today</span></article>
          <article class="Box-row"><h2><a href="/fast/project">fast / project</a></h2><span>1.2k stars today</span></article>
        "#;
        let feed = parse_github_trending("github", html.into(), "https://github.com/trending")
            .expect("GitHub fixture should parse");
        assert_eq!(feed.topics.len(), 2);
        assert_eq!(feed.topics[0].title, "fast/project");
        assert_eq!(feed.topics[0].url, "https://github.com/fast/project");
        assert_eq!(feed.topics[0].heat, Some(1_200.0));
    }

    /// Hugging Face must expose model repositories ranked by trending score, not papers.
    #[test]
    fn parses_hugging_face_trending_models() {
        let payload = serde_json::json!([{
            "id": "org/hot-model",
            "trendingScore": 87,
            "likes": 420,
            "createdAt": "2026-09-20T00:00:00.000Z"
        }]);
        let feed = parse_json(
            "hugging-face",
            &payload,
            HUGGING_FACE_TRENDING_MODELS_ENDPOINT,
        )
        .expect("Hugging Face fixture should parse");
        assert_eq!(feed.topics.len(), 1);
        assert_eq!(feed.topics[0].title, "org/hot-model");
        assert_eq!(feed.topics[0].url, "https://huggingface.co/org/hot-model");
        assert_eq!(feed.topics[0].heat, Some(87.0));
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
        let platform = |code: &str, proxy_mode| PlatformSource {
            id: 1,
            code: code.into(),
            endpoint_url: "https://example.com".into(),
            parser_type: SourceParserType::Builtin,
            parser_config: CustomParserConfig::default(),
            proxy_mode,
        };
        assert!(source_requires_proxy(&platform(
            "github",
            SourceProxyMode::Auto
        )));
        assert!(!source_requires_proxy(&platform(
            "36kr",
            SourceProxyMode::Auto
        )));
        assert!(source_requires_proxy(&platform(
            "custom",
            SourceProxyMode::Proxy
        )));
        assert!(!source_requires_proxy(&platform(
            "github",
            SourceProxyMode::Direct
        )));
    }

    #[test]
    fn parses_declarative_json_and_html_sources() {
        let json_platform = PlatformSource {
            id: 1,
            code: "custom-json".into(),
            endpoint_url: "https://example.com/api".into(),
            parser_type: SourceParserType::Json,
            parser_config: CustomParserConfig {
                items_path: Some("data.items".into()),
                id_path: Some("id".into()),
                title_path: Some("title".into()),
                url_path: Some("url".into()),
                ..CustomParserConfig::default()
            },
            proxy_mode: SourceProxyMode::Direct,
        };
        let payload =
            serde_json::json!({"data":{"items":[{"id":"1","title":"Hello","url":"/hello"}]}});
        let json = parse_custom_json(&json_platform, &payload, "https://example.com/api").unwrap();
        assert_eq!(json.topics[0].url, "https://example.com/hello");

        let html_platform = PlatformSource {
            code: "custom-html".into(),
            parser_type: SourceParserType::Html,
            parser_config: CustomParserConfig {
                item_selector: Some("article".into()),
                title_selector: Some("h2".into()),
                link_selector: Some("a".into()),
                ..CustomParserConfig::default()
            },
            ..json_platform
        };
        let html = parse_custom_html(
            &html_platform,
            "<article><h2>World</h2><a href='/world'>Read</a></article>".into(),
            "https://example.com/",
        )
        .unwrap();
        assert_eq!(html.topics[0].title, "World");
        assert_eq!(html.topics[0].url, "https://example.com/world");
    }

    /// Legacy Chinese pages commonly advertise GB2312 only in a meta tag.
    #[test]
    fn decodes_gbk_declared_in_document() {
        let html = "<meta charset=gb2312><a>通信行业</a>";
        let encoded = GBK.encode(html).0;
        assert!(decode_text(&encoded, "text/html").contains("通信行业"));
    }
}
