//! 面向用户的业务功能模块集合。

// OAuth 登录、凭据存储与账号资料/成绩查询。
pub(crate) mod account;
// 谱面包（pack）分享社区：发布、搜索、评论、评分与导入。
pub(crate) mod beatmaphub;
// 生涯统计：快照采集、日历与状态汇总。
pub(crate) mod career;
// 谱面集合管理：增删改、备份恢复、分享与缺失谱面补齐下载。
pub(crate) mod collections;
// 社区联机房间与赛事查询。
pub(crate) mod community;
// danser 本地回放渲染队列。
pub(crate) mod danser;
// 游戏会话：检测游戏启停，读取回放/截图等媒体。
pub(crate) mod game_session;
// 回放实时预览渲染（wgpu）。
pub(crate) mod live_render;
// 本地库路径探测、扫描索引与谱面/皮肤资源访问。
pub(crate) mod local_analysis;
// 本地库数据库的定位、初始化与迁移状态。
pub(crate) mod local_database;
// 本地成绩刷新。
pub(crate) mod local_scores;
// 内置音乐播放器运行时与窗口控制。
pub(crate) mod music_player;
// 打开网易云音乐搜索外部歌曲。
pub(crate) mod netease_music;
// OBS WebSocket 场景联动。
pub(crate) mod obs;
// 在线谱面搜索与多镜像下载。
pub(crate) mod online_beatmaps;
// Osekai 奖章数据查询。
pub(crate) mod osekai;
// 游戏内覆盖层（HUD + Direct 面板 + MP 补缺）。
pub(crate) mod overlay;
// 提交回放渲染任务。
pub(crate) mod replay_render;
// 相似谱面数据集加载、查询与推荐。
pub(crate) mod similarity;
// 玩家技能维度分析。
pub(crate) mod skill_analysis;
// 皮肤工坊：皮肤组件合并与原子写入。
pub(crate) mod skin_workshop;
// OpenTabletDriver（OTD）进程与配置管理。
pub(crate) mod tablet_driver;
// tosu 进程管理与 API 数据读取。
pub(crate) mod tosu;
// 赛事图池管理及 opp:// 深链接入。
pub(crate) mod tournament_pools;
// 训练谱面生成。
pub(crate) mod trainer;
// 便携版更新检查、下载与安装。
pub(crate) mod update;
// 视觉训练（生成与时间轴）。
pub(crate) mod view_trainer;
