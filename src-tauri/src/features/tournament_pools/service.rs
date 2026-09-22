use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use futures_util::{StreamExt, stream};
use serde_json::Value;

use super::{models::*, rino, standard};
use crate::{
    error::{CommandError, CommandResult},
    features::collections::{
        CollectionCandidate,
        notebook::{PoolSlot, PoolSnapshot},
    },
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
        metadata: value,
    })
}

pub(super) async fn load(
    reference: &TournamentPoolRef,
    state: &AppState,
    progress: impl Fn(&str),
) -> CommandResult<TournamentPool> {
    reference.validate()?;
    progress("fetching");
    let pool = if reference.provider == "opp" {
        standard::fetch(reference).await?
    } else {
        rino::fetch(reference).await?
    };
    progress("enriching");
    if pool.entries.is_empty() {
        return Ok(pool);
    }
    let token = crate::features::account::ensure_access_token(state)
        .await
        .ok();
    Ok(enrich(pool, |id| {
        let token = token.as_deref();
        async move { resolve_beatmap(id, state, token).await }
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

pub(super) async fn repair_metadata(
    folder_id: &str,
    state: &AppState,
) -> CommandResult<(usize, usize)> {
    let folder = state.collections.folder(folder_id)?;
    let pool = state.collections.pool_snapshot(folder_id)?;
    let ids: BTreeSet<_> = pool
        .as_ref()
        .into_iter()
        .flat_map(|pool| &pool.slots)
        .filter(|slot| {
            slot.metadata.is_none()
                && folder
                    .entries
                    .iter()
                    .any(|e| e.beatmap_id == Some(slot.beatmap_id))
        })
        .map(|slot| slot.beatmap_id)
        .collect();
    if ids.is_empty() {
        return Ok((0, 0));
    }
    let total = ids.len();
    let token = crate::features::account::ensure_access_token(state)
        .await
        .ok();
    let maps: HashMap<_, _> = stream::iter(ids)
        .map(|id| {
            let token = token.as_deref();
            async move {
                resolve_beatmap(id, state, token)
                    .await
                    .ok()
                    .map(|map| (id, map.metadata))
            }
        })
        .buffer_unordered(6)
        .filter_map(|map| async { map })
        .collect()
        .await;
    let missing = total - maps.len();
    Ok((
        state.collections.save_pool_metadata(folder_id, &maps)?,
        missing,
    ))
}

pub(super) fn save(
    pool: TournamentPool,
    state: &AppState,
) -> CommandResult<TournamentPoolSyncResult> {
    let snapshot = PoolSnapshot {
        reference: pool.reference.clone(),
        title: pool.title.clone(),
        info: pool.info.clone(),
        slots: pool
            .entries
            .iter()
            .map(|entry| PoolSlot {
                metadata: entry.beatmap.as_ref().map(|b| b.metadata.clone()),
                beatmap_id: entry.beatmap_id,
                label: format!("{}{}", entry.selection_type, entry.position),
                selected_by: entry
                    .selected_by_name
                    .clone()
                    .or_else(|| entry.selected_by.clone())
                    .unwrap_or_default(),
                download_disabled: entry.beatmap.as_ref().is_some_and(|b| b.download_disabled),
                comment: entry.comment.clone(),
                is_custom: entry.is_custom,
                is_original: entry.is_original,
            })
            .collect(),
    };
    let folder = state.collections.replace_tournament_pool(
        &pool.reference.source_id(),
        &pool.title,
        candidates(&pool),
        Some(snapshot),
    )?;
    Ok(TournamentPoolSyncResult {
        folder_id: folder.id,
        entry_count: folder.entries.len(),
        pool,
    })
}

pub(super) async fn open_or_import<F, Fut>(
    reference: &TournamentPoolRef,
    state: &AppState,
    load: F,
) -> CommandResult<PoolOpenResult>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = CommandResult<TournamentPool>>,
{
    reference.validate()?;
    if let Some(folder) = state.collections.linked_pool(&reference.source_id())? {
        return Ok(PoolOpenResult {
            folder_id: folder.id,
            existing: true,
        });
    }
    let saved = save(load().await?, state)?;
    Ok(PoolOpenResult {
        folder_id: saved.folder_id,
        existing: false,
    })
}

async fn resolve_beatmap(
    id: i32,
    state: &AppState,
    token: Option<&str>,
) -> CommandResult<TournamentBeatmap> {
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
