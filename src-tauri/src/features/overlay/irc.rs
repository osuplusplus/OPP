//! osu! Bancho IRC 客户端与 MP 自动补缺编排。
//!
//! tosu 不读取多人大厅的谱面信息；房主换图时 BanchoBot 会在 `#mp_<房间ID>`
//! 频道广播 `Beatmap changed to: ... (https://osu.ppy.sh/b/<ID>)`（与
//! bancho.js 等裁判工具相同的通路），即使本地没有该谱面也能实时收到。
//! 本模块据此做缺失检测与自动下载，状态同时写入运行时（HTTP/事件）供
//! HUD、Direct 面板与 tosu 游戏内渲染页展示。

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncBufReadExt;

use crate::{
    domain::AppSettings,
    error::{CommandError, CommandResult},
    state::AppState,
};

/// osu! IRC 官方地址：明文 6667（bancho 官方现状，文档未提供 TLS）。
const IRC_HOST: &str = "irc.ppy.sh";
const IRC_PORT: u16 = 6667;
/// 重连退避区间：从 2 秒逐次翻倍增长，封顶 30 秒。
const RECONNECT_MIN: Duration = Duration::from_secs(2);
const RECONNECT_MAX: Duration = Duration::from_secs(30);
/// 定期 WHOIS 自己：bancho 在 319 回复中列出已加入的大厅频道，借此发现房间。
const WHOIS_INTERVAL: Duration = Duration::from_secs(30);
/// 同一谱面的去重窗口：短时间内 BanchoBot 可能因加入频道回放历史消息。
const DEDUPE_WINDOW: Duration = Duration::from_secs(120);
/// 下载单飞锁被主窗口占用时的重试节奏：每 10 秒一次，最多 30 次。
const BUSY_RETRY_DELAY: Duration = Duration::from_secs(10);
const BUSY_RETRY_LIMIT: u32 = 30;

/// IRC 连接状态（供 HUD 与设置页展示）。
#[derive(Debug, Clone, Serialize, Default)]
pub struct IrcStatusInfo {
    /// disconnected | connecting | connected | error
    pub state: String,
    pub nick: String,
    pub channels: Vec<String>,
    pub message: String,
}

/// MP 自动补缺的当前阶段与提示信息。
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct MpActivityInfo {
    /// idle | checking | missing | downloading | imported | present | unresolved | error
    pub phase: String,
    pub title: String,
    pub message: String,
}

/// 覆盖层共享运行时：IRC 连接状态、MP 补缺活动与最新下载进度。
/// HTTP 状态服务与 Tauri 事件都从这里读取。
pub struct OverlayRuntime {
    pub irc: Mutex<IrcStatusInfo>,
    pub mp: Mutex<MpActivityInfo>,
    pub download: Mutex<Option<crate::features::online_beatmaps::BeatmapDownloadProgress>>,
    /// 递增以中断进行中的 IRC 会话（停用/游戏退出时）。
    pub irc_generation: AtomicU64,
    /// 连接任务运行标志；以 `swap` 原子抢占防止重复启动。
    pub irc_running: AtomicBool,
    /// 去重窗口内已处理过的谱面 ID 与登记时间。
    recent_beatmaps: Mutex<Vec<(u64, Instant)>>,
}

impl Default for OverlayRuntime {
    fn default() -> Self {
        Self {
            irc: Mutex::new(IrcStatusInfo {
                state: "disconnected".into(),
                ..IrcStatusInfo::default()
            }),
            mp: Mutex::new(MpActivityInfo {
                phase: "idle".into(),
                ..MpActivityInfo::default()
            }),
            download: Mutex::new(None),
            irc_generation: AtomicU64::new(0),
            irc_running: AtomicBool::new(false),
            recent_beatmaps: Mutex::new(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// IRC 协议解析（纯函数，便于单测）
// ---------------------------------------------------------------------------

/// 从 IRC 消息流解析出的事件，仅保留补缺流程关心的种类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrcEvent {
    Welcome,
    Ping(String),
    /// WHOIS 319 返回里发现的大厅频道（含已加入的）。
    LobbyChannels(Vec<String>),
    /// 自己（任意连接）加入了 #mp_ 频道——bancho 会镜像游戏内的进房行为。
    JoinedChannel(String),
    /// BanchoBot 播报的换图事件，携带 beatmap id。
    BeatmapChanged(u64),
}

/// 提取文本中的 osu! 谱面链接 ID（兼容 /b/ 与 /beatmaps/ 两种路径）。
pub fn extract_beatmap_id(text: &str) -> Option<u64> {
    for marker in ["/b/", "/beatmaps/"] {
        if let Some(index) = text.find(marker) {
            let digits: String = text[index + marker.len()..]
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            if let Ok(id) = digits.parse::<u64>()
                && id > 0
            {
                return Some(id);
            }
        }
    }
    None
}

/// 识别 BanchoBot 的换图播报（仅接受已知前缀，避免把玩家聊天里的链接当事件）。
pub fn parse_beatmap_broadcast(text: &str) -> Option<u64> {
    const PREFIXES: [&str; 3] = ["Beatmap changed to:", "Changed beatmap to ", "Beatmap: "];
    let trimmed = text.trim();
    if PREFIXES.iter().any(|prefix| trimmed.starts_with(prefix)) {
        extract_beatmap_id(trimmed)
    } else {
        None
    }
}

/// 从 WHOIS/JOIN 载荷识别 `#mp_<纯数字>` 频道名，容忍 `@`/`+` 等频道前缀。
fn lobby_channel_name(token: &str) -> Option<String> {
    let token = token.trim_start_matches([':', '@', '+']);
    let rest = token.strip_prefix("#mp_")?;
    if rest.chars().all(|ch| ch.is_ascii_digit()) && !rest.is_empty() {
        Some(format!("#mp_{rest}"))
    } else {
        None
    }
}

/// 解析单条 IRC 服务器消息。own_nick 用于识别"自己加入频道"的镜像事件。
pub fn parse_irc_line(line: &str, own_nick: &str) -> Option<IrcEvent> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return None;
    }
    if let Some(token) = line.strip_prefix("PING ") {
        return Some(IrcEvent::Ping(
            token.trim().trim_start_matches(':').to_owned(),
        ));
    }
    let rest = line.strip_prefix(':')?;
    let (prefix, tail) = rest.split_once(' ')?;
    let sender_nick = prefix.split('!').next().unwrap_or(prefix);
    let parts: Vec<&str> = tail.split(' ').collect();

    // 数字回复：:server 001 <nick> :Welcome / :server 319 <nick> :#mp_1 #mp_2
    if parts
        .first()
        .is_some_and(|first| first.chars().all(|ch| ch.is_ascii_digit()))
    {
        return match parts[0] {
            "001" => Some(IrcEvent::Welcome),
            "319" => {
                let trailing = tail.split(" :").nth(1).unwrap_or_default();
                let channels = trailing
                    .split(' ')
                    .filter_map(lobby_channel_name)
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                Some(IrcEvent::LobbyChannels(channels))
            }
            _ => None,
        };
    }

    match parts.first().copied() {
        Some("JOIN") => {
            let channel = parts.get(1)?.trim_start_matches(':');
            if sender_nick.eq_ignore_ascii_case(own_nick) {
                lobby_channel_name(channel).map(IrcEvent::JoinedChannel)
            } else {
                None
            }
        }
        Some("PRIVMSG") => {
            let channel = parts.get(1)?;
            if !channel.starts_with("#mp_") {
                return None;
            }
            let trailing = tail.split(" :").nth(1)?;
            if sender_nick.eq_ignore_ascii_case("BanchoBot") {
                parse_beatmap_broadcast(trailing).map(IrcEvent::BeatmapChanged)
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 连接生命周期
// ---------------------------------------------------------------------------

/// 从设置与系统密钥环取 IRC 昵称与令牌（昵称中的空格替换为下划线）；
/// 任一缺失返回 `None`，调用方据此静默跳过连接。
fn irc_credentials(state: &AppState) -> Option<(String, String)> {
    let settings = state.store.settings_snapshot().ok()?;
    let nick = settings.overlay_irc_username?.trim().replace(' ', "_");
    let token = state.credentials.get_osu_irc_token().ok().flatten()?;
    if nick.is_empty() || token.is_empty() {
        return None;
    }
    Some((nick, token))
}

/// 原地更新 IRC 状态；锁中毒时静默忽略（状态展示非关键路径）。
fn set_irc(runtime: &OverlayRuntime, patch: impl FnOnce(&mut IrcStatusInfo)) {
    if let Ok(mut status) = runtime.irc.lock() {
        patch(&mut status);
    }
}

/// 更新 MP 活动状态并同步广播 `overlay-mp-activity` 事件。
pub fn set_mp(app: &AppHandle, phase: &str, title: &str, message: &str) {
    let info = MpActivityInfo {
        phase: phase.into(),
        title: title.into(),
        message: message.into(),
    };
    let runtime = app.state::<AppState>().overlay.clone();
    if let Ok(mut current) = runtime.mp.lock() {
        *current = info.clone();
    }
    let _ = app.emit("overlay-mp-activity", info);
}

/// 按需启动 IRC 连接任务（凭据缺失时静默跳过，由设置页引导补全）。
pub fn ensure_irc_connection(app: &AppHandle) {
    let state = app.state::<AppState>();
    let runtime = state.overlay.clone();
    let Some((nick, token)) = irc_credentials(&state) else {
        return;
    };
    // 原子抢占运行标志：返回 true 表示已有连接任务在跑，直接返回。
    if runtime.irc_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let generation = runtime.irc_generation.load(Ordering::SeqCst);
    set_irc(&runtime, |status| {
        status.nick = nick.clone();
        status.state = "connecting".into();
        status.channels.clear();
        status.message = String::new();
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let runtime = app.state::<AppState>().overlay.clone();
        let mut backoff = RECONNECT_MIN;
        loop {
            if runtime.irc_generation.load(Ordering::SeqCst) != generation {
                break;
            }
            match run_irc_session(&app, &runtime, &nick, &token, generation).await {
                Ok(()) => break, // 被停止
                Err(error) => {
                    set_irc(&runtime, |status| {
                        status.state = "error".into();
                        status.message = format!("{error}");
                    });
                }
            }
            // 可中断的退避等待。
            let mut waited = Duration::ZERO;
            while waited < backoff {
                if runtime.irc_generation.load(Ordering::SeqCst) != generation {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                waited += Duration::from_secs(1);
            }
            if runtime.irc_generation.load(Ordering::SeqCst) != generation {
                break;
            }
            set_irc(&runtime, |status| {
                status.state = "connecting".into();
            });
            backoff = (backoff * 2).min(RECONNECT_MAX);
        }
        set_irc(&runtime, |status| {
            status.state = "disconnected".into();
            status.channels.clear();
        });
        runtime.irc_running.store(false, Ordering::SeqCst);
    });
}

/// 请求停止：递增代际号让任务在下一个检查点自行退出，并复位展示状态。
pub fn stop_irc(app: &AppHandle) {
    let runtime = app.state::<AppState>().overlay.clone();
    runtime.irc_generation.fetch_add(1, Ordering::SeqCst);
    set_irc(&runtime, |status| {
        status.state = "disconnected".into();
        status.channels.clear();
        status.message = String::new();
    });
}

async fn send(writer: &mut tokio::net::tcp::OwnedWriteHalf, line: &str) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    writer.write_all(format!("{line}\r\n").as_bytes()).await
}

/// 单次 IRC 会话：TCP 连接、明文认证、逐行读取与事件分发、周期 WHOIS；
/// 连接失败或断开时返回错误，由外层循环按退避策略重连。
async fn run_irc_session(
    app: &AppHandle,
    runtime: &Arc<OverlayRuntime>,
    nick: &str,
    token: &str,
    generation: u64,
) -> CommandResult<()> {
    let stream = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::net::TcpStream::connect((IRC_HOST, IRC_PORT)),
    )
    .await
    .map_err(|_| CommandError::new("IRC_TIMEOUT", "连接 osu! IRC 超时"))?
    .map_err(|error| CommandError::new("IRC_CONNECT_FAILED", format!("{error}")))?;
    let _ = stream.set_nodelay(true);
    let (reader, mut writer) = stream.into_split();

    // osu!Bancho 使用明文 PASS 认证（官方 IRC 文档现状）。
    send(&mut writer, &format!("PASS {token}")).await?;
    send(&mut writer, &format!("NICK {nick}")).await?;
    send(&mut writer, &format!("USER {nick} 8 * :OPP")).await?;

    let mut reader = tokio::io::BufReader::new(reader);
    let mut joined: HashSet<String> = HashSet::new();
    let mut buffer = Vec::new();
    let mut next_whois = tokio::time::Instant::now();

    loop {
        if runtime.irc_generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        let whois_due = next_whois <= tokio::time::Instant::now();
        // 读循环要同时响应 WHOIS 周期：到期时构造立即完成的 future 与读竞争。
        let wait: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = if whois_due {
            Box::pin(std::future::ready(()))
        } else {
            let deadline = next_whois;
            Box::pin(async move {
                tokio::time::sleep_until(deadline).await;
            })
        };
        let read = reader.read_until(b'\n', &mut buffer);
        tokio::select! {
            result = read => {
                let bytes = match result {
                    Ok(0) => {
                        return Err(CommandError::new("IRC_DISCONNECTED", "osu! IRC 连接被关闭"));
                    }
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return Err(CommandError::new("IRC_READ_FAILED", format!("{error}")));
                    }
                };
                let line = String::from_utf8_lossy(&buffer[..bytes]).into_owned();
                buffer.truncate(0);
                match parse_irc_line(&line, nick) {
                    Some(IrcEvent::Ping(token)) => {
                        send(&mut writer, &format!("PONG :{token}")).await?;
                    }
                    Some(IrcEvent::Welcome) => {
                        set_irc(runtime, |status| {
                            status.state = "connected".into();
                            status.message = String::new();
                        });
                        send(&mut writer, &format!("WHOIS {nick}")).await?;
                        next_whois = tokio::time::Instant::now() + WHOIS_INTERVAL;
                    }
                    Some(IrcEvent::LobbyChannels(channels)) => {
                        for channel in channels {
                            if joined.insert(channel.clone()) {
                                let _ = send(&mut writer, &format!("JOIN {channel}")).await;
                            }
                        }
                    }
                    Some(IrcEvent::JoinedChannel(channel)) => {
                        if joined.insert(channel.clone()) {
                            set_irc(runtime, |status| {
                                status.channels = joined.iter().cloned().collect();
                            });
                        }
                    }
                    Some(IrcEvent::BeatmapChanged(beatmap_id)) => {
                        // 补缺含 API 查询与下载，派发独立任务避免阻塞读循环。
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            handle_beatmap(app, beatmap_id).await;
                        });
                    }
                    None => {}
                }
            }
            _ = wait => {
                if whois_due {
                    let _ = send(&mut writer, &format!("WHOIS {nick}")).await;
                    next_whois = tokio::time::Instant::now() + WHOIS_INTERVAL;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MP 补缺编排
// ---------------------------------------------------------------------------

/// 去重登记：清理过期项后，若窗口内已处理过该谱面则返回 false。
fn register_beatmap(runtime: &OverlayRuntime, beatmap_id: u64) -> bool {
    if let Ok(mut recent) = runtime.recent_beatmaps.lock() {
        let now = Instant::now();
        recent.retain(|(_, at)| now.duration_since(*at) < DEDUPE_WINDOW);
        if recent.iter().any(|(id, _)| *id == beatmap_id) {
            return false;
        }
        recent.push((beatmap_id, now));
    }
    true
}

/// 补缺所需的最小谱面身份：谱面集 ID 与展示用标题/作者。
struct BeatmapIdentity {
    beatmapset_id: u64,
    title: String,
    artist: String,
}

/// 经 osu! API 把 `beatmap_id` 解析为谱面集 ID 与标题；标题/作者优先取随
/// 响应附带的 `beatmapset` 嵌套字段，缺失时回退顶层字段。
async fn resolve_beatmap(state: &AppState, beatmap_id: u64) -> CommandResult<BeatmapIdentity> {
    let access_token = crate::features::account::ensure_access_token(state).await?;
    let value = state
        .api
        .get_beatmap(&access_token, beatmap_id)
        .await
        .map_err(|_| CommandError::new("MP_BEATMAP_LOOKUP_FAILED", "查询谱面信息失败"))?;
    let set_id = value
        .get("beatmapset_id")
        .and_then(serde_json::Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or_else(|| CommandError::new("MP_BEATMAP_UNRESOLVED", "无法确定谱面集 ID"))?;
    let set = value.get("beatmapset");
    let pick = |key: &str| -> String {
        set.and_then(|set| set.get(key))
            .or_else(|| value.get(key))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Ok(BeatmapIdentity {
        beatmapset_id: set_id,
        title: pick("title"),
        artist: pick("artist"),
    })
}

/// MP 补缺主流程：去重 → API 解析 → 本地 presence 检查；缺失且开启自动
/// 下载时投递下载请求，主窗口占用下载锁则按固定间隔重试。
async fn handle_beatmap(app: AppHandle, beatmap_id: u64) {
    let state = app.state::<AppState>();
    let runtime = state.overlay.clone();
    if !register_beatmap(&runtime, beatmap_id) {
        return;
    }
    let settings = state.store.settings_snapshot().unwrap_or_default();
    let identity = match resolve_beatmap(&state, beatmap_id).await {
        Ok(identity) => identity,
        Err(error) => {
            let unresolved = error.code == "MP_BEATMAP_UNRESOLVED";
            set_mp(
                &app,
                if unresolved { "unresolved" } else { "error" },
                "",
                if unresolved {
                    "无法识别该谱面（可能未提交）".into()
                } else {
                    format!("MP 补缺：{}", error.message)
                }
                .as_str(),
            );
            return;
        }
    };
    let title = if identity.artist.is_empty() {
        identity.title.clone()
    } else {
        format!("{} - {}", identity.artist, identity.title)
    };
    set_mp(&app, "checking", &title, "");

    // presence 查询是阻塞索引读取，放到阻塞线程。
    let local = state.local_analysis.clone();
    let id = beatmap_id as i32;
    let presence =
        tauri::async_runtime::spawn_blocking(move || local.beatmap_presence(vec![id], None))
            .await
            .ok()
            .and_then(|result| result.ok())
            .unwrap_or_default();
    if presence.first().is_some_and(|entry| {
        entry.status == crate::features::local_analysis::LocalPresenceState::Present
    }) {
        set_mp(&app, "present", &title, "");
        return;
    }
    if !settings.overlay_mp_auto_download {
        set_mp(&app, "missing", &title, "");
        return;
    }
    let destination = settings
        .beatmap_download_directory
        .clone()
        .unwrap_or_default();
    if destination.trim().is_empty() {
        set_mp(&app, "error", &title, "尚未配置谱面下载目录，无法自动补缺");
        return;
    }
    set_mp(&app, "downloading", &title, "");
    let item = crate::features::online_beatmaps::BeatmapDownloadItem {
        beatmapset_id: identity.beatmapset_id,
        artist: identity.artist,
        title: identity.title,
        expected_beatmap_ids: Vec::new(),
        allow_extra_difficulties: true,
    };
    // 单飞锁被主窗口下载占用时按间隔重试，其余错误立即报错终止。
    for attempt in 0..=BUSY_RETRY_LIMIT {
        let request = crate::features::online_beatmaps::BeatmapDownloadRequest {
            destination: destination.clone(),
            items: vec![item.clone()],
            provider: default_provider(&settings),
            overwrite: false,
            include_video: settings.include_video_in_beatmap_downloads,
            open_after_download: Some(true),
        };
        match crate::features::online_beatmaps::download_online_beatmapsets(app.clone(), request)
            .await
        {
            Ok(result) => {
                if result.failed > 0
                    && result
                        .failures
                        .iter()
                        .any(|failure| failure.beatmapset_id == identity.beatmapset_id)
                {
                    let message = result
                        .failures
                        .iter()
                        .find(|failure| failure.beatmapset_id == identity.beatmapset_id)
                        .map(|failure| failure.message.clone())
                        .unwrap_or_default();
                    set_mp(&app, "error", &title, &message);
                } else {
                    set_mp(&app, "imported", &title, "");
                }
                return;
            }
            Err(error) if error.code == "DOWNLOAD_ALREADY_RUNNING" => {
                // 主窗口手动下载占用单飞锁：稍后重试同一张。
                if attempt == BUSY_RETRY_LIMIT {
                    set_mp(&app, "error", &title, "下载队列忙，已放弃本次自动补缺");
                    return;
                }
                tokio::time::sleep(BUSY_RETRY_DELAY).await;
            }
            Err(error) => {
                set_mp(&app, "error", &title, &error.message);
                return;
            }
        }
    }
}

/// 取设置中的默认下载源；序列化失败或未设置时回退 `sayobot`。
fn default_provider(settings: &AppSettings) -> String {
    serde_json::to_value(settings.default_beatmap_download_provider)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "sayobot".into())
}

// IRC 协议解析与谱面链接提取的纯函数单测。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_beatmap_ids_from_links() {
        assert_eq!(
            extract_beatmap_id("https://osu.ppy.sh/b/123456 extra"),
            Some(123456)
        );
        assert_eq!(
            extract_beatmap_id("https://osu.ppy.sh/beatmaps/42"),
            Some(42)
        );
        assert_eq!(extract_beatmap_id("no link"), None);
        assert_eq!(extract_beatmap_id("https://osu.ppy.sh/b/0"), None);
    }

    #[test]
    fn parses_banchobot_broadcasts_only() {
        assert_eq!(
            parse_beatmap_broadcast("Beatmap changed to: A - B (https://osu.ppy.sh/b/99)"),
            Some(99)
        );
        assert_eq!(
            parse_beatmap_broadcast("Changed beatmap to https://osu.ppy.sh/b/7 A - B"),
            Some(7)
        );
        assert_eq!(
            parse_beatmap_broadcast("Beatmap: https://osu.ppy.sh/b/5 A - B"),
            Some(5)
        );
        // 玩家聊天里的普通链接不构成事件。
        assert_eq!(
            parse_beatmap_broadcast("看这个 https://osu.ppy.sh/b/99"),
            None
        );
    }

    #[test]
    fn parses_whois_channel_reply() {
        let line = ":irc.ppy.sh 319 shimi :#mp_1234567 #osu #mp_99";
        match parse_irc_line(line, "shimi") {
            Some(IrcEvent::LobbyChannels(channels)) => {
                assert!(channels.contains(&"#mp_1234567".to_string()));
                assert!(channels.contains(&"#mp_99".to_string()));
                assert_eq!(channels.len(), 2);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_self_join_mirror() {
        let line = ":shimi!shimi@ppy.sh JOIN :#mp_555";
        assert_eq!(
            parse_irc_line(line, "shimi"),
            Some(IrcEvent::JoinedChannel("#mp_555".into()))
        );
        // 他人加入不触发。
        assert_eq!(parse_irc_line(":other!u@h JOIN :#mp_555", "shimi"), None);
    }

    #[test]
    fn parses_banchobot_privmsg() {
        let line = ":BanchoBot!BanchoBot@cho.ppy.sh PRIVMSG #mp_1 :Beatmap changed to: X (https://osu.ppy.sh/b/321)";
        assert_eq!(
            parse_irc_line(line, "shimi"),
            Some(IrcEvent::BeatmapChanged(321))
        );
        // 普通玩家消息即使含链接也不触发。
        let chat = ":player!u@h PRIVMSG #mp_1 :Beatmap changed to: X (https://osu.ppy.sh/b/1)";
        assert_eq!(parse_irc_line(chat, "shimi"), None);
    }

    #[test]
    fn responds_to_ping_and_welcome() {
        assert_eq!(
            parse_irc_line("PING :irc.ppy.sh", "shimi"),
            Some(IrcEvent::Ping("irc.ppy.sh".into()))
        );
        assert_eq!(
            parse_irc_line(":irc.ppy.sh 001 shimi :Welcome", "shimi"),
            Some(IrcEvent::Welcome)
        );
    }
}
