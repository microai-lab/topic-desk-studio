//! Pure source routing: saved modes govern collection; diagnostics reassess routes independently.
use crate::{
    error::{AppError, AppResult},
    models::{PlatformSource, SourceConnectionTestRoute, SourceProxyMode},
};
use std::net::IpAddr;

/// At most one alternate route is considered; no settings are rewritten or cached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoutePlan {
    pub preferred: SourceConnectionTestRoute,
    pub fallback: Option<SourceConnectionTestRoute>,
}

/// Keep loopback and local-network sources out of automatic proxy recovery.
/// Hostname checks cannot identify public names that resolve to private addresses.
fn local_endpoint(url: &url::Url) -> bool {
    let Some(host) = url.host() else { return true };
    match host {
        url::Host::Domain(name) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            !name.contains('.') || name.ends_with(".localhost") || name.ends_with(".local")
        }
        url::Host::Ipv4(ip) => local_ip(IpAddr::V4(ip)),
        url::Host::Ipv6(ip) => local_ip(IpAddr::V6(ip)),
    }
}

fn local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
        }
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(|ip| local_ip(IpAddr::V4(ip)))
            .unwrap_or_else(|| {
                ip.is_loopback()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || ip.is_unspecified()
                    || ip.is_multicast()
            }),
    }
}

/// Validate the configured address before making requests. Auto retains catalog
/// preference and may try its opposite; Direct/Proxy always remain explicit.
pub(crate) fn route_plan(
    platform: &PlatformSource,
    proxy_configured: bool,
) -> AppResult<RoutePlan> {
    let endpoint = url::Url::parse(&platform.endpoint_url)
        .map_err(|_| AppError::InvalidInput("采集地址无效".into()))?;
    if !matches!(endpoint.scheme(), "http" | "https")
        || endpoint.host().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
    {
        return Err(AppError::InvalidInput(
            "采集地址必须是不含凭据的 HTTP(S) URL".into(),
        ));
    }
    use SourceConnectionTestRoute::{Direct, Proxy};
    match platform.proxy_mode {
        SourceProxyMode::Direct => Ok(RoutePlan {
            preferred: Direct,
            fallback: None,
        }),
        SourceProxyMode::Proxy if !proxy_configured => Err(AppError::InvalidInput(
            "该来源已指定代理，请先保存代理地址".into(),
        )),
        SourceProxyMode::Proxy => Ok(RoutePlan {
            preferred: Proxy,
            fallback: None,
        }),
        SourceProxyMode::Auto if !proxy_configured || local_endpoint(&endpoint) => Ok(RoutePlan {
            preferred: Direct,
            fallback: None,
        }),
        SourceProxyMode::Auto => {
            let preferred = if crate::catalog::default_proxy_mode(&platform.code) == "proxy" {
                Proxy
            } else {
                Direct
            };
            Ok(RoutePlan {
                preferred,
                fallback: Some(if preferred == Direct { Proxy } else { Direct }),
            })
        }
    }
}

/// A user-triggered diagnostic always reassesses the current network like Auto,
/// without changing the source used later by collection or caching this result.
/// Reusing Auto policy keeps local-address and missing-proxy behavior identical.
pub(crate) fn diagnostic_route_plan(
    platform: &PlatformSource,
    proxy_configured: bool,
) -> AppResult<RoutePlan> {
    let automatic = PlatformSource {
        proxy_mode: SourceProxyMode::Auto,
        ..platform.clone()
    };
    route_plan(&automatic, proxy_configured)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SourceParserType;
    fn source(code: &str, mode: SourceProxyMode, endpoint: &str) -> PlatformSource {
        PlatformSource {
            id: 1,
            code: code.into(),
            endpoint_url: endpoint.into(),
            parser_type: SourceParserType::Builtin,
            parser_config: Default::default(),
            proxy_mode: mode,
        }
    }
    #[test]
    fn automatic_modes_keep_catalog_preference_and_require_a_configured_alternate() {
        use SourceConnectionTestRoute::{Direct, Proxy};
        assert_eq!(
            route_plan(
                &source(
                    "qbitai",
                    SourceProxyMode::Auto,
                    "https://www.qbitai.com/feed"
                ),
                true
            )
            .unwrap(),
            RoutePlan {
                preferred: Direct,
                fallback: Some(Proxy)
            }
        );
        assert_eq!(
            route_plan(
                &source(
                    "github",
                    SourceProxyMode::Auto,
                    "https://github.com/trending"
                ),
                true
            )
            .unwrap(),
            RoutePlan {
                preferred: Proxy,
                fallback: Some(Direct)
            }
        );
        assert_eq!(
            route_plan(
                &source(
                    "github",
                    SourceProxyMode::Auto,
                    "https://github.com/trending"
                ),
                false
            )
            .unwrap(),
            RoutePlan {
                preferred: Direct,
                fallback: None
            }
        );
    }
    #[test]
    fn explicit_modes_never_switch_and_proxy_requires_an_address() {
        use SourceConnectionTestRoute::{Direct, Proxy};
        assert_eq!(
            route_plan(
                &source("github", SourceProxyMode::Direct, "https://github.com"),
                true
            )
            .unwrap(),
            RoutePlan {
                preferred: Direct,
                fallback: None
            }
        );
        assert_eq!(
            route_plan(
                &source("custom", SourceProxyMode::Proxy, "http://localhost/feed"),
                true
            )
            .unwrap(),
            RoutePlan {
                preferred: Proxy,
                fallback: None
            }
        );
        assert!(route_plan(
            &source("custom", SourceProxyMode::Proxy, "https://example.com/feed"),
            false
        )
        .is_err());
    }

    #[test]
    fn diagnostics_reassess_all_saved_modes_without_mutating_collection_policy() {
        use SourceConnectionTestRoute::{Direct, Proxy};
        for mode in [
            SourceProxyMode::Direct,
            SourceProxyMode::Proxy,
            SourceProxyMode::Auto,
        ] {
            for (code, preferred, alternate) in
                [("qbitai", Direct, Proxy), ("github", Proxy, Direct)]
            {
                let source = source(code, mode, "https://example.org/feed");
                assert_eq!(
                    diagnostic_route_plan(&source, true).unwrap(),
                    RoutePlan {
                        preferred,
                        fallback: Some(alternate)
                    }
                );
                assert_eq!(source.proxy_mode, mode);
            }
        }
        let explicit_proxy = source("github", SourceProxyMode::Proxy, "https://example.org/feed");
        assert_eq!(
            diagnostic_route_plan(&explicit_proxy, false).unwrap(),
            RoutePlan {
                preferred: Direct,
                fallback: None
            }
        );
        assert!(route_plan(&explicit_proxy, false).is_err());
    }

    #[test]
    fn diagnostics_keep_even_explicit_proxy_local_sources_direct() {
        for endpoint in [
            "http://localhost/feed",
            "http://127.0.0.1/feed",
            "http://192.168.1.1/feed",
            "http://[::1]/feed",
            "http://news.local/feed",
        ] {
            let source = source("github", SourceProxyMode::Proxy, endpoint);
            assert_eq!(
                diagnostic_route_plan(&source, true).unwrap(),
                RoutePlan {
                    preferred: SourceConnectionTestRoute::Direct,
                    fallback: None
                }
            );
            assert_eq!(
                route_plan(&source, true).unwrap().preferred,
                SourceConnectionTestRoute::Proxy
            );
        }
        assert!(diagnostic_route_plan(
            &source("custom", SourceProxyMode::Direct, "file:///tmp/feed"),
            true
        )
        .is_err());
    }
    #[test]
    fn local_automatic_sources_never_leave_the_direct_route() {
        for host in [
            "localhost",
            "LOCALHOST.",
            "news.localhost",
            "news.local",
            "intranet",
            "127.0.0.1",
            "127.255.255.254",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.2",
            "169.254.10.1",
            "[::1]",
            "[::ffff:127.0.0.1]",
            "[fc00::1]",
            "[fe80::1]",
        ] {
            let plan = route_plan(
                &source(
                    "github",
                    SourceProxyMode::Auto,
                    &format!("http://{host}/feed"),
                ),
                true,
            )
            .unwrap();
            assert_eq!(plan.preferred, SourceConnectionTestRoute::Direct, "{host}");
            assert!(plan.fallback.is_none(), "{host}");
        }
        for host in [
            "198.18.0.49",
            "172.15.255.254",
            "172.32.0.1",
            "news.local.example.com",
            "example.com",
        ] {
            assert!(
                route_plan(
                    &source(
                        "qbitai",
                        SourceProxyMode::Auto,
                        &format!("https://{host}/feed")
                    ),
                    true
                )
                .unwrap()
                .fallback
                .is_some(),
                "{host}"
            );
        }
    }
    #[test]
    fn rejects_non_http_malformed_and_credentialed_source_addresses() {
        for endpoint in [
            "broken",
            "file:///tmp/feed",
            "javascript:alert(1)",
            "https://user:pass@example.com/feed",
            "https://user@example.com/feed",
        ] {
            assert!(route_plan(&source("custom", SourceProxyMode::Auto, endpoint), true).is_err());
        }
    }
}
