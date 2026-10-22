//! Public OPP mappool v1 documents. DNS is validated and pinned for each request.
use super::models::*;
use crate::{
    error::{CommandError, CommandResult},
    infrastructure::logging::{finish_span, global},
};
use futures_util::StreamExt;
use serde::Deserialize;
use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};
use url::Url;

fn invalid(message: &str) -> CommandError {
    CommandError::new("INVALID_OPP_POOL", message)
}

/// 判断 IPv4 是否为可公开访问地址：排除私网、回环、链路本地、广播、文档段、
/// 0.0.0.0/8、组播及以上、CGNAT（100.64/10）、192.0.0.0/24 与 198.18/15 基准测试段。
fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || a == 0
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19)))
}

/// 仅接受 2000::/3 全局单播地址；额外排除 6to4（2002::/16）、文档段
/// （2001:db8::/32、3fff::/20）以及 2001:0000-01ff 特殊用途段。
fn public_v6(ip: Ipv6Addr) -> bool {
    // Only global unicast; exclude documentation and transition/special-use ranges.
    let s = ip.segments();
    (s[0] & 0xe000) == 0x2000
        && s[0] != 0x2002
        && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
        && !(s[0] == 0x3fff && s[1] < 0x1000)
}

/// IP 是否公开可访问，供来源 URL 与 DNS 解析结果统一校验。
pub(super) fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

/// 校验并接受图池来源 URL（SSRF 防线）：长度不超过 1800、无凭据、无片段、
/// 仅 443 端口的 HTTPS 地址；IP 直连须为公开地址，域名须含点且排除
/// `localhost`/`.localhost`/`.local` 等本地域名。
pub(super) fn source_url(raw: &str) -> CommandResult<Url> {
    let url = Url::parse(raw).map_err(|_| invalid("图池来源必须是公开 HTTPS 地址"))?;
    if raw.len() > 1800
        || url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|p| p != 443)
    {
        return Err(invalid(
            "图池来源必须是无凭据、无片段的公开 HTTPS 地址（443 端口）",
        ));
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) if !public_v4(ip) => return Err(invalid("不能访问非公开地址")),
        Some(url::Host::Ipv6(ip)) if !public_v6(ip) => return Err(invalid("不能访问非公开地址")),
        Some(url::Host::Domain(host))
            if host == "localhost"
                || host.ends_with(".localhost")
                || host.ends_with(".local")
                || !host.contains('.') =>
        {
            return Err(invalid("不能访问本地地址"));
        }
        _ => {}
    }
    Ok(url)
}

/// OPP 标准 mappool v1 文档结构；`deny_unknown_fields` 使未知字段直接解析失败。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    title: String,
    #[serde(default)]
    ruleset: Option<String>,
    #[serde(default)]
    tournament: Option<String>,
    #[serde(default)]
    season: Option<String>,
    #[serde(default)]
    category: Option<String>,
    entries: Vec<Entry>,
}

/// 文档中的单个图位；除 `beatmap_id`/`selection_type`/`position` 外的字段均可省略。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    beatmap_id: i32,
    selection_type: String,
    position: u32,
    #[serde(default)]
    selected_by: Option<String>,
    #[serde(default)]
    selected_by_name: Option<String>,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    is_custom: bool,
    #[serde(default)]
    is_original: bool,
}

/// 解析并严格校验 OPP 标准 v1 文档：仅接受版本 1 与 osu!standard；标题与比赛信息
/// 有长度上限，条目不超过 500，`selection_type` 不得有首尾空白且 (类型, 序号) 不得重复。
pub(super) fn decode(bytes: &[u8], reference: &TournamentPoolRef) -> CommandResult<TournamentPool> {
    reference.validate()?;
    let doc: Document = serde_json::from_slice(bytes)
        .map_err(|e| CommandError::from_error("INVALID_OPP_POOL", e))?;
    if doc.version != 1 || doc.ruleset.as_deref().is_some_and(|r| r != "osu") {
        return Err(invalid("仅支持 OPP URI 标准 v1 和 osu!standard"));
    }
    if doc.title.trim().is_empty()
        || doc.title.chars().count() > 120
        || doc.entries.len() > 500
        || [&doc.tournament, &doc.season, &doc.category]
            .iter()
            .any(|s| s.as_ref().is_some_and(|s| s.chars().count() > 120))
    {
        return Err(invalid("图池标题、比赛信息或条目数量无效"));
    }
    let mut slots = HashSet::new();
    let mut entries = Vec::new();
    for entry in doc.entries {
        if entry.beatmap_id <= 0
            || entry.position == 0
            || entry.selection_type.trim().is_empty()
            || entry.selection_type.len() > 32
            || entry.selection_type.trim() != entry.selection_type
            || !slots.insert((entry.selection_type.clone(), entry.position))
            || entry
                .comment
                .as_ref()
                .is_some_and(|c| c.chars().count() > 30_000)
            || [&entry.selected_by, &entry.selected_by_name]
                .iter()
                .any(|s| s.as_ref().is_some_and(|s| s.chars().count() > 120))
        {
            return Err(invalid("图池包含非法谱面、重复图位或过长文本"));
        }
        entries.push(TournamentPoolEntry {
            beatmap_id: entry.beatmap_id,
            selection_type: entry.selection_type,
            position: entry.position,
            selected_by: entry.selected_by,
            selected_by_name: entry.selected_by_name,
            comment: entry.comment.unwrap_or_default(),
            is_custom: entry.is_custom,
            is_original: entry.is_original,
            beatmap: None,
            resolution_error: None,
        });
    }
    Ok(TournamentPool {
        reference: reference.clone(),
        title: doc.title.trim().into(),
        info: TournamentInfo {
            tournament: doc.tournament,
            season: doc.season,
            category: doc.category,
        },
        entries,
    })
}

/// 拉取标准图池文档：先自行解析 DNS 并要求全部结果为公开地址，再用
/// `resolve_to_addrs` 把连接钉死到这些地址（防 DNS rebinding / 解析结果被调包）；
/// 全程不走代理、不跟随重定向，流式读取上限 2 MiB。
pub(super) async fn fetch(reference: &TournamentPoolRef) -> CommandResult<TournamentPool> {
    let span = global().map(|log| log.operation("tournament_pools", "fetch_standard"));
    let result = async {
        reference.validate()?;
        let url = source_url(reference.url.as_deref().unwrap_or(""))?;
        let host = url.host_str().unwrap_or_default();
        let addresses: Vec<SocketAddr> = match url.host() {
            Some(url::Host::Ipv4(ip)) => vec![SocketAddr::new(ip.into(), 443)],
            Some(url::Host::Ipv6(ip)) => vec![SocketAddr::new(ip.into(), 443)],
            _ => tokio::time::timeout(
                Duration::from_secs(10),
                tokio::net::lookup_host((host, 443)),
            )
            .await
            .map_err(|_| invalid("来源域名解析超时"))??
            .collect(),
        };
        if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
            return Err(invalid("来源域名必须仅解析到公开地址"));
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .resolve_to_addrs(host, &addresses)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("OPP/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| CommandError::from_error("TOURNAMENT_NETWORK_ERROR", e))?;
        let response = client.get(url.clone()).send().await;
        if let Some(span) = &span {
            span.http_request(
                "GET",
                &format!("{}{}", url.origin().ascii_serialization(), url.path()),
                response.as_ref().ok().map(|r| r.status().as_u16()),
            );
        }
        let response = response
            .map_err(|e| CommandError::from_error("TOURNAMENT_NETWORK_ERROR", e.without_url()))?;
        if !response.status().is_success() {
            return Err(invalid("图池请求失败或发生重定向"));
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                CommandError::from_error("TOURNAMENT_NETWORK_ERROR", e.without_url())
            })?;
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(invalid("图池响应超过 2 MiB"));
            }
            bytes.extend_from_slice(&chunk);
        }
        decode(&bytes, reference)
    }
    .await;
    finish_span(span, result)
}
