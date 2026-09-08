<div align="center">
  <img src="./public/02.png" width="112" height="112" alt="OPP logo" />

  # OPP

  **一站式 osu! 工具集合**

  [![Version](https://img.shields.io/badge/version-0.4.5-ff6aa7?style=for-the-badge)](./src-tauri/tauri.conf.json)
  [![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-5ce1e6?style=for-the-badge&logo=linux&logoColor=white)](#平台支持)
  [![Tauri](https://img.shields.io/badge/Tauri-2-a673ff?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app/)
  [![Vibe Coding](https://img.shields.io/badge/Vibe_Coding-AI_Collaborative-8b5cf6?style=for-the-badge)](#vibe-coding)

  [功能](#功能概览) · [开始使用](#开始使用) · [开发](#本地开发) · [文档中心](./docs/README.md) · [版本记录](./docs/版本变更记录.md)
</div>

---

OPP 是一个使用 Tauri、Rust 与 React 构建的跨平台 osu! 桌面工具，支持 Windows x64 与 Linux。

> [!IMPORTANT]
> OPP 是独立的社区项目，与 ppy Pty Ltd 或 osu! 官方无隶属关系。osu! 是 ppy Pty Ltd 的商标。

**创了一个交流吹水群： 1059437719 有任何问题或者功能上的建议欢迎来群中吹水**

**B站视频演示：[我做了一个为osu玩家设计的工具箱](https://www.bilibili.com/video/BV1EhuC65EAB/)**

## 功能概览

- 通过官方 osu! API v2 OAuth 登录，查看玩家资料与成绩数据
- 提供谱面镜像批量下载、筛选队列与多镜像自动回退
- 管理 Stable 收藏夹、导入分享码，并自动补齐缺失谱面
- 根据本地索引查找相似谱面，并可基于最近成绩或 BP 生成推荐
- 内置 pp calculator，支持不同模式与 Mod
- 支持 Stable/lazer 本地谱面与 Skin 的浏览、预览和 `.osz` / `.osk` 导出
- 新增 Skin Workshop，可组合 Stable Skin 的组件与配置并安全保存为副本
- 支持启动 Stable 与 Lazer，并记录一次游戏会话的数据变化
- 支持 o!rdr 在线渲染与 Danser 本地回放渲染队列
- 支持 tosu、tosu-lyrics 与 OBS 直播工作流
- 支持 Trainer 练习谱面生成和网易云音乐客户端搜索
- 内置谱面图预览、osu!lazer 空间去重与占用统计、文件关联、手速测试等实用工具

当前算法口径为 [`Apeuriox/rosu-pp@pp-rework-202607`](https://github.com/Apeuriox/rosu-pp/tree/pp-rework-202607)，
使用该分支提供的最新 pp rework 算法快照。

## 开始使用

### Windows

从仓库的 [Releases](https://github.com/osuplusplus/OPP/releases/latest) 页面下载最新的 Windows x64 EXE。`OPP-vX.Y.Z-windows-x64.exe` 无需安装，可直接运行；首次启动需要系统已安装 WebView2 Runtime。

安装首个支持应用内更新的版本后，后续 Windows 便携版可在 OPP 内完成下载、原文件替换与自动重启，不需要改为安装包。

### Linux

Release 提供对应的 Linux 构建，可直接下载使用，也可以参考 [Linux 使用与构建](./docs/Linux.md) 。桌面环境需要 WebKitGTK 4.1 和可用的 Secret Service（例如 GNOME Keyring 或 KWallet）。

OPP 在 Linux 上通过 PATH 中的 `osu-wine` 与 `osu-lazer` 命令启动客户端。使用自定义安装方式时，请在“设置”中手动选择数据目录，并确保相应启动命令可用。

> [!TIP]
> 在 Wayland 下若出现无法启动、黑屏或渲染异常，可在启动前设置环境变量 `WEBKIT_DISABLE_DMABUF_RENDERER=1`

### 配置 OAuth

1. 打开 [osu! 账户设置](https://osu.ppy.sh/home/account/edit)。
2. 创建一个仅供个人使用的 OAuth 应用。
3. 将回调地址设置为：

   ```text
   http://127.0.0.1:42831/oauth/callback
   ```

4. 启动 OPP，填写 Client ID 与 Client Secret。
5. 在系统浏览器中完成 osu! 授权。

请勿把自己的 Client Secret、Token、凭据导出文件或应用数据提交到仓库。

### 配置相似谱面

1. 取得由兼容版本的 [`osu-difficulty-lab`](https://github.com/osuplusplus/osu-difficulty-lab) 生成的数据集；Mania 必须匹配提交 [`1fa21fa6`](https://github.com/osuplusplus/osu-difficulty-lab/commit/1fa21fa6a5144992df58efe7ce9d96019981fad3)，上游 `v0.3.0` tag 不含 Mania。
2. 将 standard 与 mania 数据集分别完整解压到本地目录。
3. 在 OPP 顶部选择 osu! 或 Mania，再到“相似谱面”页面选择该模式的索引根目录。应用会分别记住两套路径。

Standard 当前要求 Analyzer v4，根目录应包含 `metadata.sqlite`、`features-v*.bin`、`indexes/` 和 `normalizers/`；Mania 当前要求 Analyzer v1，根目录应包含 `mania-metadata.sqlite`、`mania-features-v1.bin`、`normalizers/mania-v1.bin` 和 `indexes/mania-v1.buckets(.sha256)`。Mania 的 DT/HT 混池还要求 `beatmaps/<BeatmapID>.osu`。如果解压后出现两层同名目录，请选择实际直接包含 metadata 文件的内层目录。更多说明见 [相似谱面数据集](./docs/similarity-dataset.md)。

## 本地开发

### 环境

- Node.js 22+
- pnpm 11+
- Rust stable toolchain
- Windows：MSVC toolchain、Windows SDK 与 WebView2
- Linux：WebKitGTK 4.1、OpenSSL、libappindicator、librsvg、libxdo 与 D-Bus 开发库；安装命令见 [Linux 使用与构建](./docs/Linux.md)

```text
pnpm install
pnpm tauri dev
```

### 质量检查

```text
pnpm lint
pnpm test
pnpm build

cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml

pnpm tauri build
```

构建产物默认位于 `src-tauri/target/release/`。Windows 打包后的 NSIS 文件位于：

```text
src-tauri/target/release/bundle/nsis/
```

Linux 当前默认生成 `src-tauri/target/release/opp`；发行打包策略见 [Linux 使用与构建](./docs/Linux.md)。

## 项目结构

```text
OPP/
├─ src/                         # React 前端
│  ├─ app/                     # 应用壳、导航与全局模式
│  ├─ features/                # 在线资料、成绩、本地谱面与 Skin
│  └─ shared/                  # 公共组件、类型和 Tauri API
├─ src-tauri/
│  ├─ crates/                  # 可独立复用的 Rust 运行时
│  └─ src/
│     ├─ account/              # OAuth、凭据和账号缓存
│     ├─ collections/          # 收藏夹同步、分享和缺失谱面补齐
│     ├─ danser/               # 本地回放渲染队列
│     ├─ local_analysis/       # 路径检测、扫描、缓存与资源分析
│     ├─ online_beatmaps/      # 在线谱面查询与下载
│     └─ similarity/           # 相似度数据集、查询和推荐
├─ docs/                       # 用户指南、数据集、架构与变更记录
└─ public/                     # 公共静态资源
```

## 贡献与已知限制

欢迎通过 [Issues](https://github.com/osuplusplus/OPP/issues) 提交问题、功能建议和可复现步骤。提交代码前请先阅读 [架构与开发](./docs/架构与开发.md)，并确保前后端质量检查全部通过。

- 相似谱面结果取决于本地 standard Analyzer v4 或 mania Analyzer v1 数据集的覆盖范围，不代表实时数据库。Mania 支持 4K/6K/7K 的 NM、DT、HT 单池或混池，难度 percentile 不是官方星数。
- o!rdr、谱面镜像、网易云音乐、tosu 与 OBS 等外部服务或应用的可用性不由 OPP 保证。

### 关联仓库

- [OPP](https://github.com/osuplusplus/OPP)
- [ppy/osu](https://github.com/ppy/osu)
- [tosuapp/tosu](https://github.com/tosuapp/tosu)
- [HollisMeynell/tosu-lyrics](https://github.com/HollisMeynell/tosu-lyrics)
- [Siflorite/mania-converter-rust](https://github.com/Siflorite/mania-converter-rust) (Apache-2.0)
- [Apeuriox/rosu-pp (`pp-rework-202607`)](https://github.com/Apeuriox/rosu-pp/tree/pp-rework-202607)
- [MaxOhn/rosu-map](https://github.com/MaxOhn/rosu-map)
- [Tauri](https://github.com/tauri-apps/tauri)
- [Ohdmire/osu-lazer-space-statistics](https://github.com/Ohdmire/osu-lazer-space-statistics)
- [Ohdmire/realm-db-reader](https://github.com/Ohdmire/realm-db-reader)
- [Ohdmire/osu-replay-render](https://github.com/Ohdmire/osu-replay-render)
- [LeoBlackMT/osumania_map_analyser](https://github.com/LeoBlackMT/osumania_map_analyser)

## 特别鸣谢

[**Rinne_0** ](https://osu.ppy.sh/users/11511458)和 [**Ribet**](https://osu.ppy.sh/users/19140906) 作为OPP的早期用户，深度参与了软件的测试，提出许多建设性意见，没有你们我可能在中间就放弃了。

图标原型：NekoArc 绘制者：**9**

感谢所有上游维护者、贡献者以及参与测试和反馈的 osu! 社区用户。
