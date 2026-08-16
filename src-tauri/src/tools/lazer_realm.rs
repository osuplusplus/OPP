//! 读取测试命令：通过 realm-db-reader 只读解析 lazer 的 client.realm，
//! 返回谱面集清单。核心读取逻辑在 `local_analysis::lazer_realm`，
//! 本命令主要用于在工具页验证 Realm 链路是否可用。

use serde::Serialize;

use crate::{
    error::{CommandError, CommandResult},
    local_analysis::lazer_realm as realm,
    platform,
};

#[derive(Debug, Clone, Serialize)]
pub struct LazerRealmBeatmapFile {
    pub filename: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LazerRealmBeatmapSet {
    pub id: String,
    pub online_id: i64,
    pub artist: String,
    pub title: String,
    pub creator: String,
    pub beatmap_count: usize,
    pub delete_pending: bool,
    pub files: Vec<LazerRealmBeatmapFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LazerRealmReadResult {
    pub realm_path: String,
    pub table_count: usize,
    pub beatmap_set_count: usize,
    pub beatmap_sets: Vec<LazerRealmBeatmapSet>,
}

#[tauri::command]
pub async fn read_lazer_realm_beatmap_sets() -> CommandResult<LazerRealmReadResult> {
    let data_root = platform::lazer_data_root()
        .ok_or_else(|| CommandError::new("LAZER_NOT_FOUND", "未找到 osu!lazer 数据目录"))?;
    let realm_path = data_root.join("client.realm");
    if !realm_path.is_file() {
        return Err(CommandError::new(
            "REALM_NOT_FOUND",
            format!("未找到 client.realm：{}", realm_path.display()),
        ));
    }
    let realm_path_display = realm_path.display().to_string();
    tokio::task::spawn_blocking(move || {
        let data = realm::read_realm_data(&realm_path)
            .map_err(|message| CommandError::new("REALM_READ_FAILED", message))?;
        let beatmap_sets = data
            .sets
            .into_iter()
            .map(|set| LazerRealmBeatmapSet {
                id: set.id,
                online_id: set.online_id,
                artist: set.artist,
                title: set.title,
                creator: set.creator,
                beatmap_count: set.beatmaps.len(),
                delete_pending: false,
                files: set
                    .files
                    .into_iter()
                    .map(|file| LazerRealmBeatmapFile {
                        filename: file.filename,
                        hash: file.hash,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        Ok(LazerRealmReadResult {
            realm_path: realm_path_display,
            table_count: data.table_count,
            beatmap_set_count: beatmap_sets.len(),
            beatmap_sets,
        })
    })
    .await
    .map_err(|join| CommandError::new("REALM_READ_FAILED", join.to_string()))?
}
