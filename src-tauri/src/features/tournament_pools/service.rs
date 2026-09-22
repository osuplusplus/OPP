use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use futures_util::{StreamExt, stream};
use serde_json::Value;

use super::{models::*, rino};
use crate::{
    error::{CommandError, CommandResult},
    features::collections::CollectionCandidate,
    state::AppState,
};

pub(super) fn decode_beatmap(id: i32, value: Value) -> CommandResult<TournamentBeatmap> {
    let invalid = || {
        CommandError::new(
            "TOURNAMENT_BEATMAP_UNAVAILABLE",
            "无法解析 osu!standard 谱面资料",
        )
    };
    if value["id"].as_i64() != Some(i64::from(id)) || value["mode"].as_str() != Some("osu") {
        return Err(invalid());
    }
    let set = &value["beatmapset"];
    let set_id = value["beatmapset_id"]
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .filter(|n| *n > 0)
        .ok_or_else(invalid)?;
    if set["id"].as_i64() != Some(i64::from(set_id)) {
        return Err(invalid());
    }
    let text = |value: &Value| value.as_str().map(str::to_owned).ok_or_else(invalid);
    Ok(TournamentBeatmap {
        beatmapset_id: set_id,
        title: text(&set["title"])?,
        artist: text(&set["artist"])?,
        creator: text(&set["creator"])?,
        difficulty_name: text(&value["version"])?,
        checksum: value["checksum"].as_str().map(str::to_owned),
        download_disabled: set["availability"]["download_disabled"]
            .as_bool()
            .unwrap_or(false),
    })
}

pub(super) async fn load(
    reference: &TournamentPoolRef,
    state: &AppState,
) -> CommandResult<TournamentPool> {
    let pool = rino::fetch(reference).await?;
    if pool.entries.is_empty() {
        return Ok(pool);
    }
    let token = crate::features::account::ensure_access_token(state)
        .await
        .ok();
    Ok(enrich(pool, |id| {
        let token = token.as_deref();
        async move {
            if let Some(token) = token
                && let Ok(Ok(value)) = tokio::time::timeout(
                    Duration::from_secs(15),
                    state.api.get_beatmap(token, id as u64),
                )
                .await
                && let Ok(beatmap) = decode_beatmap(id, value)
            {
                return Ok(beatmap);
            }
            match tokio::time::timeout(
                Duration::from_secs(15),
                state.providers.nerinyan_beatmap(id as u64),
            )
            .await
            {
                Ok(result) => result.and_then(|value| decode_beatmap(id, value)),
                Err(_) => Err(CommandError::new(
                    "TOURNAMENT_BEATMAP_TIMEOUT",
                    "谱面资料查询超时，可刷新重试",
                )),
            }
        }
    })
    .await)
}

pub(super) async fn enrich<F, Fut>(mut pool: TournamentPool, resolve: F) -> TournamentPool
where
    F: Fn(i32) -> Fut,
    Fut: std::future::Future<Output = CommandResult<TournamentBeatmap>>,
{
    let ids: BTreeSet<_> = pool.entries.iter().map(|entry| entry.beatmap_id).collect();
    let resolved: HashMap<_, _> = stream::iter(ids)
        .map(|id| {
            let future = resolve(id);
            async move { (id, future.await) }
        })
        .buffer_unordered(6)
        .collect()
        .await;
    for entry in &mut pool.entries {
        match &resolved[&entry.beatmap_id] {
            Ok(beatmap) => entry.beatmap = Some(beatmap.clone()),
            Err(error) => entry.resolution_error = Some(error.message.clone()),
        }
    }
    pool
}

pub(super) fn candidates(pool: &TournamentPool) -> Vec<CollectionCandidate> {
    pool.entries
        .iter()
        .map(|entry| {
            let beatmap = entry.beatmap.as_ref();
            CollectionCandidate {
                beatmap_id: Some(entry.beatmap_id),
                beatmapset_id: beatmap.map(|map| map.beatmapset_id),
                checksum: beatmap.and_then(|map| map.checksum.clone()),
                ruleset: Some("osu".into()),
                difficulty_name: beatmap
                    .map(|map| map.difficulty_name.clone())
                    .unwrap_or_default(),
                title: beatmap
                    .map(|map| map.title.clone())
                    .unwrap_or_else(|| format!("Beatmap #{}", entry.beatmap_id)),
                artist: beatmap.map(|map| map.artist.clone()).unwrap_or_default(),
                creator: beatmap.map(|map| map.creator.clone()).unwrap_or_default(),
                local_client: None,
                local_resource_id: None,
            }
        })
        .collect()
}
