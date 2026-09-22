use serde::{Deserialize, Serialize};

use crate::features::local_analysis::LocalClient;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillAnalysisRequest {
    pub ruleset: String,
    pub client: LocalClient,
    pub score_limit: usize,
    pub include_online: bool,
    pub force_refresh: bool,
}

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

    /// Score quality corrections apply to every dimension except Accuracy,
    /// which intentionally keeps the upstream compatibility behavior.
    pub fn scale_score_factor(&mut self, factor: f64) {
        let accuracy = self.accuracy;
        self.scale(factor);
        self.accuracy = accuracy;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContribution {
    pub beatmap_id: u64,
    pub title: String,
    pub artist: String,
    pub version: String,
    pub creator: String,
    pub mods: Vec<String>,
    pub pp: Option<f64>,
    pub accuracy: f64,
    pub combo: Option<u64>,
    pub max_combo: Option<u64>,
    pub misses: u32,
    pub weight: f64,
    pub source: String,
    pub resource_id: Option<String>,
    pub skills: SkillVector,
    pub weighted_skills: SkillVector,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCoverage {
    pub requested_scores: usize,
    pub analyzed_scores: usize,
    #[serde(default)]
    pub reused_scores: usize,
    pub local_scores: usize,
    pub online_scores: usize,
    pub skipped_scores: usize,
    pub skipped_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAnalysisResult {
    pub player: PlayerSummary,
    pub ruleset: String,
    pub algorithm: AlgorithmInfo,
    pub skills: SkillVector,
    pub contributions: Vec<SkillContribution>,
    pub coverage: SkillCoverage,
    pub fetched_at: String,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSummary {
    pub id: u64,
    pub username: String,
    pub country_code: String,
    pub avatar_url: String,
    pub avatar_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmInfo {
    pub id: String,
    pub version: String,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::SkillVector;

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
