//! Network collection, protocol parsing and bounded cross-platform coordination.

use std::cell::Cell;
use std::collections::{HashSet, VecDeque};
use std::error::Error as _;
use std::io::Read;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use encoding_rs::GBK;
use regex::Regex;
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, REFERER, USER_AGENT};
use scraper::{Html, Selector};
use serde_json::Value;
use time::{Duration as TimeDuration, OffsetDateTime};
use url::Url;

use crate::catalog::{google_trends_topic_url, HUGGING_FACE_TRENDING_MODELS_ENDPOINT};
use crate::database::open_database;
use crate::error::{AppError, AppResult};
#[cfg(test)]
use crate::models::SourceProxyMode;
use crate::models::{
    CollectedTopic, CollectionStats, ParsedFeed, PlatformSource, SourceConnectionTestResult,
    SourceConnectionTestRoute, SourceConnectionTestStatus, SourceParserType,
};
use crate::repository::TopicRepository;

const ITEMS_PER_PLATFORM: usize = 50;
const REQUEST_TIMEOUT_SECONDS: u64 = 15;
const MAX_WORKERS: usize = 6;
const SOURCE_TEST_TIMEOUT: Duration = Duration::from_secs(30);
const AUTO_COLLECTION_TIMEOUT: Duration = Duration::from_secs(60);
// Leave budget for a fallback even when the primary endpoint needs both retries.
const SOURCE_TEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(7);
const SOURCE_TEST_MAX_BYTES: usize = 8 * 1024 * 1024;
const LEGACY_HUGGING_FACE_MODELS_ENDPOINT: &str = "https://huggingface.co/api/models?sort=trendingScore&direction=-1&limit=50&expand=trendingScore,likes,downloads,createdAt,lastModified";
// A public headline index is a bounded fallback, not a replacement for the publisher feed.
const MIT_NEWS_INDEX_ENDPOINT: &str =
    "https://www.bing.com/news/search?q=site%3Atechnologyreview.com&format=rss";
const MIT_OFFICIAL_FEED: &str = "https://www.technologyreview.com/feed/";
const USER_AGENT_VALUE: &str =
    "TopicDeskStudio/0.1 (+https://github.com/dataelement/topic-desk-studio)";

/// Auto recovery and diagnostics bound each source, including all fallbacks.
/// Diagnostics additionally bound total bytes across routes and detail requests.
#[derive(Clone, Copy)]
struct SourceTestBudget {
    deadline: Instant,
    route_deadline: Instant,
    bytes_left: Option<usize>,
    diagnostic: bool,
}
thread_local! {
    static SOURCE_TEST_BUDGET: Cell<Option<SourceTestBudget>> = const { Cell::new(None) };
}
struct SourceTestScope(Option<SourceTestBudget>);
impl SourceTestScope {
    fn start(diagnostic: bool) -> Self {
        let deadline = Instant::now()
            + if diagnostic {
                SOURCE_TEST_TIMEOUT
            } else {
                AUTO_COLLECTION_TIMEOUT
            };
        Self(SOURCE_TEST_BUDGET.replace(Some(SourceTestBudget {
            deadline,
            route_deadline: deadline,
            bytes_left: diagnostic.then_some(SOURCE_TEST_MAX_BYTES),
            diagnostic,
        })))
    }
}
impl Drop for SourceTestScope {
    fn drop(&mut self) {
        SOURCE_TEST_BUDGET.set(self.0);
    }
}

fn source_test_request_timeout() -> AppResult<Duration> {
    SOURCE_TEST_BUDGET.with(|budget| match budget.get() {
        Some(budget) => {
            let remaining = budget
                .deadline
                .min(budget.route_deadline)
                .saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(AppError::CollectionConnection(
                    "数据源连接或读取超时，请重试".into(),
                ));
            }
            Ok(remaining.min(if budget.diagnostic {
                SOURCE_TEST_REQUEST_TIMEOUT
            } else {
                Duration::from_secs(REQUEST_TIMEOUT_SECONDS)
            }))
        }
        None => Ok(Duration::from_secs(REQUEST_TIMEOUT_SECONDS)),
    })
}

/// Native requests can report their final route without exposing transport errors
/// or causing the frontend to infer routing from human-readable error messages.
struct RoutedSource {
    result: AppResult<ParsedFeed>,
    route: Option<SourceConnectionTestRoute>,
    fallback_from: Option<SourceConnectionTestRoute>,
}

fn source_client(proxy: Option<&str>) -> AppResult<Client> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .redirect(reqwest::redirect::Policy::limited(8))
        .no_proxy();
    if let Some(proxy) = proxy {
        builder = builder.proxy(
            reqwest::Proxy::all(proxy)
                .map_err(|_| AppError::Collection("代理配置无效，请检查网络设置".into()))?,
        );
    }
    builder.build().map_err(collection_error)
}

/// Only a typed connection failure can switch once. Diagnostics reassess routes
/// independently; collection respects saved modes. Both share the same fetcher.
fn run_source_routes(
    platform: &PlatformSource,
    proxy_configured: bool,
    diagnostic: bool,
    mut fetch: impl FnMut(SourceConnectionTestRoute) -> AppResult<ParsedFeed>,
) -> RoutedSource {
    let plan = match if diagnostic {
        crate::collection_routing::diagnostic_route_plan(platform, proxy_configured)
    } else {
        crate::collection_routing::route_plan(platform, proxy_configured)
    } {
        Ok(plan) => plan,
        Err(error) => {
            return RoutedSource {
                result: Err(error),
                route: None,
                fallback_from: None,
            }
        }
    };
    // Explicit/no-alternate collection keeps its established per-request policy;
    // only recoverable Auto collection needs a total routing deadline.
    if !diagnostic && plan.fallback.is_none() {
        return RoutedSource {
            result: fetch(plan.preferred),
            route: Some(plan.preferred),
            fallback_from: None,
        };
    }
    let _scope = SourceTestScope::start(diagnostic);
    if plan.fallback.is_some() {
        SOURCE_TEST_BUDGET.with(|budget| {
            let mut current = budget.get().expect("source scope initialized");
            current.route_deadline = Instant::now() + SOURCE_TEST_TIMEOUT / 2;
            budget.set(Some(current));
        });
    }
    let first = fetch(plan.preferred);
    if !matches!(first, Err(AppError::CollectionConnection(_))) || plan.fallback.is_none() {
        return RoutedSource {
            result: first,
            route: Some(plan.preferred),
            fallback_from: None,
        };
    }
    let alternate = plan.fallback.expect("alternate checked");
    SOURCE_TEST_BUDGET.with(|budget| {
        let mut current = budget.get().expect("source scope initialized");
        current.route_deadline = current.deadline;
        budget.set(Some(current));
    });
    let second = fetch(alternate).map_err(|second| {
        let first = first.expect_err("only a connection failure can switch");
        AppError::Collection(format!("自动切换前：{first}；切换后：{second}"))
    });
    RoutedSource {
        result: second,
        route: Some(alternate),
        fallback_from: Some(plan.preferred),
    }
}

fn fetch_routed_source(
    platform: &PlatformSource,
    proxy: Option<&str>,
    diagnostic: bool,
) -> RoutedSource {
    run_source_routes(platform, proxy.is_some(), diagnostic, |route| {
        let client = source_client(if route == SourceConnectionTestRoute::Proxy {
            proxy
        } else {
            None
        })?;
        fetch_source_with(platform, diagnostic, |endpoint| {
            fetch_endpoint(&client, platform, endpoint)
        })
    })
}

/// Reassess routes once per transient source check, regardless of its saved mode.
/// Empty responses are warnings; browser-session sources keep their manual workflow.
pub fn test_source_connection(
    platform: &PlatformSource,
    proxy: Option<&str>,
) -> SourceConnectionTestResult {
    let started = Instant::now();
    if platform.code == "xiaohongshu" {
        return connection_test_result(platform, None, None, started, None);
    }
    let routed = fetch_routed_source(platform, proxy, true);
    connection_test_result(
        platform,
        routed.route,
        routed.fallback_from,
        started,
        Some(routed.result),
    )
}

fn connection_test_result(
    platform: &PlatformSource,
    route: Option<SourceConnectionTestRoute>,
    fallback_from: Option<SourceConnectionTestRoute>,
    started: Instant,
    result: Option<AppResult<ParsedFeed>>,
) -> SourceConnectionTestResult {
    let (status, item_count, error) = match result {
        None => (
            SourceConnectionTestStatus::Manual,
            0,
            Some("该来源依赖网页登录，请使用“登录采集”检查；连通性测试不会读取登录会话".into()),
        ),
        Some(Ok(feed)) if feed.topics.is_empty() => (SourceConnectionTestStatus::Empty, 0, None),
        Some(Ok(feed)) => (SourceConnectionTestStatus::Success, feed.topics.len(), None),
        Some(Err(error)) => (
            SourceConnectionTestStatus::Failed,
            0,
            Some(error.to_string()),
        ),
    };
    SourceConnectionTestResult {
        code: platform.code.clone(),
        status,
        elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        item_count,
        error,
        route,
        fallback_from,
    }
}

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
    let jobs = Arc::new(Mutex::new(jobs));
    let (sender, receiver) = mpsc::channel();
    let worker_count = MAX_WORKERS.min(job_count);

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let jobs = Arc::clone(&jobs);
            let sender = sender.clone();
            let proxy_url = network_settings.proxy_url.as_deref();
            scope.spawn(move || loop {
                let job = jobs.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some((platform, run_id)) = job else { break };
                let result = fetch_routed_source(&platform, proxy_url, false).result;
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
#[cfg(test)]
fn source_requires_proxy(platform: &PlatformSource) -> bool {
    match platform.proxy_mode {
        SourceProxyMode::Proxy => true,
        SourceProxyMode::Direct => false,
        SourceProxyMode::Auto => crate::catalog::default_proxy_mode(&platform.code) == "proxy",
    }
}

#[cfg(test)]
fn source_test_route(platform: &PlatformSource, proxy: Option<&str>) -> SourceConnectionTestRoute {
    crate::collection_routing::route_plan(platform, proxy.is_some())
        .unwrap()
        .preferred
}

/// Resolve shipped endpoint repairs while preserving every user-customized URL.
fn source_endpoints(platform: &PlatformSource) -> Vec<&str> {
    // Upgrade only the endpoints shipped by old installations. Once the user
    // edits this field, their configured endpoint must remain authoritative.
    let mut endpoints = if platform.code == "hugging-face"
        && (platform.endpoint_url == LEGACY_HUGGING_FACE_MODELS_ENDPOINT
            || platform.endpoint_url == "https://huggingface.co/api/daily_papers")
    {
        vec![HUGGING_FACE_TRENDING_MODELS_ENDPOINT]
    } else {
        vec![platform.endpoint_url.as_str()]
    };
    match platform.code.as_str() {
        "36kr" => endpoints.push("https://news.orz.ai/api/v1/dailynews/?platform=36kr"),
        "xueqiu" => endpoints.push("https://news.orz.ai/api/v1/dailynews/?platform=xueqiu"),
        "zhihu" => endpoints.push("https://news.orz.ai/api/v1/dailynews/?platform=zhihu"),
        "mit-technology-review"
            if platform.endpoint_url == MIT_OFFICIAL_FEED
                && matches!(platform.parser_type, SourceParserType::Builtin) =>
        {
            endpoints.push(MIT_NEWS_INDEX_ENDPOINT);
        }
        _ => {}
    }
    endpoints
}

/// Fetch one source with known fallbacks and reject an apparently successful empty parse.
/// Keep collection's endpoint ordering and bounded empty-cache retry behavior in
/// diagnostics. Tests retain empty parsed feeds to distinguish them from failures.
fn fetch_source_with(
    platform: &PlatformSource,
    keep_empty: bool,
    mut fetch: impl FnMut(&str) -> AppResult<ParsedFeed>,
) -> AppResult<ParsedFeed> {
    let endpoints = source_endpoints(platform);
    let mut failures = Vec::new();
    let mut empty_feed = None;
    let mut connection_only = true;
    for endpoint in endpoints {
        for empty_attempt in 0..2 {
            match fetch(endpoint) {
                Ok(feed) if !feed.topics.is_empty() => return Ok(feed),
                Ok(_) if empty_attempt == 0 && endpoint.contains("news.orz.ai") => {
                    connection_only = false;
                    // The aggregator briefly publishes an empty list while
                    // rotating its cache; retry once before declaring failure.
                    let remaining = source_test_request_timeout()
                        .map_err(|error| AppError::Collection(error.to_string()))?;
                    std::thread::sleep(Duration::from_millis(800).min(remaining));
                    continue;
                }
                Ok(feed) => {
                    connection_only = false;
                    empty_feed = Some(feed);
                    failures.push(if platform.code == "xiaohongshu" {
                        "xiaohongshu 页面已改为动态签名加载，公开 HTML 不含热点数据".into()
                    } else {
                        format!("{} 未解析到有效热点", platform.code)
                    });
                }
                Err(AppError::Collection(message)) => {
                    connection_only = false;
                    empty_feed = None;
                    failures.push(message);
                }
                Err(AppError::CollectionConnection(message)) => {
                    empty_feed = None;
                    failures.push(message);
                }
                Err(error) => {
                    connection_only = false;
                    empty_feed = None;
                    failures.push(error.to_string());
                }
            }
            break;
        }
    }
    if keep_empty {
        if let Some(feed) = empty_feed {
            return Ok(feed);
        }
    }
    Err(if connection_only {
        AppError::CollectionConnection(failures.join("；"))
    } else {
        AppError::Collection(failures.join("；"))
    })
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
    let bytes = response_bytes(response)?;
    if platform.code == "mit-technology-review" && endpoint == MIT_NEWS_INDEX_ENDPOINT {
        return parse_mit_news_index(&bytes);
    }

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

/// Bound total diagnostic response bytes across all fallback and detail requests;
/// regular collection retains its established behavior.
fn response_bytes(response: Response) -> AppResult<Vec<u8>> {
    let Some(budget) = SOURCE_TEST_BUDGET
        .get()
        .filter(|budget| budget.bytes_left.is_some())
    else {
        return response
            .bytes()
            .map(|bytes| bytes.to_vec())
            .map_err(|error| request_error("数据源", "", error));
    };
    let remaining = budget.bytes_left.expect("diagnostic byte budget checked");
    let bytes = read_test_response(response, remaining)?;
    SOURCE_TEST_BUDGET.set(Some(SourceTestBudget {
        bytes_left: Some(remaining - bytes.len()),
        ..budget
    }));
    Ok(bytes)
}

fn read_test_response(reader: impl Read, limit: usize) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let read = reader.take(limit as u64 + 1).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > limit {
        // A reset after a partial body still consumed its byte allowance; a new
        // route must not receive a fresh 8 MiB budget for the same diagnostic.
        SOURCE_TEST_BUDGET.with(|budget| {
            if let Some(mut current) = budget.get() {
                current.bytes_left = current
                    .bytes_left
                    .map(|left| left.saturating_sub(bytes.len()));
                budget.set(Some(current));
            }
        });
    }
    read.map_err(|error| {
        let message = "读取数据源响应失败或超时".into();
        if connection_error_chain(&error) {
            AppError::CollectionConnection(message)
        } else {
            AppError::Collection(message)
        }
    })?;
    if bytes.len() > limit {
        return Err(AppError::Collection("数据源测试响应超过 8 MiB 限制".into()));
    }
    Ok(bytes)
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
        let timeout = source_test_request_timeout()?;
        match client
            .get(endpoint)
            .headers(headers.clone())
            .timeout(timeout)
            .send()
        {
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
    let connection = !error.is_status() && connection_error_chain(&error);
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
    let message = format!("{code} · {host}：{detail}");
    if connection {
        AppError::CollectionConnection(message)
    } else {
        AppError::Collection(message)
    }
}

/// Inspect typed causes only. Words in a URL, parser error, or HTTP body must
/// never authorize sending a request through a different network route.
fn connection_error_chain(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut cause = Some(error);
    while let Some(error) = cause {
        if error
            .downcast_ref::<reqwest::Error>()
            .is_some_and(|error| error.is_connect() || error.is_timeout())
        {
            return true;
        }
        if error.downcast_ref::<std::io::Error>().is_some_and(|error| {
            matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
            )
        }) {
            return true;
        }
        cause = error.source();
    }
    false
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
        let url = title
            .as_deref()
            .and_then(|title| google_trends_topic_url(code, title))
            .or_else(|| entry.links.first().map(|link| link.href.clone()));
        if let (Some(title), Some(url)) = (title, url) {
            if valid_http_url(&url) && !title.is_empty() {
                topics.push(CollectedTopic {
                    platform_code: code.to_owned(),
                    // Google supplies the same RSS URL for every entry and feed-rs
                    // may derive identical IDs from it; the trend phrase is the
                    // only stable per-entry identity available in this feed.
                    stable_id: google_trends_topic_url(code, &title)
                        .map(|_| title.clone())
                        .or_else(|| (!entry.id.trim().is_empty()).then_some(entry.id)),
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

/// Recover only official article permalinks from the public news index. Index
/// GUIDs are unstable tracking URLs; WordPress post IDs must match the original
/// publisher feed's GUIDs so switching transports never creates duplicates.
fn mit_index_article(value: &str) -> Option<(String, String)> {
    let mut url = Url::parse(value).ok()?;
    if matches!(url.host_str(), Some("www.bing.com" | "bing.com"))
        && url.path() == "/news/apiclick.aspx"
    {
        let targets = url
            .query_pairs()
            .filter(|(key, _)| key == "url")
            .map(|(_, value)| value.into_owned())
            .collect::<Vec<_>>();
        if targets.len() != 1 {
            return None;
        }
        url = Url::parse(&targets[0]).ok()?;
    }
    if !matches!(url.scheme(), "http" | "https")
        || !matches!(
            url.host_str(),
            Some("www.technologyreview.com" | "technologyreview.com")
        )
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let pattern = Regex::new(
        r"^/[0-9]{4}/(?:0[1-9]|1[0-2])/(?:0[1-9]|[12][0-9]|3[01])/([1-9][0-9]*)/[^/]+/?$",
    )
    .ok()?;
    let post_id = pattern.captures(url.path())?.get(1)?.as_str().to_owned();
    url.set_scheme("https").ok()?;
    url.set_host(Some("www.technologyreview.com")).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    Some((
        url.into(),
        format!("https://www.technologyreview.com/?p={post_id}"),
    ))
}

/// Only publisher articles are eligible: reject author/profile pages, unrelated
/// publishers and repeated index entries. Empty results still fail collection.
fn parse_mit_news_index(bytes: &[u8]) -> AppResult<ParsedFeed> {
    let mut feed = parse_feed("mit-technology-review", bytes)?;
    let mut seen = HashSet::new();
    feed.topics = feed
        .topics
        .into_iter()
        .filter_map(|mut topic| {
            let (url, stable_id) = mit_index_article(&topic.url)?;
            if !seen.insert(stable_id.clone()) {
                return None;
            }
            topic.url = url;
            topic.stable_id = Some(stable_id);
            Some(topic)
        })
        .collect();
    for (index, topic) in feed.topics.iter_mut().enumerate() {
        topic.rank = (index + 1) as u32;
    }
    feed.invalid_count = feed.fetched_count.saturating_sub(feed.topics.len() as u32);
    Ok(feed)
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
    let ids: Vec<Value> = serde_json::from_slice(&response_bytes(request(
        client,
        &platform.code,
        &format!("{base}/topstories.json"),
    )?)?)
    .map_err(collection_error)?;
    let mut topics = Vec::new();
    let fetched_count = ids.len().min(ITEMS_PER_PLATFORM) as u32;
    for (index, id) in ids.into_iter().take(ITEMS_PER_PLATFORM).enumerate() {
        let item: Value = serde_json::from_slice(&response_bytes(request(
            client,
            &platform.code,
            &format!("{base}/item/{id}.json"),
        )?)?)
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
    let payload: Value = serde_json::from_slice(&response_bytes(request(
        client,
        &platform.code,
        &endpoint,
    )?)?)
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

    fn diagnostic_platform(code: &str) -> PlatformSource {
        PlatformSource {
            id: 7,
            code: code.into(),
            endpoint_url: "https://example.org/feed".into(),
            parser_type: SourceParserType::Rss,
            parser_config: Default::default(),
            proxy_mode: SourceProxyMode::Auto,
        }
    }

    fn fixture_feed(code: &str) -> ParsedFeed {
        parse_feed(code, include_bytes!("../../tests/fixtures/qbitai.xml")).unwrap()
    }

    #[test]
    fn auto_recovers_from_typed_transport_failures_in_both_directions_once() {
        use SourceConnectionTestRoute::{Direct, Proxy};
        for (code, routes) in [("qbitai", [Direct, Proxy]), ("github", [Proxy, Direct])] {
            for diagnostic in [true, false] {
                let source = diagnostic_platform(code);
                let mut attempts = Vec::new();
                let routed = run_source_routes(&source, true, diagnostic, |route| {
                    attempts.push(route);
                    if attempts.len() == 1 {
                        Err(AppError::CollectionConnection(
                            "fixture transport error".into(),
                        ))
                    } else {
                        Ok(fixture_feed(code))
                    }
                });
                assert_eq!(attempts, routes);
                assert!(routed.result.is_ok());
                assert_eq!(routed.route, Some(routes[1]));
                assert_eq!(routed.fallback_from, Some(routes[0]));
                assert!(SOURCE_TEST_BUDGET.get().is_none());
            }
        }
    }

    #[test]
    fn auto_never_switches_for_http_parse_or_empty_results_or_error_words() {
        use SourceConnectionTestRoute::Direct;
        let source = diagnostic_platform("qbitai");
        for text in [
            "HTTP 401",
            "HTTP 403",
            "HTTP 404",
            "HTTP 429",
            "HTTP 503",
            "Feed 无效",
            "https://tls.example.com DNS reset timeout",
        ] {
            let mut attempts = 0;
            let routed = run_source_routes(&source, true, true, |route| {
                attempts += 1;
                assert_eq!(route, Direct);
                Err(AppError::Collection(text.into()))
            });
            assert_eq!(attempts, 1, "{text}");
            assert!(routed.fallback_from.is_none());
        }
        let mut attempts = 0;
        let routed = run_source_routes(&source, true, true, |_| {
            attempts += 1;
            Ok(ParsedFeed {
                topics: vec![],
                fetched_count: 0,
                invalid_count: 0,
            })
        });
        assert_eq!(attempts, 1);
        assert!(routed.result.unwrap().topics.is_empty());
        assert!(routed.fallback_from.is_none());
    }

    #[test]
    fn explicit_collection_modes_missing_proxy_and_local_auto_cannot_switch_routes() {
        for mode in [SourceProxyMode::Direct, SourceProxyMode::Proxy] {
            let mut source = diagnostic_platform("qbitai");
            source.proxy_mode = mode;
            let mut attempts = Vec::new();
            let routed = run_source_routes(&source, true, false, |route| {
                attempts.push(route);
                Err(AppError::CollectionConnection("fixture".into()))
            });
            assert_eq!(attempts.len(), 1);
            assert!(routed.fallback_from.is_none());
            if mode == SourceProxyMode::Proxy {
                let absent = run_source_routes(&source, false, false, |_| {
                    panic!("missing proxy must not request")
                });
                assert!(absent.route.is_none());
                assert!(absent
                    .result
                    .unwrap_err()
                    .to_string()
                    .contains("请先保存代理地址"));
            }
        }
        let mut source = diagnostic_platform("qbitai");
        source.endpoint_url = "http://localhost/feed".into();
        let mut attempts = 0;
        let routed = run_source_routes(&source, true, true, |_| {
            attempts += 1;
            Err(AppError::CollectionConnection("fixture".into()))
        });
        assert_eq!(attempts, 1);
        assert!(routed.fallback_from.is_none());
    }

    #[test]
    fn every_diagnostic_restarts_route_judgment_without_rewriting_saved_modes() {
        use SourceConnectionTestRoute::{Direct, Proxy};
        for mode in [
            SourceProxyMode::Direct,
            SourceProxyMode::Proxy,
            SourceProxyMode::Auto,
        ] {
            for (code, expected) in [("qbitai", [Direct, Proxy]), ("github", [Proxy, Direct])] {
                let mut source = diagnostic_platform(code);
                source.proxy_mode = mode;
                let mut all_attempts = Vec::new();
                for _ in 0..2 {
                    let mut attempt = 0;
                    let routed = run_source_routes(&source, true, true, |route| {
                        all_attempts.push(route);
                        attempt += 1;
                        if attempt == 1 {
                            Err(AppError::CollectionConnection("fixture reset".into()))
                        } else {
                            Ok(fixture_feed(code))
                        }
                    });
                    assert!(routed.result.is_ok());
                    assert_eq!(routed.route, Some(expected[1]));
                    assert_eq!(routed.fallback_from, Some(expected[0]));
                    assert_eq!(source.proxy_mode, mode);
                    assert!(SOURCE_TEST_BUDGET.get().is_none());
                }
                assert_eq!(all_attempts, [expected, expected].concat());
            }
        }
    }

    #[test]
    fn diagnostics_probe_direct_when_no_proxy_exists_and_do_not_proxy_local_sources() {
        let mut source = diagnostic_platform("github");
        source.proxy_mode = SourceProxyMode::Proxy;
        let mut attempts = Vec::new();
        let routed = run_source_routes(&source, false, true, |route| {
            attempts.push(route);
            Ok(fixture_feed(&source.code))
        });
        assert_eq!(attempts, [SourceConnectionTestRoute::Direct]);
        assert!(routed.result.is_ok());
        assert_eq!(routed.route, Some(SourceConnectionTestRoute::Direct));
        assert!(routed.fallback_from.is_none());
        assert_eq!(source.proxy_mode, SourceProxyMode::Proxy);
        for endpoint in [
            "http://localhost/feed",
            "http://127.0.0.1/feed",
            "http://[::1]/feed",
            "http://10.1.1.1/feed",
        ] {
            source.endpoint_url = endpoint.into();
            let mut attempts = Vec::new();
            let routed = run_source_routes(&source, true, true, |route| {
                attempts.push(route);
                Err(AppError::CollectionConnection("fixture reset".into()))
            });
            assert_eq!(attempts, [SourceConnectionTestRoute::Direct]);
            assert_eq!(routed.route, Some(SourceConnectionTestRoute::Direct));
            assert!(routed.fallback_from.is_none());
            assert_eq!(source.proxy_mode, SourceProxyMode::Proxy);
        }
    }

    #[test]
    fn a_failed_alternate_reports_both_failures_and_serializes_actual_route() {
        let source = diagnostic_platform("qbitai");
        let routed = run_source_routes(&source, true, true, |route| match route {
            SourceConnectionTestRoute::Direct => {
                Err(AppError::CollectionConnection("direct TLS failure".into()))
            }
            SourceConnectionTestRoute::Proxy => Err(AppError::Collection("HTTP 403".into())),
        });
        let result = connection_test_result(
            &source,
            routed.route,
            routed.fallback_from,
            Instant::now(),
            Some(routed.result),
        );
        assert_eq!(result.status, SourceConnectionTestStatus::Failed);
        let error = result.error.as_deref().unwrap();
        assert!(error.contains("direct TLS failure") && error.contains("HTTP 403"));
        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["route"], "proxy");
        assert_eq!(json["fallbackFrom"], "direct");
    }

    #[test]
    fn budgets_reserve_an_alternate_without_shortening_explicit_collection() {
        let source = diagnostic_platform("qbitai");
        for diagnostic in [true, false] {
            let mut attempts = 0;
            let result = run_source_routes(&source, true, diagnostic, |_| {
                attempts += 1;
                let budget = SOURCE_TEST_BUDGET.get().unwrap();
                if attempts == 1 {
                    assert!(budget.route_deadline < budget.deadline);
                    assert!(
                        budget.route_deadline.duration_since(Instant::now())
                            <= Duration::from_secs(15)
                    );
                    Err(AppError::CollectionConnection("fixture".into()))
                } else {
                    assert_eq!(budget.route_deadline, budget.deadline);
                    assert_eq!(
                        budget.bytes_left,
                        diagnostic.then_some(SOURCE_TEST_MAX_BYTES)
                    );
                    Ok(fixture_feed(&source.code))
                }
            });
            assert!(result.result.is_ok());
        }
        let mut explicit = source;
        explicit.proxy_mode = SourceProxyMode::Direct;
        run_source_routes(&explicit, true, false, |_| {
            assert!(SOURCE_TEST_BUDGET.get().is_none());
            Ok(fixture_feed(&explicit.code))
        });
        run_source_routes(&explicit, false, true, |_| {
            let budget = SOURCE_TEST_BUDGET.get().unwrap();
            assert_eq!(budget.route_deadline, budget.deadline);
            Ok(fixture_feed(&explicit.code))
        });
    }

    #[test]
    fn typed_connection_detection_ignores_error_messages_and_identifies_reset_causes() {
        for kind in [
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionRefused,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::BrokenPipe,
        ] {
            assert!(connection_error_chain(&std::io::Error::new(
                kind,
                "no keywords"
            )));
        }
        assert!(!connection_error_chain(&std::io::Error::other(
            "TLS DNS timeout reset https://tls.example.com"
        )));
        assert!(!connection_error_chain(&AppError::Collection(
            "TLS 安全连接失败".into()
        )));
        let platform = diagnostic_platform("36kr");
        let result = fetch_source_with(&platform, true, |endpoint| {
            if endpoint == platform.endpoint_url {
                Err(AppError::Collection("HTTP 403".into()))
            } else {
                Err(AppError::CollectionConnection("TLS failure".into()))
            }
        });
        assert!(matches!(result, Err(AppError::Collection(_))));
    }

    #[test]
    fn a_partial_body_reset_consumes_the_shared_diagnostic_byte_budget() {
        struct Partial(bool);
        impl Read for Partial {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                if self.0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "fixture",
                    ));
                }
                self.0 = true;
                output[..2].copy_from_slice(b"ab");
                Ok(2)
            }
        }
        let _scope = SourceTestScope::start(true);
        assert!(matches!(
            read_test_response(Partial(false), SOURCE_TEST_MAX_BYTES),
            Err(AppError::CollectionConnection(_))
        ));
        assert_eq!(
            SOURCE_TEST_BUDGET.get().unwrap().bytes_left,
            Some(SOURCE_TEST_MAX_BYTES - 2)
        );
    }

    #[test]
    fn connection_test_uses_fallback_data_and_preserves_collection_empty_policy() {
        let platform = diagnostic_platform("36kr");
        let endpoints = std::cell::RefCell::new(Vec::new());
        let feed = fetch_source_with(&platform, true, |endpoint| {
            endpoints.borrow_mut().push(endpoint.to_owned());
            if endpoint.contains("news.orz.ai") {
                parse_feed(
                    &platform.code,
                    include_bytes!("../../tests/fixtures/qbitai.xml"),
                )
            } else {
                Err(AppError::Collection("HTTP 403".into()))
            }
        })
        .unwrap();
        assert!(!feed.topics.is_empty());
        assert_eq!(*endpoints.borrow(), source_endpoints(&platform));
        let platform = diagnostic_platform("custom-rss");
        let empty = || {
            Ok(ParsedFeed {
                topics: vec![],
                fetched_count: 0,
                invalid_count: 0,
            })
        };
        let parsed = fetch_source_with(&platform, true, |_| empty()).unwrap();
        let result = connection_test_result(
            &platform,
            Some(SourceConnectionTestRoute::Direct),
            None,
            Instant::now(),
            Some(Ok(parsed)),
        );
        assert_eq!(result.status, SourceConnectionTestStatus::Empty);
        assert_eq!(result.item_count, 0);
        assert!(fetch_source_with(&platform, false, |_| empty()).is_err());
    }

    #[test]
    fn connection_test_classifies_parsed_data_http_failures_and_verification_pages() {
        let platform = diagnostic_platform("qbitai");
        let parsed = parse_feed(
            &platform.code,
            include_bytes!("../../tests/fixtures/qbitai.xml"),
        );
        let result = connection_test_result(
            &platform,
            Some(SourceConnectionTestRoute::Proxy),
            None,
            Instant::now(),
            Some(parsed),
        );
        assert_eq!(result.status, SourceConnectionTestStatus::Success);
        assert!(result.item_count > 0);
        assert!(result.error.is_none());
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["route"], "proxy");
        assert_eq!(json["status"], "success");
        assert!(json.get("elapsedMs").is_some());
        assert!(json.get("itemCount").is_some());
        for failure in [
            Err(AppError::Collection("HTTP 403 Forbidden".into())),
            parse_feed(
                &platform.code,
                b"<html><title>Verify you are human</title></html>",
            ),
        ] {
            let result = connection_test_result(
                &platform,
                Some(SourceConnectionTestRoute::Direct),
                None,
                Instant::now(),
                Some(failure),
            );
            assert_eq!(result.status, SourceConnectionTestStatus::Failed);
            assert!(result.error.is_some());
            assert_eq!(result.item_count, 0);
        }
    }

    #[test]
    fn browser_session_sources_never_make_a_network_probe_or_claim_success() {
        for mode in [
            SourceProxyMode::Direct,
            SourceProxyMode::Proxy,
            SourceProxyMode::Auto,
        ] {
            let mut source = diagnostic_platform("xiaohongshu");
            source.proxy_mode = mode;
            let result = test_source_connection(&source, Some("invalid-proxy"));
            assert_eq!(result.status, SourceConnectionTestStatus::Manual);
            assert!(result.route.is_none());
            assert!(result.fallback_from.is_none());
            assert_eq!(result.item_count, 0);
            assert!(result.error.as_deref().unwrap().contains("登录采集"));
            assert_eq!(source.proxy_mode, mode);
        }
    }

    #[test]
    fn connection_test_resolves_the_same_proxy_routes_as_collection() {
        let mut platform = diagnostic_platform("github");
        let proxy = Some("http://127.0.0.1:7897");
        assert_eq!(
            source_test_route(&platform, proxy),
            SourceConnectionTestRoute::Proxy
        );
        assert_eq!(
            source_test_route(&platform, None),
            SourceConnectionTestRoute::Direct
        );
        platform.proxy_mode = SourceProxyMode::Direct;
        assert_eq!(
            source_test_route(&platform, proxy),
            SourceConnectionTestRoute::Direct
        );
        platform.code = "qbitai".into();
        platform.proxy_mode = SourceProxyMode::Auto;
        assert_eq!(
            source_test_route(&platform, proxy),
            SourceConnectionTestRoute::Direct
        );
        platform.proxy_mode = SourceProxyMode::Proxy;
        assert_eq!(
            source_test_route(&platform, proxy),
            SourceConnectionTestRoute::Proxy
        );
    }

    #[test]
    fn an_empty_primary_does_not_hide_a_later_timeout_or_response_failure() {
        let platform = diagnostic_platform("36kr");
        let result = fetch_source_with(&platform, true, |endpoint| {
            if endpoint == platform.endpoint_url {
                Ok(ParsedFeed {
                    topics: vec![],
                    fetched_count: 0,
                    invalid_count: 0,
                })
            } else {
                Err(AppError::Collection("读取数据源响应失败或超时".into()))
            }
        });
        assert!(result.is_err());
    }

    #[test]
    fn connection_test_deadlines_and_body_limits_are_enforced_and_scoped() {
        assert!(SOURCE_TEST_BUDGET.get().is_none());
        {
            let _scope = SourceTestScope::start(true);
            assert_eq!(
                SOURCE_TEST_BUDGET.get().unwrap().bytes_left,
                Some(SOURCE_TEST_MAX_BYTES)
            );
            assert!(source_test_request_timeout().unwrap() <= Duration::from_secs(15));
            SOURCE_TEST_BUDGET.set(Some(SourceTestBudget {
                deadline: Instant::now() - Duration::from_millis(1),
                route_deadline: Instant::now(),
                bytes_left: Some(0),
                diagnostic: true,
            }));
            assert!(source_test_request_timeout().is_err());
        }
        assert!(SOURCE_TEST_BUDGET.get().is_none());
        assert_eq!(read_test_response(&b"1234"[..], 4).unwrap(), b"1234");
        assert!(read_test_response(&b"12345"[..], 4).is_err());
        assert!(read_test_response(&b"x"[..], 0).is_err());
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fixture failure"))
            }
        }
        assert!(read_test_response(Broken, 4).is_err());
    }

    /// The shipped fixtures protect RSS and Atom compatibility independently of network access.
    #[test]
    fn parses_rss_fixture() {
        let bytes = include_bytes!("../../tests/fixtures/qbitai.xml");
        let feed = parse_feed("qbitai", bytes).expect("fixture should parse");
        assert!(!feed.topics.is_empty());
        assert_eq!(feed.topics[0].platform_code, "qbitai");
    }

    #[test]
    fn google_trends_rss_items_do_not_open_the_xml_feed_or_share_an_identity() {
        let bytes = br#"<?xml version="1.0" encoding="UTF-8"?>
          <rss version="2.0"><channel><title>Daily Search Trends</title>
            <item><title>sa vs aus</title><link>https://trends.google.com/trending/rss?geo=US</link></item>
            <item><title>new model</title><link>https://trends.google.com/trending/rss?geo=US</link></item>
          </channel></rss>"#;
        let feed = parse_feed("google-trends-global", bytes).expect("feed should parse");
        assert_eq!(feed.topics.len(), 2);
        assert_eq!(feed.topics[0].stable_id.as_deref(), Some("sa vs aus"));
        assert_eq!(feed.topics[1].stable_id.as_deref(), Some("new model"));
        assert!(feed.topics.iter().all(|topic| {
            topic.url.contains("/trends/explore?") && !topic.url.contains("/trending/rss")
        }));
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

    #[test]
    fn mit_fallback_is_secondary_and_preserves_custom_source_configuration() {
        let mut platform = PlatformSource {
            id: 1,
            code: "mit-technology-review".into(),
            endpoint_url: MIT_OFFICIAL_FEED.into(),
            parser_type: SourceParserType::Builtin,
            parser_config: Default::default(),
            proxy_mode: SourceProxyMode::Direct,
        };
        assert_eq!(
            source_endpoints(&platform),
            vec![MIT_OFFICIAL_FEED, MIT_NEWS_INDEX_ENDPOINT]
        );
        platform.endpoint_url = "https://example.org/custom.xml".into();
        assert_eq!(
            source_endpoints(&platform),
            vec!["https://example.org/custom.xml"]
        );
        platform.endpoint_url = MIT_OFFICIAL_FEED.into();
        platform.parser_type = SourceParserType::Rss;
        assert_eq!(source_endpoints(&platform), vec![MIT_OFFICIAL_FEED]);
    }

    #[test]
    fn mit_index_filters_profiles_duplicates_and_preserves_official_topic_identity() {
        let original = br#"<rss version="2.0"><channel><title>MIT</title><item>
            <title>Original title</title><link>https://www.technologyreview.com/2026/10/26/1147645/story/</link>
            <guid isPermaLink="false">https://www.technologyreview.com/?p=1147645</guid>
            </item></channel></rss>"#;
        let index = br#"<rss version="2.0"><channel><title>News index</title>
            <item><title>Author</title><link>https://www.technologyreview.com/author/person/</link></item>
            <item><title>Index title</title><link>https://www.bing.com/news/apiclick.aspx?url=https%3A%2F%2Fwww.technologyreview.com%2F2026%2F10%2F26%2F1147645%2Fstory%2F%3Fref%3Dindex</link><guid>tracking-guid</guid></item>
            <item><title>Duplicate</title><link>https://technologyreview.com/2026/10/26/1147645/story/</link></item>
            <item><title>Other publication</title><link>https://example.org/2026/10/26/1147645/story/</link></item>
            </channel></rss>"#;
        let feed = parse_mit_news_index(index).unwrap();
        assert_eq!(feed.fetched_count, 4);
        assert_eq!(feed.invalid_count, 3);
        assert_eq!(feed.topics.len(), 1);
        assert_eq!(feed.topics[0].rank, 1);
        assert_eq!(feed.topics[0].title, "Index title");
        assert_eq!(
            feed.topics[0].url,
            "https://www.technologyreview.com/2026/10/26/1147645/story/"
        );
        let official = parse_feed("mit-technology-review", original).unwrap();
        assert_eq!(
            crate::identity::identify_topic(&feed.topics[0])
                .unwrap()
                .hash,
            crate::identity::identify_topic(&official.topics[0])
                .unwrap()
                .hash
        );
    }

    #[test]
    fn mit_index_rejects_untrusted_links_and_invalid_feeds() {
        for url in [
            "javascript:alert(1)",
            "file:///2026/10/26/123/story/",
            "https://www.technologyreview.com.evil.org/2026/10/26/123/story/",
            "https://www.technologyreview.com@evil.org/2026/10/26/123/story/",
            "https://user:password@www.technologyreview.com/2026/10/26/123/story/",
            "https://www.technologyreview.com:8443/2026/10/26/123/story/",
            "https://www.technologyreview.com/2026/13/26/123/story/",
            "https://www.technologyreview.com/2026/10/26/0/story/",
            "https://www.bing.com/news/apiclick.aspx?url=javascript%3Aalert(1)",
            "https://www.bing.com/news/apiclick.aspx?url=x&url=y",
            "https://evil.org/redirect?url=https://www.technologyreview.com/2026/10/26/123/story/",
        ] {
            assert!(mit_index_article(url).is_none(), "{url}");
        }
        assert!(parse_mit_news_index(b"not an RSS document").is_err());
        let empty = parse_mit_news_index(
            br#"<rss version="2.0"><channel><title>Empty</title></channel></rss>"#,
        )
        .unwrap();
        assert!(empty.topics.is_empty());
    }

    #[test]
    fn repairs_only_shipped_hugging_face_endpoints_and_expands_fields_individually() {
        let mut platform = PlatformSource {
            id: 1,
            code: "hugging-face".into(),
            endpoint_url: LEGACY_HUGGING_FACE_MODELS_ENDPOINT.into(),
            parser_type: SourceParserType::Builtin,
            parser_config: Default::default(),
            proxy_mode: SourceProxyMode::Proxy,
        };
        for legacy in [
            LEGACY_HUGGING_FACE_MODELS_ENDPOINT,
            "https://huggingface.co/api/daily_papers",
        ] {
            platform.endpoint_url = legacy.into();
            assert_eq!(
                source_endpoints(&platform),
                vec![HUGGING_FACE_TRENDING_MODELS_ENDPOINT]
            );
        }
        let url = Url::parse(HUGGING_FACE_TRENDING_MODELS_ENDPOINT).unwrap();
        let expanded = url
            .query_pairs()
            .filter(|(name, _)| name == "expand")
            .map(|(_, value)| value.into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            expanded,
            [
                "trendingScore",
                "likes",
                "downloads",
                "createdAt",
                "lastModified"
            ]
        );
        for custom in [
            "https://huggingface.co/api/models?limit=10",
            "https://example.org/api/daily_papers",
        ] {
            platform.endpoint_url = custom.into();
            assert_eq!(source_endpoints(&platform), vec![custom]);
        }
        platform.code = "custom-models".into();
        platform.endpoint_url = LEGACY_HUGGING_FACE_MODELS_ENDPOINT.into();
        assert_eq!(
            source_endpoints(&platform),
            vec![LEGACY_HUGGING_FACE_MODELS_ENDPOINT]
        );
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
