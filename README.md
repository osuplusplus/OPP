<div align="center">
  <img src="./public/02.png" width="112" height="112" alt="OPP logo" />

  <h1>OPP</h1>
  <p><strong>一站式 osu! 工具集合</strong></p>

  [![Version](https://img.shields.io/badge/version-0.5.0-ff6aa7?style=for-the-badge)](./docs/版本变更记录.md#v050)
  [![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-5ce1e6?style=for-the-badge&logo=linux&logoColor=white)](#开始使用)
  [![Tauri](https://img.shields.io/badge/Tauri-2-a673ff?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app/)

  [下载](https://github.com/osuplusplus/OPP/releases/latest) · [功能](#功能概览) · [开始使用](#开始使用) · [文档中心](./docs/README.md) · [版本记录](./docs/版本变更记录.md)
</div>

OPP 使用 Tauri 2、React 19 与 Rust 构建，支持 Windows x64 与 Linux。

> [!IMPORTANT]
> OPP 是独立的社区项目，与 ppy Pty Ltd 或 osu! 官方无隶属关系。osu! 是 ppy Pty Ltd 的商标。

交流群：**1059437719** · [B 站视频演示](https://www.bilibili.com/video/BV1EhuC65EAB/)

## 功能概览

| 功能 | 说明 |
| --- | --- |
| 在线谱面 | 搜索、筛选、舞台预览、批量下载、多镜像回退与失败重试 |
| 收藏与社区 | Stable／lazer 收藏读取、Stable 写回、分享码、BeatmapHub 曲包发现与发布 |
| 比赛与练习 | OPP URI 图池导入、个人标签／笔记／成绩记录、Trainer 谱面生成与预览 |
| 相似谱面与分析 | Standard／Mania 本地数据集检索、最近成绩／BP 推荐、技能分析与 PP 计算 |
| 本地资源 | Stable／lazer 谱面与皮肤浏览、增量索引、导出、Skin Workshop 与空间管理 |
| 音乐与回放 | 本地谱库播放器、悬浮迷你播放器、实时回放预览、o!rdr 与 Danser 渲染 |
| 账号与直播 | osu! OAuth、玩家资料与成绩、游戏会话记录、tosu／歌词／OBS 联动 |

v0.5.0 的完整变化见[版本记录](./docs/版本变更记录.md#v050)。

## 开始使用

### 下载与运行

从 [Releases](https://github.com/osuplusplus/OPP/releases/latest) 选择对应平台的资源：

- **Windows x64**：`OPP_<版本>_windows_x64_portable.exe` 为免安装便携版，`*_setup.exe` 为安装包。运行需要 WebView2 Runtime；便携版支持应用内下载更新、原路径替换和自动重启。
- **Linux**：提供 AppImage 与 deb。运行依赖、客户端启动命令和 Wayland 排错见 [Linux 使用与构建](./docs/Linux.md)。

### 首次配置

1. 在 [osu! 账户设置](https://osu.ppy.sh/home/account/edit) 创建个人 OAuth 应用，将回调地址设为 `http://127.0.0.1:42831/oauth/callback`。
2. 在 OPP 中填写 Client ID 与 Client Secret，在系统浏览器完成授权。
3. 在“设置 → 常规”选择游戏模式与默认客户端，核对 Stable／lazer 数据目录，再扫描本地资源。
4. 使用相似谱面前，按[数据集说明](./docs/similarity-dataset.md)下载并解压匹配的数据，在相似谱面页分别配置 Standard 与 Mania 根目录。数据集不随应用分发。

请勿将 Client Secret、Token、凭据导出文件或应用数据提交到仓库。

### 使用边界

- lazer 收藏数据库保持只读；URI 图池初次导入仅保存在 OPP，写入 Stable 需显式选择“导入 Stable”。详见[收藏夹与个人记录](./docs/收藏夹浏览与个人记录.md)和 [OPP URI 标准](./docs/OPP-URI标准.md)。
- `.osz`／`.osk` 默认打开端设置与显示器伽马工具仅支持 Windows；其他平台差异见 [Linux 文档](./docs/Linux.md)。
- 相似结果受本地数据集覆盖范围限制；镜像下载、在线渲染和直播联动依赖对应外部服务或应用。

## 本地开发与贡献

准备 Node.js 22+、pnpm 11+、Rust stable 及对应平台依赖后运行：

```bash
pnpm install
pnpm tauri dev
```

环境依赖、目录结构、质量检查与发布流程统一维护在[架构与开发](./docs/架构与开发.md)。欢迎向 **dev** 分支提交 PR，或通过 [Issues](https://github.com/osuplusplus/OPP/issues) 提供问题、建议与复现步骤。

## 关联项目与鸣谢

- 框架与游戏：[Tauri](https://github.com/tauri-apps/tauri)、[ppy/osu](https://github.com/ppy/osu)
- 分析与转换：[osu-difficulty-lab](https://github.com/osuplusplus/osu-difficulty-lab)、[rosu-pp（pp-rework-202607）](https://github.com/Apeuriox/rosu-pp/tree/pp-rework-202607)、[rosu-map](https://github.com/MaxOhn/rosu-map)、[mania-converter-rust](https://github.com/Siflorite/mania-converter-rust)、[osumania_map_analyser](https://github.com/LeoBlackMT/osumania_map_analyser)
- 本地资源与回放：[osu-lazer-space-statistics](https://github.com/Ohdmire/osu-lazer-space-statistics)、[realm-db-reader](https://github.com/Ohdmire/realm-db-reader)、[osu-replay-render](https://github.com/Ohdmire/osu-replay-render)
- 直播：[tosu](https://github.com/tosuapp/tosu)、[tosu-lyrics](https://github.com/HollisMeynell/tosu-lyrics)

感谢早期用户 [Rinne_0](https://osu.ppy.sh/users/11511458) 和 [Ribet](https://osu.ppy.sh/users/19140906) 持续参与测试与反馈，以及所有上游维护者、贡献者和社区用户。图标原型为 NekoArc，由 **9** 绘制。

OPP 使用 [MIT License](./LICENSE)；第三方声明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
