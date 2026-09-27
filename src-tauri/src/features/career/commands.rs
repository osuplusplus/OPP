use chrono::NaiveDate;
use serde_json::json;
use tauri::State;

use crate::{
    domain::{Ruleset, Score},
    error::{CommandError, CommandResult},
    features::{
        account::ensure_access_token,
        career::{models::*, service::scan_media},
    },
    infrastructure::logging::global,
    state::AppState,
};

#[tauri::command]
pub async fn capture_career_snapshot(
    ruleset: Ruleset,
    force_refresh: bool,
    state: State<'_, AppState>,
) -> CommandResult<CareerCaptureResult> {
    let _span = global().map(|logger| logger.operation("career", "capture_career_snapshot"));
    if !state.career.status()?.configured {
        return Err(CommandError::new(
            "CAREER_DATABASE_UNAVAILABLE",
            "请先配置本地数据库后启用生涯记录",
        ));
    }
    if !force_refresh {
        let snapshot = state.store.snapshot()?;
        if let Some(user_id) = snapshot.current_user_id
            && state.career.has_captured_today(user_id, ruleset)?
        {
            return Ok(CareerCaptureResult {
                local_date: crate::features::career::service::current_local_date(),
                status: "captured".into(),
                created: false,
                fetched_at: None,
                profile_available: true,
                scores_available: true,
                media_available: true,
                message: "今天的快照已经存在".into(),
            });
        }
    }
    let token = match ensure_access_token(&state).await {
        Ok(token) => token,
        Err(error) => {
            let snapshot = state.store.snapshot()?;
            let _ = state.career.record_failure(
                snapshot.current_user_id,
                snapshot.username.as_deref(),
                ruleset,
                &error.message,
            );
            return Err(error);
        }
    };
    let profile = match state.api.get_own_profile(&token, ruleset).await {
        Ok(profile) => profile,
        Err(error) => {
            let snapshot = state.store.snapshot()?;
            let _ = state.career.record_failure(
                snapshot.current_user_id,
                snapshot.username.as_deref(),
                ruleset,
                &error.message,
            );
            return Err(error);
        }
    };
    let scores = fetch_top_scores(&state, &token, profile.id, ruleset)
        .await
        .ok();
    let (media, media_ok) = scan_media(&state);
    let sections = json!({ "profile": true, "scores": scores.is_some(), "media": media_ok, "medals": profile.user_achievements.is_some() });
    state
        .career
        .capture(
            &profile,
            ruleset,
            scores.as_deref(),
            &media,
            &sections,
            force_refresh,
        )
        .map(|mut result| {
            result.scores_available = scores.is_some();
            result.media_available = media_ok;
            result
        })
}

async fn fetch_top_scores(
    state: &AppState,
    token: &str,
    user_id: u64,
    ruleset: Ruleset,
) -> CommandResult<Vec<Score>> {
    let first = state
        .api
        .get_user_scores(
            token,
            user_id,
            ruleset,
            crate::domain::ScoreCategory::Best,
            0,
            100,
        )
        .await?;
    let second = state
        .api
        .get_user_scores(
            token,
            user_id,
            ruleset,
            crate::domain::ScoreCategory::Best,
            100,
            100,
        )
        .await?;
    let mut scores = first;
    scores.extend(second);
    scores.truncate(200);
    Ok(scores)
}

#[tauri::command]
pub async fn get_career_calendar(
    ruleset: Ruleset,
    start_date: String,
    end_date: String,
    state: State<'_, AppState>,
) -> CommandResult<CareerCalendar> {
    let _span = global().map(|logger| logger.operation("career", "get_career_calendar"));
    validate_dates(&start_date, &end_date)?;
    let user_id = state
        .store
        .snapshot()?
        .current_user_id
        .ok_or_else(|| CommandError::new("PROFILE_REQUIRED", "请先登录 osu! 账号"))?;
    state
        .career
        .calendar(user_id, ruleset, &start_date, &end_date)
}

#[tauri::command]
pub async fn get_career_day(
    ruleset: Ruleset,
    date: String,
    state: State<'_, AppState>,
) -> CommandResult<CareerDayDetail> {
    let _span = global().map(|logger| logger.operation("career", "get_career_day"));
    NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| CommandError::new("INVALID_DATE", "日期格式必须是 YYYY-MM-DD"))?;
    let user_id = state
        .store
        .snapshot()?
        .current_user_id
        .ok_or_else(|| CommandError::new("PROFILE_REQUIRED", "请先登录 osu! 账号"))?;
    state.career.day(user_id, ruleset, &date)
}

#[tauri::command]
pub async fn get_career_status(state: State<'_, AppState>) -> CommandResult<CareerStatus> {
    let _span = global().map(|logger| logger.operation("career", "get_career_status"));
    state.career.status()
}

#[tauri::command]
pub async fn clear_career_history(
    before_date: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let _span = global().map(|logger| logger.operation("career", "clear_career_history"));
    if let Some(date) = before_date.as_deref() {
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| CommandError::new("INVALID_DATE", "日期格式必须是 YYYY-MM-DD"))?;
    }
    state.career.clear(before_date.as_deref())
}

fn validate_dates(start: &str, end: &str) -> CommandResult<()> {
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .map_err(|_| CommandError::new("INVALID_DATE", "日期格式必须是 YYYY-MM-DD"))?;
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d")
        .map_err(|_| CommandError::new("INVALID_DATE", "日期格式必须是 YYYY-MM-DD"))?;
    if start > end {
        return Err(CommandError::new(
            "INVALID_DATE_RANGE",
            "开始日期不能晚于结束日期",
        ));
    }
    Ok(())
}
