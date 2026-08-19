use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use super::models::{
    BeatmapDownloadItem, BeatmapDownloadProgress, DownloadProgressCounts, OnlineBeatmapSearchQuery,
};
use chrono::NaiveDate;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::{
    account::ensure_access_token,
    error::{CommandError, CommandResult},
    app::models::Ruleset,
    app::state::AppState,
};

pub const MAX_COLLECT_RESULTS: usize = 500;
pub const MAX_BATCH_ITEMS: usize = 500;
const GENRE_IDS: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14];
const LANGUAGE_IDS: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

impl OnlineBeatmapSearchQuery {
    fn to_api_parameters(&self) -> CommandResult<Vec<(String, String)>> {
        validate_date_range("ranked", &self.ranked_from, &self.ranked_to)?;
        validate_date_range("submitted", &self.submitted_from, &self.submitted_to)?;
        validate_date_range("updated", &self.updated_from, &self.updated_to)?;

        let mut filters = Vec::new();
        push_free_text(&mut filters, &self.query);
        push_text_filter(&mut filters, "artist", &self.artist);
        push_text_filter(&mut filters, "title", &self.title);
        push_text_filter(&mut filters, "source", &self.source);
        push_text_filter(&mut filters, "creator", &self.mapper);
        push_text_filter(&mut filters, "difficulty", &self.difficulty);
        for tag in self
            .tags
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
        {
            push_text_filter(&mut filters, "tag", tag);
        }

        push_date_range(&mut filters, "ranked", &self.ranked_from, &self.ranked_to);
        push_date_range(
            &mut filters,
            "submitted",
            &self.submitted_from,
            &self.submitted_to,
        );
        push_date_range(
            &mut filters,
            "updated",
            &self.updated_from,
            &self.updated_to,
        );

        push_number_range(
            &mut filters,
            "favourites",
            self.favourites_min,
            self.favourites_max,
        )?;
        push_number_range(&mut filters, "stars", self.stars_min, self.stars_max)?;
        push_number_range(&mut filters, "bpm", self.bpm_min, self.bpm_max)?;
        push_number_range(&mut filters, "length", self.length_min, self.length_max)?;
        push_number_range(&mut filters, "ar", self.ar_min, self.ar_max)?;
        push_number_range(&mut filters, "cs", self.cs_min, self.cs_max)?;
        push_number_range(&mut filters, "od", self.od_min, self.od_max)?;
        push_number_range(&mut filters, "hp", self.hp_min, self.hp_max)?;
        if self.ruleset == Some(Ruleset::Mania) {
            push_number_range(&mut filters, "keys", self.keys_min, self.keys_max)?;
        }

        let mut parameters = Vec::new();
        if !filters.is_empty() {
            parameters.push(("q".into(), filters.join(" ")));
        }
        if let Some(ruleset) = self.ruleset {
            parameters.push(("m".into(), ruleset_id(ruleset).to_string()));
        }

        let status = self.status.trim();
        if !status.is_empty() {
            const STATUSES: &[&str] = &[
                "any",
                "leaderboard",
                "ranked",
                "qualified",
                "loved",
                "favourites",
                "pending",
                "wip",
                "graveyard",
                "mine",
            ];
            if !STATUSES.contains(&status) {
                return Err(CommandError::new("INVALID_FILTER", "未知的谱面状态筛选"));
            }
            parameters.push(("s".into(), status.into()));
        }
        if let Some(genre) = self.genre {
            validate_category("genre", genre, GENRE_IDS)?;
            parameters.push(("g".into(), genre.to_string()));
        }
        if let Some(language) = self.language {
            validate_category("language", language, LANGUAGE_IDS)?;
            parameters.push(("l".into(), language.to_string()));
        }

        if !self.content_filter.trim().is_empty() {
            let allowed = [
                "recommended",
                "converts",
                "follows",
                "spotlights",
                "featured_artists",
            ];
            if !allowed.contains(&self.content_filter.trim()) {
                return Err(CommandError::new("INVALID_FILTER", "未知的内容筛选"));
            }
            parameters.push(("c".into(), self.content_filter.trim().into()));
        }
        if !self.grade.trim().is_empty() {
            parameters.push(("r".into(), self.grade.trim().into()));
        }
        if !self.played.trim().is_empty() {
            parameters.push(("played".into(), self.played.trim().into()));
        }

        let mut extras = BTreeSet::new();
        for extra in self.extras.iter().map(|extra| extra.trim()) {
            if extra.is_empty() {
                continue;
            }
            if !matches!(extra, "video" | "storyboard") {
                return Err(CommandError::new("INVALID_FILTER", "未知的附加内容筛选"));
            }
            extras.insert(extra);
        }
        if !extras.is_empty() {
            parameters.push(("e".into(), extras.into_iter().collect::<Vec<_>>().join(".")));
        }
        if self.include_nsfw {
            parameters.push(("nsfw".into(), "true".into()));
        }

        let sort = self.sort.trim();
        if !sort.is_empty() {
            const SORTS: &[&str] = &[
                "relevance_asc",
                "relevance_desc",
                "title_asc",
                "title_desc",
                "artist_asc",
                "artist_desc",
                "difficulty_asc",
                "difficulty_desc",
                "ranked_asc",
                "ranked_desc",
                "rating_asc",
                "rating_desc",
                "plays_asc",
                "plays_desc",
                "favourites_asc",
                "favourites_desc",
            ];
            if !SORTS.contains(&sort) {
                return Err(CommandError::new("INVALID_FILTER", "未知的排序方式"));
            }
            parameters.push(("sort".into(), sort.into()));
        }
        if let Some(cursor) = self
            .cursor_string
            .as_deref()
            .map(str::trim)
            .filter(|cursor| !cursor.is_empty())
        {
            parameters.push(("cursor_string".into(), cursor.into()));
        }
        Ok(parameters)
    }
}

pub fn normalize_official_response(value: &mut Value) {
    annotate_source(value, "official");
    if let Some(items) = value.get_mut("beatmapsets").and_then(Value::as_array_mut) {
        for item in items {
            annotate_source(item, "official");
        }
    }
}

pub fn annotate_source(value: &mut Value, source: &str) {
    if let Some(object) = value.as_object_mut() {
        object.insert("opp_source".into(), Value::String(source.into()));
        object.insert(
            "opp_fetched_at".into(),
            Value::String(chrono::Utc::now().to_rfc3339()),
        );
    }
}

pub async fn search_with_adapters(
    query: &OnlineBeatmapSearchQuery,
    state: &AppState,
) -> CommandResult<Value> {
    let access_token = ensure_access_token(state).await?;
    let mut value = state
        .api
        .search_beatmapsets(&access_token, &query.to_api_parameters()?)
        .await?;
    normalize_official_response(&mut value);
    Ok(value)
}

pub fn push_free_text(filters: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        filters.push(value.into());
    }
}

pub fn push_text_filter(filters: &mut Vec<String>, field: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        filters.push(format!("{field}={}", quote_filter_value(value)));
    }
}

pub fn push_date_range(filters: &mut Vec<String>, field: &str, from: &str, to: &str) {
    if !from.trim().is_empty() {
        filters.push(format!("{field}>={}", from.trim()));
    }
    if !to.trim().is_empty() {
        filters.push(format!("{field}<={}", to.trim()));
    }
}

pub fn push_number_range(
    filters: &mut Vec<String>,
    field: &str,
    min: Option<f64>,
    max: Option<f64>,
) -> CommandResult<()> {
    if min.is_some_and(|value| !value.is_finite() || value < 0.0)
        || max.is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        return Err(CommandError::new(
            "INVALID_FILTER",
            format!("{field} 的筛选值必须是非负数字"),
        ));
    }
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        return Err(CommandError::new(
            "INVALID_FILTER",
            format!("{field} 的最小值不能大于最大值"),
        ));
    }
    if let Some(min) = min {
        filters.push(format!("{field}>={}", compact_number(min)));
    }
    if let Some(max) = max {
        filters.push(format!("{field}<={}", compact_number(max)));
    }
    Ok(())
}

pub fn compact_number(value: f64) -> String {
    let value = format!("{value:.4}");
    value.trim_end_matches('0').trim_end_matches('.').into()
}

pub fn quote_filter_value(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn parse_date(field: &str, value: &str) -> CommandResult<Option<NaiveDate>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(Some)
        .map_err(|_| CommandError::new("INVALID_FILTER", format!("{field} 必须是 YYYY-MM-DD")))
}

pub fn validate_date_range(field: &str, from: &str, to: &str) -> CommandResult<()> {
    let from = parse_date(&format!("{field}_from"), from)?;
    let to = parse_date(&format!("{field}_to"), to)?;
    if let (Some(from), Some(to)) = (from, to)
        && from > to
    {
        return Err(CommandError::new(
            "INVALID_FILTER",
            format!("{field} 的起始日期不能晚于截止日期"),
        ));
    }
    Ok(())
}

pub fn validate_category(field: &str, value: u8, allowed: &[u8]) -> CommandResult<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(CommandError::new(
            "INVALID_FILTER",
            format!("未知的 {field} 筛选值"),
        ))
    }
}

pub fn ruleset_id(ruleset: Ruleset) -> u8 {
    match ruleset {
        Ruleset::Osu => 0,
        Ruleset::Taiko => 1,
        Ruleset::Fruits => 2,
        Ruleset::Mania => 3,
    }
}

pub fn prepare_destination(value: &str) -> CommandResult<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CommandError::new("INVALID_DESTINATION", "请选择下载目录"));
    }
    let destination = PathBuf::from(value);
    std::fs::create_dir_all(&destination)?;
    let destination = destination.canonicalize()?;
    if !destination.is_dir() {
        return Err(CommandError::new("INVALID_DESTINATION", "下载目标不是目录"));
    }
    Ok(destination)
}

pub fn find_existing_beatmapset(destination: &Path, beatmapset_id: u64) -> Option<PathBuf> {
    let prefix = format!("{beatmapset_id} ");
    let exact = format!("{beatmapset_id}.osz");
    std::fs::read_dir(destination)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            ((name == exact || name.starts_with(&prefix))
                && name.to_ascii_lowercase().ends_with(".osz"))
            .then(|| entry.path())
        })
}

pub fn sanitize_filename(value: &str) -> String {
    let mut sanitized = value
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                '_'
            } else {
                character
            }
        })
        .take(180)
        .collect::<String>();
    sanitized = sanitized.trim_matches([' ', '.']).to_string();
    if sanitized.is_empty() {
        sanitized = "beatmapset.osz".into();
    }
    if !sanitized.to_ascii_lowercase().ends_with(".osz") {
        sanitized.push_str(".osz");
    }
    sanitized
}

pub fn progress_for_item(
    phase: &str,
    counts: DownloadProgressCounts,
    item: &BeatmapDownloadItem,
    message: Option<String>,
) -> BeatmapDownloadProgress {
    BeatmapDownloadProgress {
        phase: phase.into(),
        total: counts.total,
        processed: counts.processed,
        completed: counts.completed,
        skipped: counts.skipped,
        failed: counts.failed,
        current_beatmapset_id: Some(item.beatmapset_id),
        current_title: Some(format!("{} — {}", item.artist, item.title)),
        message,
        downloaded_bytes: 0,
        total_bytes: None,
        bytes_per_second: 0.0,
        completed_paths: None,
        destination: None,
    }
}

pub fn emit_progress(app: &AppHandle, progress: BeatmapDownloadProgress) {
    let _ = app.emit("beatmap-download-progress", progress);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> OnlineBeatmapSearchQuery {
        OnlineBeatmapSearchQuery {
            query: "j-pop".into(),
            ruleset: Some(Ruleset::Mania),
            status: "ranked".into(),
            sort: "ranked_desc".into(),
            mapper: "Mapper Name".into(),
            ranked_from: "2025-01-01".into(),
            ranked_to: "2025-12-31".into(),
            stars_min: Some(4.25),
            stars_max: Some(6.5),
            keys_min: Some(4.0),
            keys_max: Some(4.0),
            ..Default::default()
        }
    }

    #[test]
    fn builds_official_search_syntax_for_priority_filters() {
        let parameters = query().to_api_parameters().expect("parameters");
        let q = parameters
            .iter()
            .find(|(key, _)| key == "q")
            .map(|(_, value)| value.as_str())
            .expect("q");
        assert!(q.contains("creator=\"Mapper Name\""));
        assert!(q.contains("ranked>=2025-01-01"));
        assert!(q.contains("ranked<=2025-12-31"));
        assert!(q.contains("stars>=4.25"));
        assert!(q.contains("stars<=6.5"));
        assert!(q.contains("keys>=4"));
        assert!(q.contains("keys<=4"));
        assert!(parameters.contains(&("m".into(), "3".into())));
        assert!(parameters.contains(&("s".into(), "ranked".into())));
    }

    #[test]
    fn forwards_relevance_sort_to_the_official_search_api() {
        let mut query = query();
        query.sort = "relevance_desc".into();

        assert!(
            query
                .to_api_parameters()
                .expect("parameters")
                .contains(&("sort".into(), "relevance_desc".into()))
        );
    }

    #[test]
    fn forwards_any_status_to_include_graveyard_results() {
        let mut query = query();
        query.status = "any".into();

        assert!(
            query
                .to_api_parameters()
                .expect("parameters")
                .contains(&("s".into(), "any".into()))
        );
    }

    #[test]
    fn rejects_inverted_numeric_ranges_and_invalid_dates() {
        let mut invalid_range = query();
        invalid_range.stars_min = Some(7.0);
        invalid_range.stars_max = Some(4.0);
        assert!(invalid_range.to_api_parameters().is_err());

        let mut invalid_date = query();
        invalid_date.ranked_from = "2025/01/01".into();
        assert!(invalid_date.to_api_parameters().is_err());

        let mut inverted_dates = query();
        inverted_dates.ranked_from = "2025-12-31".into();
        inverted_dates.ranked_to = "2025-01-01".into();
        assert!(inverted_dates.to_api_parameters().is_err());
    }

    #[test]
    fn rejects_unknown_api_filter_values() {
        let mut invalid_genre = query();
        invalid_genre.genre = Some(8);
        assert!(invalid_genre.to_api_parameters().is_err());

        let mut invalid_extra = query();
        invalid_extra.extras = vec!["background".into()];
        assert!(invalid_extra.to_api_parameters().is_err());
    }

    #[test]
    fn content_filters_use_official_parameters() {
        let mut query = query();
        query.content_filter = "spotlights".into();
        assert!(
            query
                .to_api_parameters()
                .unwrap()
                .contains(&("c".into(), "spotlights".into()))
        );
    }
}
