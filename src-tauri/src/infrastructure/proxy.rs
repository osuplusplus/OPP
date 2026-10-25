//! 出站 HTTP 代理自动发现：环境变量 → Windows 系统代理 → 本机 7890 端口探测。
//!
//! 供社区服务与外部翻译 API 等需要"国内可达性不确定"的请求复用；
//! 探测全部失败时返回 `None`，调用方直连。

/// 代理发现顺序：环境变量 → Windows 系统代理设置 → 本机 7890 端口探测。
pub(crate) fn discover_proxy_url() -> Option<String> {
    ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
        .into_iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(windows_internet_proxy)
        .or_else(loopback_proxy)
}

/// 读取 Windows 注册表中的系统代理，优先 `https=`/`http=` 前缀条目。
#[cfg(windows)]
fn windows_internet_proxy() -> Option<String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let enabled = settings.get_value::<u32, _>("ProxyEnable").ok()? != 0;
    if !enabled {
        return None;
    }
    let value = settings.get_value::<String, _>("ProxyServer").ok()?;
    let candidate = value
        .split(';')
        .find_map(|entry| {
            entry
                .strip_prefix("https=")
                .or_else(|| entry.strip_prefix("http="))
        })
        .unwrap_or(value.as_str())
        .trim();
    (!candidate.is_empty()).then(|| normalize_http_proxy(candidate))
}

#[cfg(not(windows))]
fn windows_internet_proxy() -> Option<String> {
    None
}

fn loopback_proxy() -> Option<String> {
    // 7890 is the standard local mixed-proxy port used by the current desktop setup.
    // Only select it when a listener is actually present, so direct-network users are unaffected.
    let address =
        std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 7890);
    std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(100))
        .ok()
        .map(|_| "http://127.0.0.1:7890".to_string())
}

/// 为缺少协议前缀的代理地址补上 `http://`。
fn normalize_http_proxy(value: &str) -> String {
    if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    }
}
