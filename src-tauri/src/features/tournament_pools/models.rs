//! 图池领域模型与面向前端的序列化契约。
use serde::{Deserialize, Serialize};

use crate::error::{CommandError, CommandResult};

/// rino 图池支持的阶段 ID 与中文标签对照，既用于引用校验，也用于标题展示。
pub const CATEGORIES: [(&str, &str); 6] = [
    ("qualification", "资格赛"),
    ("ro16", "十六强"),
    ("quarterfinals", "四分之一决赛"),
    ("semifinals", "半决赛"),
    ("finals", "决赛"),
    ("grandfinals", "总决赛"),
];

/// 唯一确定一个图池来源的引用，同时充当收藏夹与图池之间的关联键。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TournamentPoolRef {
    /// 来源类型：`opp`（标准 JSON 文档导入）或 `rino`（赛事接口）。
    pub provider: String,
    /// rino 赛季，仅允许 `s1`/`s2`；`opp` 来源必须留空。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub season: String,
    /// rino 阶段，取值必须是 `CATEGORIES` 中的 ID；`opp` 来源必须留空。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// `opp` 来源的图池文档地址（须为公开 HTTPS）；`rino` 来源必须为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl TournamentPoolRef {
    /// 校验引用自身的合法组合：`opp` 只校验公开 HTTPS 来源地址，
    /// `rino` 则强制限定赛季与阶段枚举且不允许携带 `url`。
    pub fn validate(&self) -> CommandResult<()> {
        // `opp` 分支：赛季与阶段留空，仅要求来源地址本身合法可访问。
        if self.provider == "opp" && self.season.is_empty() && self.category.is_empty() {
            return super::standard::source_url(self.url.as_deref().unwrap_or("")).map(|_| ());
        }
        // 其余输入一律按 rino 的严格枚举校验，任一条件不满足即整体拒绝。
        if self.provider != "rino"
            || self.url.is_some()
            || !matches!(self.season.as_str(), "s1" | "s2")
            || !CATEGORIES.iter().any(|(id, _)| *id == self.category)
        {
            return Err(CommandError::new(
                "INVALID_TOURNAMENT_POOL",
                "不支持的比赛、赛季或阶段",
            ));
        }
        Ok(())
    }

    /// 生成稳定的来源标识：收藏夹据此识别同一图池、避免重复建夹；
    /// `opp` 把规范化后的 URL 拼入标识，`rino` 使用“提供方:赛季:阶段”三元组。
    pub fn source_id(&self) -> String {
        if self.provider == "opp" {
            let url = super::standard::source_url(self.url.as_deref().unwrap_or(""));
            return format!(
                "tournament:opp:{}",
                url.map(|u| u.to_string()).unwrap_or_default()
            );
        }
        format!(
            "tournament:{}:{}:{}",
            self.provider, self.season, self.category
        )
    }

    /// rino 图池的展示标题（固定“ASC 星域杯”前缀，阶段转中文标签）；
    /// 标准文档图池不用此方法，标题取自文档自身 `title` 字段。
    pub fn title(&self) -> String {
        let category = CATEGORIES
            .iter()
            .find(|(id, _)| *id == self.category)
            .map(|(_, label)| *label)
            .unwrap_or(&self.category);
        format!("ASC 星域杯 {} {category}", self.season.to_uppercase())
    }
}

/// 补全后的谱面展示资料，来自 osu! 官方接口或 Nerinyan 镜像。
#[derive(Debug, Clone, Serialize)]
pub struct TournamentBeatmap {
    pub beatmapset_id: i32,
    pub title: String,
    pub artist: String,
    pub creator: String,
    pub difficulty_name: String,
    /// 谱面 MD5 校验和，用于收藏夹候选与本地文件匹配。
    pub checksum: Option<String>,
    /// 谱集是否被作者禁用下载，保存快照时用于标记不可下载的图位。
    pub download_disabled: bool,
    /// 接口返回的原始谱面 JSON，随图池快照原样落盘，重启后无需重新请求。
    pub metadata: serde_json::Value,
}

/// 图池中的单个图位：谱面 ID 加选图信息，谱面资料在补全阶段回填。
#[derive(Debug, Clone, Serialize)]
pub struct TournamentPoolEntry {
    pub beatmap_id: i32,
    /// 图位类型（NM/HD/DT 等模组缩写），与 `position` 的组合在池内必须唯一。
    pub selection_type: String,
    /// 同一类型内的序号，从 1 开始，0 视为非法。
    pub position: u32,
    /// 选图人 ID（rino 接口提供），仅作展示。
    pub selected_by: Option<String>,
    /// 选图人用户名，展示时优先于 `selected_by`。
    pub selected_by_name: Option<String>,
    pub comment: String,
    /// 是否为自定义图位（对应 rino 上游的 `isCustome` 字段）。
    pub is_custom: bool,
    /// 是否为原图位（对应 rino 上游的 `isOrigin` 字段）。
    pub is_original: bool,
    /// 补全成功后的谱面资料。
    pub beatmap: Option<TournamentBeatmap>,
    /// 补全失败的错误消息，前端据此提示刷新重试。
    pub resolution_error: Option<String>,
}

/// 一个完整图池：来源引用、标题、比赛信息与全部图位。
#[derive(Debug, Clone, Serialize)]
pub struct TournamentPool {
    pub reference: TournamentPoolRef,
    pub title: String,
    /// `flatten` 把比赛信息平铺到图池顶层，与前端展示契约保持一致。
    #[serde(flatten)]
    pub info: TournamentInfo,
    pub entries: Vec<TournamentPoolEntry>,
}

/// 图池自述的比赛信息（均可选），来自标准文档字段或 rino 接口回填。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tournament: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// 显式同步的结果：目标收藏夹 ID、同步后的条目数与解析出的图池。
#[derive(Debug, Serialize)]
pub struct TournamentPoolSyncResult {
    pub folder_id: String,
    pub entry_count: usize,
    pub pool: TournamentPool,
}

/// 打开图池的结果：`existing` 为 `true` 表示命中已关联的本地文件夹，未发起网络请求。
#[derive(Serialize)]
pub struct PoolOpenResult {
    pub folder_id: String,
    pub existing: bool,
}
