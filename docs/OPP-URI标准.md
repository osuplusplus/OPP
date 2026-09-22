# OPP URI 标准 v1

OPP URI 标准用于从网页或其他应用打开 osu! 图池，并在 OPP 收藏页保存、查看和练习。标准起源于 **ASC 星域杯** 的图池联动需求；ASC 是首个兼容实例，不是协议的使用限制。其他赛事可自行托管符合本规范的 JSON，无需 OPP 为每个赛事发布适配版本。

v1 仅支持 **osu!standard**。本规范中的“必须”表示接入要求，“可选”表示省略后仍可正常导入。OPP 是独立社区工具，本协议不属于 osu! 官方协议。

## 1. URI 入口

```text
opp://mappool/import?url=<百分号编码的公开 HTTPS JSON 地址>
```

完整示例：

```text
opp://mappool/import?url=https%3A%2F%2Fexample.com%2Fpools%2Fs1%2Fqualification.json
```

赛事页面可生成按钮：

```html
<a id="open-opp">在 OPP 中打开图池</a>
<script>
  const source = 'https://example.com/pools/s1/qualification.json';
  document.getElementById('open-opp').href =
    'opp://mappool/import?url=' + encodeURIComponent(source);
</script>
```

`url` 必须且只能出现一次，整个来源地址编码一次。来源本身包含 query 时，`?`、`&`、`=` 同样属于被编码的内容。禁止附加动作参数、URI 用户名、密码、端口和 fragment。OPP URI 总长最多 2048 字节，解码后的来源地址最多 1800 字节。

每个图池应使用**长期稳定且独立的来源 URL**。OPP 用规范化后的完整 URL（包括 query）识别收藏：主机名大小写、默认 HTTPS 端口等按 URL 规则归一化；不会排序 query 或移除参数。改变路径或 query 通常意味着另一个收藏。文档中的赛事名称和阶段是展示信息，不用于匹配、覆盖其他来源。

来源必须使用公开 HTTPS、443 端口，无凭据和 fragment。OPP 拒绝本地、内网、回环、链路本地、特殊用途地址和 HTTP 重定向，验证 DNS 返回的全部地址并固定此次连接的解析结果，不使用系统代理访问图池来源。接口须可匿名读取 JSON，不应在地址中携带令牌。

## 2. 图池 JSON

最小文档：

```json
{
  "version": 1,
  "title": "示例杯 资格赛",
  "entries": [
    { "beatmap_id": 1234567, "selection_type": "NM", "position": 1 }
  ]
}
```

完整文档（示意数据，不代表 ASC 实际选图）：

```json
{
  "version": 1,
  "title": "示例杯 S1 资格赛",
  "ruleset": "osu",
  "tournament": "示例杯",
  "season": "S1",
  "category": "资格赛",
  "entries": [
    {
      "beatmap_id": 1234567,
      "selection_type": "NM",
      "position": 1,
      "selected_by": "12345",
      "selected_by_name": "Picker",
      "comment": "注意后半段的节奏变化。",
      "is_custom": false,
      "is_original": true
    },
    { "beatmap_id": 2345678, "selection_type": "TB", "position": 1 }
  ]
}
```

### 文档字段

| 字段 | 类型 | 要求与默认值 |
| --- | --- | --- |
| `version` | 整数 | 必填，必须为 `1`；协议版本与 OPP 应用版本独立 |
| `title` | 字符串 | 必填，去除首尾空白后非空，最多 120 个 Unicode 字符 |
| `entries` | 数组 | 必填，最多 500 个图位；空数组表示尚未发布，不创建或覆盖收藏 |
| `ruleset` | 字符串 | 可选，省略或 `null` 为 `osu`；v1 仅接受 `osu` |
| `tournament` | 字符串 | 可选，赛事名称，最多 120 个字符 |
| `season` | 字符串 | 可选，赛季展示文字，最多 120 个字符 |
| `category` | 字符串 | 可选，阶段展示文字，最多 120 个字符，无固定枚举 |

### 图位字段

| 字段 | 类型 | 要求与默认值 |
| --- | --- | --- |
| `beatmap_id` | 整数 | 必填，osu! 官方难度 BID，范围 1–2147483647，不是曲包 SID |
| `selection_type` | 字符串 | 必填，非空、无首尾空白，最多 32 UTF-8 字节，例如 `NM`、`HD`、`HR`、`DT`、`LZ`、`TB` |
| `position` | 整数 | 必填，范围 1–4294967295；与类型组成图位，例如 `NM1` |
| `selected_by` | 字符串 | 可选，选图人 ID，最多 120 个字符；即使是数字 ID 也使用字符串 |
| `selected_by_name` | 字符串 | 可选，选图人显示名称，最多 120 个字符；优先显示名称，缺省回退到 ID |
| `comment` | 字符串 | 可选，选图评语，最多 30000 个字符；省略或 `null` 为无评语 |
| `is_custom` | 布尔值 | 可选，是否比赛定制，默认 `false` |
| `is_original` | 布尔值 | 可选，是否原创，默认 `false` |

可选文本字段允许省略或 `null`，空文本不展示。布尔字段只能是布尔值，省略时为 `false`。同一 `(selection_type, position)` 必须唯一；同一 BID 可占多个图位，收藏成员与下载按 BID 去重，页面保留每个图位。

图位类型是赛事标签，保留原值，不自动转换为游戏 Mod 设置。通用文档的 `entries` 顺序作为收藏顺序保存，页面也可按图位排序。图池不提供可执行动作、保存路径、曲包下载地址或任意 HTML；v1 拒绝未定义字段。需要新增协议能力时应明确升级规范与客户端，而不是改变现有字段语义。

谱面标题、谱师、难度名、曲包 ID、MD5 等由 OPP 现有 osu! API／镜像 provider 补全，不需要赛事重复提供。单张谱面资料失败不阻止其他条目保存；缺少资料的条目保留 BID，后续手动同步可重试补全。

## 3. 唤醒与本地收藏

1. 浏览器请求打开 OPP；OPP 恢复主窗口，在现有启动／登录流程完成后进入收藏页。
2. 首次导入显示读取图池、补全资料、保存到 OPP、完成等真实任务阶段。成功后自动进入该收藏。
3. 再次打开同一来源直接读取已经保存的本地图池，不自动联网更新；重启后仍可离线浏览。
4. 用户点击“同步图池”时才重新请求来源。同步严格替换图池成员及比赛元数据，移除旧图和手动加入的条目，保留收藏 ID、用户改名、创建时间和个人标签／笔记／成绩。已移除谱面的个人记录不会因来源同步而删除，重新出现时可恢复。

新 URI 收藏默认**仅保存在 OPP**，不要求安装游戏，不提示用户离页前写回，不被全局 Stable 写回顺带导入。删除该收藏才会删除对应个人记录；本地游戏文件不受影响。

图池成员与来源快照保存在同一收藏分片。旧版独立快照仍可读取，首次同步后写入新结构。旧收藏保持原有 Stable 同步行为；本次新建的 URI 收藏才默认关闭 Stable 同步。

重复的唤醒通知不重复导入。导入任务串行执行，尚未开始的不同链接只保留最新一个；旧任务可以完成保存，但不能覆盖最新链接的页面。请求失败、错误 JSON、空图池或保存失败会显示错误并提供重试，不以空响应清空已保存图池。系统“减少动态效果”设置会关闭导入动画。

## 4. 下载与游戏打开

- **下载图池**：下载当前保存的图池成员，不自动同步来源。按曲包合并，校验曲包包含所有选中的 BID，允许包内附带其他难度。复用现有下载源、目录、视频和会话管理；没有目录时选择目录，不要求 Stable 安装。缺少资料或禁止下载的条目会列出并跳过。此操作不自动打开曲包，不写游戏收藏数据库。 下载结束可点击“一键打开已下载曲包”，调用系统 `.osz` 关联应用，打开失败可重试；这不会开启 Stable 收藏同步。默认先用小夜，再回退 Catboy、Nerinyan；Hinai 仅在手动选中时参与。下载最多两路并行，单源响应上限 20 秒、无数据上限 15 秒，连续 20 秒不足 32 KiB 会回退；总请求上限 120 秒，单包上限 256 MiB。同一谱面集附带的未提交难度（BID 0/-1）不参与已提交难度比对，所选 BID 仍必须全部存在。官网 `.osz` API [要求 lazer 授权](https://osu.ppy.sh/docs/index.html#beatmapsets-GETapi-v2-beatmapsets--beatmapset--download)，OPP 当前登录无法自动调用；失败项提供官网入口，在浏览器登录后补下载，浏览器保存的文件不计入 OPP 本批成功结果。
- **导入 Stable**：明确将该收藏加入 Stable 同步范围，再执行现有补齐、备份、冲突检查和写回流程。执行失败时仍保留 OPP 数据，可修复原因后重试。该收藏随后参与正常 Stable 同步。
- **在 lazer 中打开**：对有效 BID 生成 `osu://b/<BID>`，直接作为参数交给检测到的 lazer 程序；不依赖系统默认 `osu://` 关联，所以默认关联到 Stable 也不会选错客户端。打开失败会展示错误，可检查安装路径或打开谱面官网。打开对应谱面不等于自动开始游玩。
- 本版本不实现 lazer collection 数据库写入。比赛信息、图位、标签、笔记和成绩留在 OPP，逐图打开用于在 lazer 中查看／练习。

lazer URI 行为参考 [官方链接解析代码](https://github.com/ppy/osu/blob/master/osu.Game/Online/Chat/MessageFormatter.cs) 与 [官方桌面协议注册](https://github.com/ppy/osu/blob/master/osu.Desktop/Windows/WindowsAssociationManager.cs)。

## 5. ASC 兼容入口与来源

```text
opp://mappool/rino?season=s1&category=qualification
```

旧入口继续支持 `season=s1|s2`，阶段为 `qualification`、`ro16`、`quarterfinals`、`semifinals`、`finals`、`grandfinals`。二者必须且只能出现一次，不接受其他参数。

兼容适配器固定读取 `https://rino.ink/api/map-selections`，附加 `approved=true`、`padding=true`。它将历史字段 `selectedMods`、`modPosition`、`selectedByUsername`、`isCustome`、`isOrigin` 转换为 OPP 语义。`isCustome` 的历史拼写只存在于此适配器；标准使用 `is_custom`，评语使用 `comment`。

旧来源标识 `tournament:rino:<season>:<category>` 保持不变，已有收藏不会因为标准升级重复创建。通用 URL 入口是独立来源，不会根据自报赛事名称合并进旧 ASC 收藏。界面与新建收藏使用 ASC 星域杯名称，用户已经修改的收藏名保留。

## 6. 平台、接口与验收

Windows、Linux 正式构建正常启动时注册 `opp` 协议，目标为当前程序路径。便携版首次使用、移动或重命名后须先运行一次。Windows 使用当前用户注册，无需管理员；Linux 依赖 `xdg-mime` 和 `update-desktop-database`。开发构建和便携更新助手不覆盖正式版协议注册。注册失败记日志但不阻止应用启动。

现有 Tauri 命令名称保留；reference 支持 `{ provider: "rino", season, category }` 或 `{ provider: "opp", url }`：

| 命令／事件 | 作用 |
| --- | --- |
| `get_tournament_pool({ reference })` | 读取、校验并补全远程图池 |
| `sync_tournament_pool_collection({ reference })` | 显式同步，返回 `folder_id`、`entry_count`、`pool` |
| `open_tournament_pool({ reference, requestId })` | 本地优先打开或首次导入，返回 `folder_id`、`existing` |
| `get_pending_tournament_link()` / `acknowledge_tournament_link({ id })` | 启动／登录期间的待处理链接及按请求 ID 确认 |
| `tournament-pool-open` | 主窗口唤醒事件，携带 `id`、`reference` |
| `tournament-pool-import-progress` | 主窗口阶段事件，携带 `request_id`、`phase`（`fetching`、`enriching`、`saving`） |
| `enable_collection_stable_sync({ folderId })` | 用户明确要求导入 Stable 时加入同步范围 |
| `open_lazer_beatmap({ beatmapId })` | 校验 BID 并将官方 URI 交给 lazer |

来源 DNS 超时为 10 秒，HTTP 请求超时为 20 秒，响应体最大 2 MiB；单份文档最多 500 个图位。HTTP 错误、重定向、过大响应、版本不支持、数据校验失败都不视作空图池。

自动化回归覆盖协议校验、可选字段、多图位、来源地址、导入去重与串行处理、本地重开、快照与个人记录、同步范围、下载隔离、查询刷新及 lazer 按钮。

发布前必须在 Windows 和 Linux **分别实机验收**：首次注册、浏览器冷启动／运行中唤醒、登录后恢复、托盘隐藏、迷你播放器恢复、重复／连续不同链接、程序移动后重新注册，以及 lazer 冷／热启动打开。另需验证默认 `osu://` 关联到 Stable 时仍打开 lazer。自动化测试和交叉编译不能替代这些平台实测。

### 收藏页展示与旧快照补全

具体文件夹隐藏总览页标题和全局工具栏；导入成功后阶段面板自动隐藏，下载完成后结果默认折叠，仍可展开查看失败原因及打开曲包。图池设置内保留手动同步及其成员替换说明。

在线谱面资料（封面标识、星数、BPM、时长、AR／OD／CS／HP、可用的物件数量和最大 Combo）随快照保存，显示不依赖本地安装。未在本地索引中找到谱面时仅标记“未下载”；需要本地分析的峰值 NPS 不以空值占位。

旧快照首次查看时只补全缺少的谱面资料，不重新请求来源 JSON，不同步图池成员，不开启 Stable 同步，也不改个人记录。补全成功的数据持久保存；失败保留已有资料并提供重试。完整快照离线查看不会重复请求谱面资料。
