# ASC 星域杯 S2 官网资料

2026-10-08 核对，正式录入内容见 [astracup-s2.json](./astracup-s2.json)。合作关系由项目维护者确认；赛事信息来自官网公开资料。

已录入 VPS，公告 ID：`a263392b-e70a-4918-83bc-5d6284f5df4c`。服务端“待公布日期”兼容改动、数据库迁移与录入脚本保存于 [vps-pending-dates.patch](./vps-pending-dates.patch)。该补丁应用于服务端项目，不属于客户端代码。上线前已备份数据库，服务端通过类型、Lint、构建与 20 项测试（含独立 PostgreSQL 集成测试），上线后验证默认列表、关键词／模式／状态筛选及详情。

| 信息 | 核对结果 | 来源 |
| --- | --- | --- |
| 赛季与状态 | 当前赛季 S2，upcoming；S1 已结束 | [公开设置](https://rino.ink/api/settings) |
| 模式与客户端 | osu!standard，osu!lazer | [S2 比赛手册](https://rino.ink/guide) |
| 主办方 | AeCw，官网标注“主办、视觉设计” | [联系赛事组](https://rino.ink/contact) |
| STAFF | 2026-09-20 发布招募，裁判、选图、直播、测图、规则及创作岗位 | [S2 招募](https://rino.ink/news/5) |
| 选手报名 | registration_enabled 为 false；日期尚未公布 | [公开设置](https://rino.ink/api/settings) |
| 参赛 PP | 设置为 2000–9000 PP；S2 手册草案约 9400 PP，存在冲突，尚待正式公告确认 | [设置](https://rino.ink/api/settings)、[手册](https://rino.ink/guide) |
| 比赛日程 | S2 未公布日期；当前赛程接口为空 | [赛程](https://rino.ink/schedule)、[赛程接口](https://rino.ink/api/schedules) |
| 图池、奖励 | 手册草案暂列 GF 约 7.2★，奖励沿用 S1；标明待确认 | [S2 手册](https://rino.ink/guide) |
| 封面 | 官网没有找到独立 S2 海报，使用官方 Logo；不把群二维码当海报 | [官方 Logo](https://rino.ink/NewLogo/newLogo.svg) |

报名及比赛日期均录入 `null`，不沿用 S1 日期，不推断开赛日期。`registration_url` 指向赛事首页，当前开放的 STAFF 招募入口在正文中单独注明。更新赛事后应同步本资料和录入 JSON。
