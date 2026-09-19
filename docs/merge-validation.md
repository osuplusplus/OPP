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

## 第二步：新界面适配 #42

`codex/mania-pattern-integration` 在第一步整合结果之上适配 OPP #42（`d417090`）。配套 osu-difficulty-lab #4 已合入官方 main；运行库依赖已切换到官方仓库并固定为 v4 manifest 指定的 `92b791c`。

已完成：

- 保留 dev 的 Mania 页面、逐首舞台和筛选；两个旧卡片/比较组件保持删除。六维比较和键型信息迁入 `SimilarityWorkspace`。
- 两侧键型齐全时展示六维覆盖率；缺一侧时仍用旧难度比较，已有的主模式、RC/LN 比例及细分键型单独展示。
- 没有在线链接的记录保留浏览和历史，禁用在线试听、下载、预览、收藏和历史跳转；封面及相邻预取也不会请求此类记录的合成 ID。
- 没有采用 PR 的“只检查目录即返回 ready”逻辑，保留真实后台索引校验；没有采用跳过无键型参考成绩的逻辑，旧数据继续走 v1 推荐。
- 保留 #40 的 Mod 池筛选和 dev 的后台任务调度，新增可空 `pattern_view`，命令名和请求格式不变。

验证结果：

| 检查 | 结果 |
| --- | --- |
| 前端 lint / build | 通过 |
| 前端测试 | 62 个文件、194 项通过 |
| 应用及独立运行库 rustfmt check | 通过 |
| Windows 应用 Rust 测试 | 185 项通过、8 项忽略；新增的旧数据集测试另行执行通过 |
| Windows 独立运行库测试 | 46 项通过；v4 无源文件包集成测试通过 |
| Linux/WSL 独立运行库测试 | 46 项通过；v4 无源文件包集成测试通过 |
| 真实旧 Mania 数据集推荐 | 通过；最近游玩/BP × NM/DT/HT/混池，分别检查 4K/6K/7K 结果，确认无键型记录也保留参考谱面 |
| 真实 v4 Mania 数据集推荐 | 通过；校验全部 manifest 哈希，并在无 `beatmaps/` 的发布包上覆盖最近游玩/BP × NM/DT/HT/混池 × 4K/6K/7K，响应包含 MMA 键型视图 |
| 浏览器新界面冒烟 | Edge 无头模式在桌面及较窄视口显示六维舞台，无页面脚本错误；使用预览数据，不代表原生 Tauri 验收 |
| Windows 严格 clippy | 未通过；原始 dev 与整合版均为 486 项，按诊断文本分组的数量完全一致 |

## 合入前剩余条件

1. 修复第一步原有 clippy 诊断并完成上面的原生平台验收，再将 `codex/integrate-dev-main` 合入 main。
2. 使用带原始 `.osu` 文件的 lab 验证目录配置 `MMA_TEST_DATASET`，执行生成端与运行端逐记录一致性测试。正式 v4 发布包刻意不带源文件，因此只能用于已经通过的 `MMA_PACKAGED_DATASET` 测试。
3. 显式运行数据集测试：

```powershell
cargo test --manifest-path src-tauri/crates/osu-difficulty-runtime/Cargo.toml --test mania_pattern_dataset -- --ignored
```

4. 生成端逐记录测试、完整质量检查及平台验收通过后，才将 dev 合入 main。

## dev 更新状态

v4 发布包：`C:\Users\Shimi\Desktop\osu-mania-dataset-v4`。manifest 声明 12,450 张谱面、37,350 条 MMA 记录、824 张本地身份谱面；7 个运行文件的 SHA-256 已全部核对通过。`dev` 更新后直接包含 main #40、dev 新界面和 #42/v4 运行库支持，main 仍保持不变。

旧数据集命令级回归可通过设置 `LEGACY_MANIA_DATASET`（包含原生 4K/6K/7K 的 NM/DT/HT 数据）后复现：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml legacy_dataset_still_recommends_without_pattern_records -- --ignored
```

数据集只读且不纳入提交；数据库探测使用 SQLite immutable URI，避免只读连接生成 WAL/SHM 文件。
