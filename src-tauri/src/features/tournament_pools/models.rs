use serde::{Deserialize, Serialize};

use crate::error::{CommandError, CommandResult};

pub const CATEGORIES: [(&str, &str); 6] = [
    ("qualification", "资格赛"),
    ("ro16", "十六强"),
    ("quarterfinals", "四分之一决赛"),
    ("semifinals", "半决赛"),
    ("finals", "决赛"),
    ("grandfinals", "总决赛"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TournamentPoolRef {
    pub provider: String,
    pub season: String,
    pub category: String,
}

impl TournamentPoolRef {
    pub fn validate(&self) -> CommandResult<()> {
        if self.provider != "rino"
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

    pub fn source_id(&self) -> String {
        format!(
            "tournament:{}:{}:{}",
            self.provider, self.season, self.category
        )
    }

    pub fn title(&self) -> String {
        let category = CATEGORIES
            .iter()
            .find(|(id, _)| *id == self.category)
            .map(|(_, label)| *label)
            .unwrap_or(&self.category);
        format!("Rino {} {category}", self.season.to_uppercase())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TournamentBeatmap {
    pub beatmapset_id: i32,
    pub title: String,
    pub artist: String,
    pub creator: String,
    pub difficulty_name: String,
    pub checksum: Option<String>,
    pub download_disabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TournamentPoolEntry {
    pub beatmap_id: i32,
    pub selection_type: String,
    pub position: u32,
    pub selected_by: Option<String>,
    pub selected_by_name: Option<String>,
    pub comment: String,
    pub is_custom: bool,
    pub is_original: bool,
    pub beatmap: Option<TournamentBeatmap>,
    pub resolution_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TournamentPool {
    pub reference: TournamentPoolRef,
    pub title: String,
    pub entries: Vec<TournamentPoolEntry>,
}

#[derive(Debug, Serialize)]
pub struct TournamentPoolSyncResult {
    pub folder_id: String,
    pub entry_count: usize,
    pub pool: TournamentPool,
}
