//! 技能分析模块的数据契约：请求参数、八维技能向量、
//! 逐条成绩贡献与整体结果及覆盖统计。

use serde::{Deserialize, Serialize};

use crate::features::local_analysis::LocalClient;

/// 技能分析请求参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillAnalysisRequest {
    /// 规则集名，首版仅接受 `"osu"`。
    pub ruleset: String,
    /// 本地谱面来源客户端。
    pub client: LocalClient,
    /// 分析成绩数上限，首版固定 200，其他值会被拒绝。
    pub score_limit: usize,
    /// 本地缺谱面时是否允许在线下载补齐。
    pub include_online: bool,
    /// 强制刷新：跳过缓存读取，失败时允许回退旧缓存。
    pub force_refresh: bool,
}

/// 八维技能向量：耐力（stamina）、坚韧（tenacity）、敏捷（agility）、
/// 准确（accuracy）、精准（precision）、反应（reaction）、记忆（memory）、
/// 读谱（reading）。数值为算法经验刻度而非 PP，量级仅供维度间比较。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SkillVector {
    pub stamina: f64,
    pub tenacity: f64,
    pub agility: f64,
    pub accuracy: f64,
    pub precision: f64,
    pub reaction: f64,
    pub memory: f64,
    pub reading: f64,
}

impl SkillVector {
    /// 将另一向量的各维度按 `weight` 累加进自身（加权求和的累积步）。
    pub fn add_weighted(&mut self, value: &Self, weight: f64) {
        self.stamina += value.stamina * weight;
        self.tenacity += value.tenacity * weight;
        self.agility += value.agility * weight;
        self.accuracy += value.accuracy * weight;
        self.precision += value.precision * weight;
        self.reaction += value.reaction * weight;
        self.memory += value.memory * weight;
        self.reading += value.reading * weight;
    }

    /// 各维度统一乘以 `factor`（用于除以总权重完成归一化）。
    pub fn scale(&mut self, factor: f64) {
        self.stamina *= factor;
        self.tenacity *= factor;
        self.agility *= factor;
        self.accuracy *= factor;
        self.precision *= factor;
        self.reaction *= factor;
        self.memory *= factor;
        self.reading *= factor;
    }

    /// 按成绩质量系数整体缩放，但 Accuracy 维度保持原值（兼容上游口径）。
    /// Score quality corrections apply to every dimension except Accuracy,
    /// which intentionally keeps the upstream compatibility behavior.
    pub fn scale_score_factor(&mut self, factor: f64) {
        let accuracy = self.accuracy;
        self.scale(factor);
        self.accuracy = accuracy;
    }
}

/// 单条成绩的贡献明细：既是前端榜单一行的展示数据，也是聚合输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContribution {
    pub beatmap_id: u64,
    pub title: String,
    pub artist: String,
    pub version: String,
    pub creator: String,
    /// 成绩 Mod 缩写列表（统一大写）。
    pub mods: Vec<String>,
    /// 成绩 PP（osu! API 原值，可能缺失）。
    pub pp: Option<f64>,
    /// 命中准确率（0-1 小数，osu! API 口径）。
    pub accuracy: f64,
    pub combo: Option<u64>,
    /// 谱面理论满连击（API 缺失时由谱面重算）。
    pub max_combo: Option<u64>,
    pub misses: u32,
    /// PP 聚合权重：优先取 API `weight.percentage`，缺失时按 0.95^名次
    /// 兜底，下限 0.0001。
    pub weight: f64,
    /// 谱面来源：`local` 或 `online`。
    pub source: String,
    /// 本地资源库 ID（`online` 来源为 `None`）。
    pub resource_id: Option<String>,
    /// 谱面原始技能向量（未乘成绩质量系数）。
    pub skills: SkillVector,
    /// 已乘成绩质量系数的向量，聚合时再乘 `weight`。
    pub weighted_skills: SkillVector,
    /// 错误信息（当前恒为 `None`，失败成绩直接跳过并计入覆盖统计）。
    pub error: Option<String>,
}

/// 本次分析的覆盖统计：各类计数及逐条跳过原因。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCoverage {
    /// 请求分析的成绩总数（去重前）。
    pub requested_scores: usize,
    /// 实际产出贡献的成绩数（含复用）。
    pub analyzed_scores: usize,
    /// 其中直接复用旧缓存结果的数量。
    #[serde(default)]
    pub reused_scores: usize,
    /// 使用本地谱面的成绩数。
    pub local_scores: usize,
    /// 使用在线谱面的成绩数。
    pub online_scores: usize,
    /// 跳过未分析的成绩数。
    pub skipped_scores: usize,
    /// 跳过原因列表，与跳过成绩一一对应。
    pub skipped_reasons: Vec<String>,
}

/// 技能分析完整结果；同时作为持久化缓存与前端载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAnalysisResult {
    pub player: PlayerSummary,
    pub ruleset: String,
    pub algorithm: AlgorithmInfo,
    /// 按 PP 权重加权归一后的总技能向量。
    pub skills: SkillVector,
    pub contributions: Vec<SkillContribution>,
    pub coverage: SkillCoverage,
    /// 生成时间（RFC3339 UTC）。
    pub fetched_at: String,
    /// 刷新失败回退旧缓存时为 `true`。
    pub stale: bool,
}

/// 玩家展示信息摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSummary {
    pub id: u64,
    pub username: String,
    pub country_code: String,
    pub avatar_url: String,
    /// 内嵌头像 data URL（可能为 `None`）。
    pub avatar_data_url: Option<String>,
}

/// 算法标识信息；版本号参与缓存键的构成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmInfo {
    pub id: String,
    pub version: String,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::SkillVector;

    // 质量系数缩放时 Accuracy 维度必须保持不变。
    #[test]
    fn score_factor_preserves_accuracy_dimension() {
        let mut skills = SkillVector {
            stamina: 100.0,
            tenacity: 100.0,
            agility: 100.0,
            accuracy: 100.0,
            precision: 100.0,
            reaction: 100.0,
            memory: 100.0,
            reading: 100.0,
        };
        skills.scale_score_factor(0.5);
        assert_eq!(skills.accuracy, 100.0);
        assert_eq!(skills.stamina, 50.0);
    }
}
