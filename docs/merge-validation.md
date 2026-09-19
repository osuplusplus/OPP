# dev / main 与 Mania 数据集整合验收

检查日期：2026-09-19。整合起点：main `e037841`、dev `8a3318e`。

## 第一步：dev 与 main

分支：`codex/integrate-dev-main`。普通 merge 保留双方历史；main 的 #40 参考成绩 Mod 池修复与 dev 的新界面、后台任务调度同时保留。

- 新增最近游玩/BP 的 NM、DT、HT、混池快速/完整请求及缓存隔离回归。
- 修复 dev 的 OTD 面板 lint 问题，使用 TanStack Query 管理驱动状态读取。
- `pnpm lint`、`pnpm test -- --maxWorkers=4`（181 项）、`pnpm build`：通过。
- Windows `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`：通过。
- Windows `cargo test --manifest-path src-tauri/Cargo.toml --all-targets`：185 项通过、7 项原有忽略测试。
- 严格 clippy：未通过。对原始 dev `8a3318e` 也执行同一命令，确认 lib test 原有 486 项诊断；主要是公共错误类型过大、未使用代码和其他 lint。没有降低或屏蔽 lint 等级。

按维护者选择保留严格门槛：整合分支先完成，main 保持 `e037841`，修复原有 clippy 问题并完成平台验收后才能更新。没有推送、发布版本或改写历史。

## 剩余平台验收

Linux/WSL 已有 Rust 工具链，但缺少 WebKitGTK 4.1、ALSA 开发依赖，且当前会话无法无密码 sudo。因此尚未完成 Linux 整个 Tauri 应用构建及运行验证。Windows 和 Linux 的原生播放器、后台任务、实时预览冒烟检查也仍待完成；前端构建和单元测试不替代这些检查。

## 第二步安排

`codex/mania-pattern-integration` 在第一步整合结果之上适配 OPP #42（`d417090`）。因第一步门槛未满足，暂不依赖 main 已更新，也不合入 main。新算法还需要配套 osu-difficulty-lab #4 合入以及真实新数据集验收。
