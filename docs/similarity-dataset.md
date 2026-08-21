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

## osu!mania Analyzer v1

Mania 运行时严格匹配提交 [`1fa21fa6a5144992df58efe7ce9d96019981fad3`](https://github.com/osuplusplus/osu-difficulty-lab/commit/1fa21fa6a5144992df58efe7ce9d96019981fad3) 中的 `mania-roxy-interlude-similarity-v1`，只支持原生 NoMod 4K、6K 和 7K 谱面。上游 `v0.3.0` tag 不包含 Mania 管线，不能把该 tag 当作兼容版本；在上游发布后续 tag 前应按完整提交 SHA 识别产物。Mania 数据集根目录必须直接包含以下五个运行时文件：

```text
mania-metadata.sqlite
mania-features-v1.bin
normalizers/mania-v1.bin
indexes/mania-v1.buckets
indexes/mania-v1.buckets.sha256
```

`mania-raw-features.bin`、原始 `beatmaps/`、catalog 和下载 manifest 只用于重建或审计，不是 OPP 查询所需文件。不要分发或选择 `mania-v1-pre-filename-id-fix/`；该目录是修复 BeatmapID 映射前的旧产物。

Mania 会按键数分别归一化并检索。结果中的难度 percentile 表示该谱面在同键数 Ranked 语料中的相对位置，不是 osu! 官方星数，也不能跨键数直接比较。首版不支持 5K/8K+、Key Mod、DT/HT、Random、自定义倍速或基于 SV 滚速的相似度。

## 仓库与发布策略

本地索引、特征文件、归一化文件和生成的检索文件会被刻意排除在版本控制之外。发布 Mania 查询数据时只需打包上面的五个运行时文件；不要包含原始谱面、下载数据库、`mania-raw-features.bin` 或旧版修复前目录。

## 常见问题

- 如果解压后出现两层同名文件夹，请选择实际直接包含 `metadata.sqlite` 或 `mania-metadata.sqlite` 的内层目录。
- 切换 standard 与 mania 后需要分别配置一次目录；应用会记住两套路径。
- 如果应用提示算法版本不兼容，请重新下载匹配当前运行时的完整数据集，不要混用不同版本的 metadata、features、normalizer 和 index。
- 数据集生成、覆盖范围和校验信息以对应的 `osu-difficulty-lab` Release 说明为准；索引结果不代表实时数据库。
