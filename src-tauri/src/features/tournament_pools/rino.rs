use std::{collections::HashSet, time::Duration};

use futures_util::StreamExt;
use serde::Deserialize;

use super::models::{TournamentPool, TournamentPoolEntry, TournamentPoolRef};
use crate::{
    error::{CommandError, CommandResult},
    infrastructure::logging::{finish_span, global},
};

const ENDPOINT: &str = "https://rino.ink/api/map-selections";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct Response {
    success: bool,
    data: Vec<Selection>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    beatmap_id: i32,
    selected_mods: String,
    mod_position: u32,
    season: String,
    category: String,
    approved: bool,
    padding: bool,
    selected_by: Option<String>,
    selected_by_username: Option<String>,
    comment: Option<String>,
    is_custome: bool,
    is_origin: bool,
}

pub(super) fn decode(bytes: &[u8], reference: &TournamentPoolRef) -> CommandResult<TournamentPool> {
    reference.validate()?;
    let response: Response = serde_json::from_slice(bytes)
        .map_err(|e| CommandError::from_error("INVALID_TOURNAMENT_RESPONSE", e))?;
    if !response.success || response.data.len() > 500 {
        return Err(CommandError::new(
            "INVALID_TOURNAMENT_RESPONSE",
            "比赛接口返回失败或图池过大",
        ));
    }
    let mut slots = HashSet::new();
    let mut entries = Vec::new();
    for item in response.data {
        if item.beatmap_id <= 0
            || item.mod_position == 0
            || item.selected_mods.trim().is_empty()
            || item.selected_mods.len() > 32
            || !item.approved
            || !item.padding
            || item.season != reference.season
            || item.category != reference.category
            || !slots.insert((item.selected_mods.clone(), item.mod_position))
        {
            return Err(CommandError::new(
                "INVALID_TOURNAMENT_RESPONSE",
                "图池包含无效谱面、重复图位或不匹配的阶段",
            ));
        }
        entries.push(TournamentPoolEntry {
            beatmap_id: item.beatmap_id,
            selection_type: item.selected_mods,
            position: item.mod_position,
            selected_by: item.selected_by,
            selected_by_name: item.selected_by_username,
            comment: item.comment.unwrap_or_default(),
            is_custom: item.is_custome,
            is_original: item.is_origin,
            beatmap: None,
            resolution_error: None,
        });
    }
    let rank = |label: &str| {
        ["NM", "HD", "HR", "DT", "FM", "LZ", "TB"]
            .iter()
            .position(|item| *item == label)
            .unwrap_or(7)
    };
    entries.sort_by(|a, b| {
        (rank(&a.selection_type), &a.selection_type, a.position).cmp(&(
            rank(&b.selection_type),
            &b.selection_type,
            b.position,
        ))
    });
    Ok(TournamentPool {
        reference: reference.clone(),
        title: reference.title(),
        entries,
    })
}

pub(super) async fn fetch(reference: &TournamentPoolRef) -> CommandResult<TournamentPool> {
    let span = global().map(|logger| logger.operation("tournament_pools", "fetch_rino"));
    let result = async {
        reference.validate()?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("OPP/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| CommandError::from_error("TOURNAMENT_NETWORK_ERROR", e))?;
        let response = client
            .get(ENDPOINT)
            .query(&[
                ("season", reference.season.as_str()),
                ("category", reference.category.as_str()),
                ("approved", "true"),
                ("padding", "true"),
            ])
            .send()
            .await;
        if let Some(span) = &span {
            span.http_request(
                "GET",
                ENDPOINT,
                response.as_ref().ok().map(|r| r.status().as_u16()),
            );
        }
        let response =
            response.map_err(|e| CommandError::from_error("TOURNAMENT_NETWORK_ERROR", e))?;
        if !response.status().is_success() {
            return Err(CommandError::new(
                "TOURNAMENT_NETWORK_ERROR",
                format!("比赛接口请求失败：{}", response.status()),
            ));
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|e| CommandError::from_error("TOURNAMENT_NETWORK_ERROR", e))?;
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(CommandError::new(
                    "INVALID_TOURNAMENT_RESPONSE",
                    "比赛接口响应过大",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        decode(&bytes, reference)
    }
    .await;
    finish_span(span, result)
}
