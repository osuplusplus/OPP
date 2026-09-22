# OPP Linux 使用与构建

OPP 支持 Linux 桌面环境。在线功能、本地资源、Stable/lazer 游戏会话、收藏夹、o!rdr、Danser 本地渲染、tosu/OBS 和常用工具均可直接使用；默认打开端设置和显示器伽马当前仅支持 Windows。

## 运行前准备

Linux 桌面需要：

- WebKitGTK 4.1，用于显示 Tauri 界面；
- D-Bus 与 Secret Service 实现，例如 GNOME Keyring 或 KWallet，用于保存 OAuth Token、Client Secret 和 OBS 密码；
- `osu-wine`（Stable）或 `osu-lazer`（lazer）启动命令；
- 使用 tosu 时，PATH 中还需要 `tosu` 和 `pkexec`（PolicyKit）；
- 使用 Danser 本地渲染时，PATH 中需要 `danser` 和 `ffmpeg`；Trainer 变速或截取同样依赖 `ffmpeg`；
- 本地谱库音乐和谱面试听使用 Rust 音频引擎，通过 ALSA 输出并通过 D-Bus MPRIS 接入系统媒体控制；皮肤音效等网页音频仍需要 WebKitGTK 的 GStreamer 音频插件。

在 Debian/Ubuntu 上构建时，可先安装 Tauri 2 和凭据后端所需依赖：

```bash
sudo apt update
sudo apt install \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libdbus-1-dev \
  libasound2-dev \
  gstreamer1.0-plugins-good \
  pkg-config
```

其他发行版请按 [Tauri 2 官方前置依赖说明](https://v2.tauri.app/start/prerequisites/) 安装对应软件包。

## 从源码运行

除系统依赖外，还需要 Node.js 22+、pnpm 11+ 和 Rust stable：

```bash
pnpm install
pnpm tauri dev
```

质量检查与通用发布步骤见[架构与开发](./架构与开发.md#质量门槛)。生成本地主程序：

```bash
pnpm tauri build
```

仓库默认关闭 Tauri bundle，主程序位于 `src-tauri/target/release/opp`。Release 工作流显式选择 AppImage 和 deb；本地生成相同格式可运行：

```bash
pnpm tauri build --bundles appimage,deb
```

打包后仍须在目标发行版验证运行依赖和桌面集成。图池 URI 注册依赖 `xdg-mime` 与 `update-desktop-database`；便携程序移动后需从新位置启动一次。

## tosu 与 OBS

Linux 上的 tosu 需要读取 Wine 进程内存。OPP 会通过 `pkexec` 显示 PolicyKit 授权窗口，并由看门狗进程管理 tosu；停止时优先使用运行时目录中的停止标志，必要时再次请求授权。

如果 tosu 无法启动，请检查：

```bash
command -v tosu
command -v pkexec
```

OBS WebSocket 默认地址仍为 `ws://127.0.0.1:4455`。桌面会话必须提供可解锁的 Secret Service，否则 OPP 无法安全保存密码和 OAuth 凭据。

## Danser 本地渲染

Linux 使用 danser-go：在 PATH 中查找 `danser` ，ffmpeg 可来自 Danser 发行包或系统 PATH。danser-go 的 settings 保存在 `~/.config/danser`（而非可执行文件旁边）。

渲染时 OPP 会基于你当前选择的 settings profile 生成专用的 `opp.json`，把录制输出目录指向导出目录，不会改动你自己的配置文件。可用以下命令检查依赖是否就绪：

```bash
command -v danser
command -v ffmpeg
```

## Wayland 渲染问题

在 Wayland 下若出现窗口无法启动、黑屏或渲染异常，通常与 WebKitGTK 的 DMA-BUF 渲染器有关，可在启动前关闭它：

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./opp
```

## 已知平台限制

- `.osz` / `.osk` 默认打开端依赖 Windows 注册表，Linux 不显示该设置。
- 显示器伽马依赖 Windows GDI，Linux 不显示该工具。
- 实时回放预览使用 X11 原生窗口；Wayland 会话需有可用的 XWayland，须单独验证缩放和输入行为。
- Stable 在 Linux 上通常运行于 Wine；收藏夹写回前仍需关闭游戏，避免覆盖游戏正在写入的数据。
