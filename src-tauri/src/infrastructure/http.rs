//! 共享 HTTP 客户端工厂：统一 User-Agent 与超时样板，避免各模块散建 reqwest 客户端。
//! 代理、强制 HTTP/1.1、禁用重定向等特殊需求仍由调用方自行构建。

use std::time::Duration;

use crate::error::{CommandError, CommandResult};

pub(crate) struct HttpClientSpec {
    /// 附加在 `OPP/<版本>` 之后的 UA 说明，便于服务端识别流量来源。
    pub ua_note: &'static str,
    /// 整体请求超时（含响应体读取）。
    pub timeout: Option<Duration>,
    /// 建立连接（TCP+TLS）的超时。
    pub connect_timeout: Option<Duration>,
}

/// 按规格构建共享 `reqwest::Client`：UA 统一为 `OPP/<版本><ua_note>`，
/// 便于 osu! 与社区服务端识别流量来源；两个超时未设置时沿用 reqwest 默认值。
pub(crate) fn http_client(spec: HttpClientSpec) -> CommandResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder().user_agent(format!(
        "OPP/{}{}",
        env!("CARGO_PKG_VERSION"),
        spec.ua_note
    ));
    if let Some(timeout) = spec.timeout {
        builder = builder.timeout(timeout);
    }
    if let Some(timeout) = spec.connect_timeout {
        builder = builder.connect_timeout(timeout);
    }
    builder
        .build()
        .map_err(|error| CommandError::network(error.to_string()))
}
