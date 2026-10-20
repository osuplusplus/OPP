//! 游戏内覆盖层：透明置顶小窗 + tosu 游戏内渲染 + MP 自动补缺。
//!
//! - `service`：置顶窗口生命周期、热键与游戏状态钩子
//! - `irc`：osu! Bancho IRC 客户端与 MP 自动补缺编排（后端常驻）
//! - `ingame`：tosu ingame overlay 安装与本地状态 HTTP 服务（独占全屏方案）
//!
//! HUD 模式为点击穿透的状态卡片，面板模式为热键唤出的 Direct 侧栏；
//! 置顶窗口隐藏不销毁，MP 监听在后端与窗口生命周期无关。

pub(crate) mod commands;
pub(crate) mod ingame;
pub(crate) mod irc;
pub(crate) mod service;
