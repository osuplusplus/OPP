# 相似谱面数据集

## 获取与使用

OPP 不会在应用 EXE 中内置谱面索引。请取得由兼容版本的 [`osu-difficulty-lab`](https://github.com/osuplusplus/osu-difficulty-lab) 生成的数据集并完整解压，再在“相似谱面”页面为当前游戏模式选择对应的根目录。

osu!standard 与 osu!mania 使用两套互相独立的目录和算法，不能混用或通过重命名文件互相转换。OPP 始终以只读方式访问所选目录，不会修改或上传索引内容。

## osu!standard Analyzer v4

当前 standard 运行时要求 Analyzer v4（`five-dimension-slider-rosu-reading-v4`）。数据集根目录应直接包含：

```text
metadata.sqlite
features-v*.bin
indexes/difficulty-main.hnsw
normalizers/v*.bin
```

Analyzer v4 的五个维度为 Aim、Speed、Reading、Slider 和 Overlap。Reading 来自锁定的 `Apeuriox/rosu-pp` rework 快照；Analyzer v2/v3 的特征、归一化文件和 HNSW 索引不能直接迁移，必须使用匹配 v4 的数据集。

## osu!mania v4 数据集

当前 Mania 运行时与 [`osu-difficulty-lab@92b791c`](https://github.com/osuplusplus/osu-difficulty-lab/commit/92b791c95ea4a6c552e89d5b711551f0f2fb83a1) 生成的 v4 数据集配套，底层继续使用 `mania-roxy-interlude-similarity-v1` 特征，并加入 `mania-mma-pattern-v2` 键型记录。支持原生 4K、6K 和 7K，以及 NM、DT、HT 单池或混池。数据集根目录必须直接包含：

```text
mania-metadata.sqlite
mania-features-v1.bin
mania-mod-features-v1.bin
mania-mma-features.bin
normalizers/mania-v1.bin
indexes/mania-v1.buckets
indexes/mania-v1.buckets.sha256
dataset-manifest.json
```

`mania-mod-features-v1.bin` 保存 DT/HT 候选特征，`mania-mma-features.bin` 保存三种 Mod 的键型记录。OPP 只读取这些预计算文件，不要求包内包含 `beatmaps/`，也不会在查询时重算 `.osu` 源文件。请保持所有文件来自同一份 manifest；不要混用旧版 SQLite、特征、归一化或索引文件。

如果已有 `mania-ranked-rebuilt-20260821` 和独立的 `mania-beatmaps` 源谱面目录，可在 OPP 工作区执行：

```powershell
cargo run --manifest-path src-tauri/crates/osu-difficulty-runtime/Cargo.toml --example build_mod_features -- `
  C:\path\to\mania-ranked-rebuilt-20260821 `
  C:\path\to\mania-beatmaps
```

该命令只写入目标目录中的 `mania-mod-features-v1.bin`，不会修改 SQLite、NoMod 特征或源谱面。

Mania 会先按键数隔离，再按 Analyzer 输出的 family 与 dominant pattern 分层，最后在同层内按难度分位和特征距离排序。结果中的难度 percentile 表示该谱面在同键数 Ranked 语料中的相对位置，不是 osu! 官方星数，也不能跨键数直接比较。支持 NM、DT、HT 单池或混池；暂不支持 5K/8K+、Key Mod、Random、自定义倍速或基于 SV 滚速的相似度。

## 仓库与发布策略

本地索引、特征文件、归一化文件和生成的检索文件会被刻意排除在版本控制之外。仅发布 NoMod 包时打包基础运行时文件；提供 DT/HT 功能的完整包还需额外打包 `mania-mod-features-v1.bin`。不要包含下载数据库、`mania-raw-features.bin` 或旧版修复前目录。

## 常见问题

- 如果解压后出现两层同名文件夹，请选择实际直接包含 `metadata.sqlite` 或 `mania-metadata.sqlite` 的内层目录。v4 运行包不需要 `beatmaps/`。
- 切换 standard 与 mania 后需要分别配置一次目录；应用会记住两套路径。
- 如果应用提示算法版本不兼容，请重新下载匹配当前运行时的完整数据集，不要混用不同版本的 metadata、features、normalizer 和 index。
- 数据集生成、覆盖范围和校验信息以对应的 `osu-difficulty-lab` Release 说明为准；索引结果不代表实时数据库。

## 键型数据

键型数据保存在 `mania-mma-features.bin` 与 `mania_mma_analyses` 表中，是数据集格式里的可选部分：旧数据集没有这一块，OPP 也能正常打开。配套数据集包含六类覆盖率、细分键型、主模式和 RC/LN 音符占比，记录版本为 2。旧 Mania 数据集和 standard 数据不受影响，仍走原来的读取路径。

OPP 不自己分析键型，只读取数据集里的记录。数据集没有键型记录，或者谱面内容与记录的校验值不一致时，该谱面就没有键型数据，继续按原来的 v1 规则读取和排序。

NM/DT/HT 候选来自各自的预计算记录，DT/HT 记录由 lab 的 mania-mod-export 生成。查询时直接比较同键数、所选 Mod 池里的全部记录，不再按旧版难度分段预筛，并按源文件校验值去重。旧数据集继续使用原 v1 查询。

推荐排序按五项加权：键型 50%、强度 25%、时间结构 15%、LN 音符比例差 7%、时长差 3%。键型部分包含覆盖率、细分键型、键型簇时长分布、主模式、各类别 BPM 和 SV 标记。

主模式沿用 mania_map_analyser 的分类结果，不取覆盖率最高的类别。数据集中有少量没有在线 ID 的本地谱面，OPP 不会为它们生成在线链接。
