//! 玩家技能分析模块：基于 osuSkills 公开模型的八维评估，
//! 将玩家 Top 200 成绩按 PP 权重聚合为技能向量。

mod algorithm;
mod commands;
mod models;

pub use commands::analyze_player_skills;
