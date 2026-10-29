# opp-bin（AUR）打包与发布说明

本目录存放 AUR 包 `opp-bin` 的全部"源码"。AUR 不是二进制仓库，上面只有三个文本文件，
本机的包组装发生在每个用户机器上（`yay`/`makepkg` 按 PKGBUILD 下载 deb、校验哈希后解包安装）：

| 文件 | 作用 | 由谁维护 |
| --- | --- | --- |
| `PKGBUILD` | 构建配方：deb 下载地址、sha256、依赖、解包方式 | 仓库内更新（CI 可代办） |
| `.SRCINFO` | PKGBUILD 的元数据摘要，AUR 靠它索引版本 | `makepkg --printsrcinfo` 生成，CI 自动再生 |
| `LICENSE` | 随包安装的 MIT 许可证（根 LICENSE 副本） | 基本不变 |

`opp-bin` 是官方 GitHub Release deb（`OPP_<版本>_linux_amd64.deb`）的重打包：deb 由
ubuntu-22.04 构建（glibc 2.35，对 Arch 向前兼容），内含 `usr/bin/opp`、desktop 文件与
hicolor 图标，desktop 已带 `Exec=opp`、`Icon=opp`、`MimeType=x-scheme-handler/opp`。

## 首次发布（交给有 AUR 账号的代发人执行）

把本目录的 `PKGBUILD`、`.SRCINFO`、`LICENSE` 三个文件交给代发人，其执行：

```bash
# 包尚不存在时直接 clone 会报 repository not found，属正常；改用：
mkdir opp-bin && cd opp-bin
git init -b master
git remote add origin ssh://aur@aur.archlinux.org/opp-bin.git
cp <收到的三个文件> .
git add PKGBUILD .SRCINFO LICENSE
git commit -m "Initial import: opp-bin 0.6.0-1"
git push origin master    # 首次 push 即在 AUR 创建 opp-bin，推送者成为维护者
```

推送后打开 <https://aur.archlinux.org/packages/opp-bin/> 确认页面生成、版本为 0.6.0-1。
同时请代发人把 PKGBUILD 首行 `# Maintainer:` 改为自己的 AUR 用户名（仅为惯例标注，
AUR 页面显示的维护者是推送账号本身）。

## 后续版本更新（两条路，可随时切换）

### 方式 A：CI 一键推送（推荐，热修复秒级跟进）

代发人在自己机器上生成一把**专用**密钥并把公钥添加到自己 AUR 账号的 SSH keys
（AUR 支持一个账号挂多把密钥）：

```bash
ssh-keygen -t ed25519 -f ~/aur-opp-push -C "opp-bin CI push"
# 把 ~/aur-opp-push.pub 内容粘贴到 https://aur.archlinux.org/account/代发用户名/edit 的 SSH key 列表
```

然后把私钥文件内容交给仓库管理员，存入 GitHub 仓库
`Settings → Secrets and variables → Actions → New repository secret`，
名称必须是 **`AUR_SSH_PRIVATE_KEY`**。

之后每次发版：Actions → **AUR** → Run workflow，填入 tag（如 `v0.6.1`）即可。工作流会：

1. 从该 tag 的 Release 下载 `OPP_<版本>_linux_amd64.deb` 并计算 sha256（草稿 Release 需勾选 `allow_draft`）；
2. 渲染 PKGBUILD（版本号、哈希、pkgrel 自动处理）；
3. 在 archlinux 容器里 `makepkg` 完整试打包 + `ldd` 依赖审计，失败即中止；
4. 幂等推送（内容无变化则跳过）。

> ⚠️ AUR 的 SSH 密钥是**账号级**权限：私钥一旦泄露，持有者可推送该账号下的所有包。
> 因此务必使用上面新生成的专用密钥，不要交出代发人日常使用的密钥；私钥只进 GitHub
> Secrets，不落仓库。若某天密钥需要作废，代发人在 AUR 账号设置删掉对应公钥即可。

### 方式 B：纯手动接力（不涉密钥交接）

发版后把更新过的 `PKGBUILD` + `.SRCINFO` 发给代发人（或由其在 AUR 仓库里自行修改），由他
push。生成更新文件两种办法任选：

- 在 GitHub Actions 页用 `dry_run=true` 跑一次 AUR 工作流，从日志里取渲染后的 PKGBUILD
  与 .SRCINFO（dry_run 不需要任何密钥）；
- 手动改：`pkgver`、两个 `sha256sums` 的第一个（deb 新哈希），以及 deb 文件名出现处
  （`source=`、`noextract=`，均为 `${pkgver}` 模板无需动）；同版本重推时 `pkgrel` 加一。

### 版本号规则

- `pkgver`：随 Release tag（去掉 `v` 前缀）。
- `pkgrel`：上游版本变了就重置为 `1`；**同一版本**需要重推（比如 deb 重传）时才 +1。
- 用户侧更新体验：`pacman -Syu` 即看到新版，与其他系统包一致。

## 推送后验证（任何一台 Arch 机器）

```bash
yay -Si opp-bin          # 或 paru -Si opp-bin，核对版本与依赖描述
yay -S opp-bin
opp                      # 启动主程序
xdg-open "opp://import?..."   # 深链（需安装后由 OPP 运行时注册，或直接验证桌面项存在）
pacman -Ql opp-bin | head    # 确认 /usr/bin/opp、desktop、图标、LICENSE 落位
```

## depends 的依据与调整

初版依赖清单来自 v0.6.0 二进制的 ELF 分析（`webkit2gtk-4.1`、`gtk3`、`alsa-lib`＝libasound、
`dbus`＝libdbus-1、`libx11`、`libayatana-appindicator`＝托盘 ayatana soname；x11rb 为纯 Rust
实现不占 libxcb）。之后如需增删，参考 AUR 工作流日志里的 **ldd 审计表**（依赖库 → 所属
pacman 包），它会随每次推送自动打印。

ffmpeg、danser、tosu、osu-lazer、osu-wine 为可选运行时（optdepends），不强制安装；
tosu 需要 `pkexec`（polkit，桌面系统通常已带）。

## 相关文档

- 用户侧安装说明：`docs/Linux.md` 的「安装」一节
- Release 产物与命名规则：`.github/workflows/Release.yml`
- 本工作流：`.github/workflows/AUR.yml`
