use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TosuStatus {
    pub installed: bool,
    pub executable_path: Option<String>,
    pub api_base_url: String,
    pub api_reachable: bool,
    pub running: bool,
    pub owned_by_opp: bool,
    pub dashboard_url: String,
    pub last_error: Option<String>,
    pub lyrics: TosuLyricsStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TosuLyricsStatus {
    pub installed: bool,
    pub executable_path: Option<String>,
    pub running: bool,
    pub owned_by_opp: bool,
    pub proxy_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TosuLogEntry {
    pub at: DateTime<Utc>,
    pub stream: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TosuLiveSnapshot {
    pub state: Option<String>,
    pub mode: Option<String>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub difficulty: Option<String>,
    pub song_time_ms: Option<i64>,
    pub song_length_ms: Option<i64>,
    pub score: Option<u64>,
    pub combo: Option<u64>,
    pub max_combo: Option<u64>,
    pub accuracy: Option<f64>,
    pub misses: Option<u64>,
    pub hit_300: Option<u64>,
    pub hit_100: Option<u64>,
    pub hit_50: Option<u64>,
    pub pp_current: Option<f64>,
    pub pp_fc: Option<f64>,
    pub mods: Option<String>,
    /// 当前谱面身份（多人补缺/覆盖层用）。tosu 用 0/-1 表示"无谱面"，
    /// 这里统一归一化为 `None`。
    pub beatmap_id: Option<i64>,
    pub beatmapset_id: Option<i64>,
    pub beatmap_md5: Option<String>,
}

fn value<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(root, |current, key| current.get(*key))
}

fn text(root: &Value, path: &[&str]) -> Option<String> {
    value(root, path)?.as_str().map(ToOwned::to_owned)
}

fn integer(root: &Value, path: &[&str]) -> Option<i64> {
    value(root, path)?
        .as_i64()
        .or_else(|| value(root, path)?.as_u64().map(|number| number as i64))
}

fn unsigned(root: &Value, path: &[&str]) -> Option<u64> {
    value(root, path)?
        .as_u64()
        .or_else(|| value(root, path)?.as_f64().map(|number| number as u64))
}

fn decimal(root: &Value, path: &[&str]) -> Option<f64> {
    value(root, path)?
        .as_f64()
        .or_else(|| value(root, path)?.as_u64().map(|number| number as f64))
}

/// 正数才有意义（tosu 用 0/-1 占位表示无数据）。
fn positive_integer(root: &Value, path: &[&str]) -> Option<i64> {
    integer(root, path).filter(|number| *number > 0)
}

impl TosuLiveSnapshot {
    pub fn from_v2(value: &Value) -> Self {
        Self {
            state: text(value, &["state", "name"]),
            mode: text(value, &["play", "mode", "name"])
                .or_else(|| text(value, &["settings", "mode", "name"])),
            artist: text(value, &["beatmap", "artistUnicode"])
                .or_else(|| text(value, &["beatmap", "artist"])),
            title: text(value, &["beatmap", "titleUnicode"])
                .or_else(|| text(value, &["beatmap", "title"])),
            difficulty: text(value, &["beatmap", "version"]),
            song_time_ms: integer(value, &["beatmap", "time", "live"]),
            song_length_ms: integer(value, &["beatmap", "time", "lastObject"]),
            score: unsigned(value, &["play", "score"]),
            combo: unsigned(value, &["play", "combo", "current"]),
            max_combo: unsigned(value, &["play", "combo", "max"]),
            accuracy: decimal(value, &["play", "accuracy"]),
            misses: unsigned(value, &["play", "hits", "0"]),
            hit_300: unsigned(value, &["play", "hits", "300"]),
            hit_100: unsigned(value, &["play", "hits", "100"]),
            hit_50: unsigned(value, &["play", "hits", "50"]),
            pp_current: decimal(value, &["play", "pp", "current"]),
            pp_fc: decimal(value, &["play", "pp", "fc"]),
            mods: text(value, &["play", "mods", "name"]),
            beatmap_id: positive_integer(value, &["beatmap", "id"]),
            beatmapset_id: positive_integer(value, &["beatmap", "setId"])
                .or_else(|| positive_integer(value, &["beatmap", "set"])),
            beatmap_md5: text(value, &["beatmap", "checksum"])
                .or_else(|| text(value, &["beatmap", "md5"]))
                .filter(|checksum| !checksum.is_empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_live_core_fields() {
        let data = serde_json::json!({
            "state": {"name": "Playing"}, "play": {"mode": {"name": "Osu"}, "score": 1234, "accuracy": 98.5, "combo": {"current": 42, "max": 88}, "hits": {"0": 1, "100": 2, "300": 3, "50": 4}, "pp": {"current": 12.3, "fc": 45.6}, "mods": {"name": "HD"}},
            "beatmap": {"artistUnicode": "artist", "titleUnicode": "title", "version": "Insane", "time": {"live": 1000, "lastObject": 2000}}
        });
        let snapshot = TosuLiveSnapshot::from_v2(&data);
        assert_eq!(snapshot.title.as_deref(), Some("title"));
        assert_eq!(snapshot.combo, Some(42));
        assert_eq!(snapshot.pp_fc, Some(45.6));
    }

    #[test]
    fn maps_beatmap_identity_fields() {
        let data = serde_json::json!({
            "state": {"name": "MultiplayerLobby"},
            "beatmap": {"id": 123456, "set": 654321, "checksum": "abc123"}
        });
        let snapshot = TosuLiveSnapshot::from_v2(&data);
        assert_eq!(snapshot.beatmap_id, Some(123456));
        assert_eq!(snapshot.beatmapset_id, Some(654321));
        assert_eq!(snapshot.beatmap_md5.as_deref(), Some("abc123"));
    }

    #[test]
    fn maps_beatmap_identity_field_aliases() {
        let data = serde_json::json!({
            "beatmap": {"id": 42, "setId": 99, "md5": "deadbeef"}
        });
        let snapshot = TosuLiveSnapshot::from_v2(&data);
        assert_eq!(snapshot.beatmap_id, Some(42));
        assert_eq!(snapshot.beatmapset_id, Some(99));
        assert_eq!(snapshot.beatmap_md5.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn normalizes_placeholder_beatmap_identity() {
        let data = serde_json::json!({
            "beatmap": {"id": 0, "set": -1, "checksum": ""}
        });
        let snapshot = TosuLiveSnapshot::from_v2(&data);
        assert_eq!(snapshot.beatmap_id, None);
        assert_eq!(snapshot.beatmapset_id, None);
        assert_eq!(snapshot.beatmap_md5, None);
    }
}
