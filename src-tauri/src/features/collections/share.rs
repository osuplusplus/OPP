use std::{
    collections::{BTreeSet, HashSet},
    io::{Read, Write},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use super::{
    CollectionEntry, CollectionFolder, CollectionSharePreview, CollectionSource,
    stable::{push_uleb, read_uleb},
};
use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::LocalClient,
    state::AppState,
};

const SHARE_PREFIX: &str = "OPPC2";
const LEGACY_SHARE_PREFIX: &str = "OPPC1";
const MAX_SHARE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub(super) struct SharePayload {
    pub(super) version: u8,
    pub(super) name: String,
    pub(super) creator: String,
    pub(super) created_at: String,
    pub(super) exported_at: String,
    pub(super) entries: Vec<CollectionEntry>,
}

fn push_share_string(value: &str, output: &mut Vec<u8>) {
    push_uleb(value.len(), output);
    output.extend_from_slice(value.as_bytes());
}

fn read_share_string(bytes: &[u8], offset: &mut usize) -> CommandResult<String> {
    let length = read_uleb(bytes, offset)?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| CommandError::new("INVALID_SHARE_CODE", "分享码字符串长度无效"))?;
    let value = std::str::from_utf8(
        bytes
            .get(*offset..end)
            .ok_or_else(|| CommandError::new("INVALID_SHARE_CODE", "分享码内容不完整"))?,
    )
    .map_err(|_| CommandError::new("INVALID_SHARE_CODE", "分享码文本无效"))?
    .to_string();
    *offset = end;
    Ok(value)
}

pub(super) fn encode_share(payload: &SharePayload) -> CommandResult<String> {
    // Online entries use only their stable IDs. This is the information needed
    // to identify an exact difficulty and cuts large share codes dramatically.
    let mut raw = vec![2_u8];
    push_share_string(&payload.name, &mut raw);
    push_share_string(&payload.creator, &mut raw);
    push_share_string(&payload.created_at, &mut raw);
    push_share_string(&payload.exported_at, &mut raw);
    push_uleb(payload.entries.len(), &mut raw);
    for entry in &payload.entries {
        if let (Some(beatmapset_id), Some(beatmap_id)) = (entry.beatmapset_id, entry.beatmap_id) {
            raw.push(0);
            push_uleb(beatmapset_id as usize, &mut raw);
            push_uleb(beatmap_id as usize, &mut raw);
            continue;
        }
        raw.push(1);
        let checksum = entry.checksum.as_deref().unwrap_or("");
        push_share_string(checksum, &mut raw);
        push_share_string(entry.ruleset.as_deref().unwrap_or(""), &mut raw);
        push_share_string(&entry.difficulty_name, &mut raw);
        push_share_string(&entry.title, &mut raw);
        push_share_string(&entry.artist, &mut raw);
        push_share_string(&entry.creator, &mut raw);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&raw)?;
    let encoded = encoder.finish()?;
    let checksum = Sha256::digest(&encoded);
    Ok(format!(
        "{SHARE_PREFIX}.{}.{}",
        URL_SAFE_NO_PAD.encode(encoded),
        URL_SAFE_NO_PAD.encode(checksum)
    ))
}

pub(super) fn decode_share(code: &str) -> CommandResult<SharePayload> {
    let parts = code.trim().split('.').collect::<Vec<_>>();
    if parts.len() != 3 || ![SHARE_PREFIX, LEGACY_SHARE_PREFIX].contains(&parts[0]) {
        return Err(CommandError::new(
            "INVALID_SHARE_CODE",
            "不是有效的 OPP 收藏夹分享码",
        ));
    }
    let encoded = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| CommandError::new("INVALID_SHARE_CODE", "分享码内容无法读取"))?;
    if encoded.len() > MAX_SHARE_BYTES {
        return Err(CommandError::new("INVALID_SHARE_CODE", "分享码过大"));
    }
    let expected = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| CommandError::new("INVALID_SHARE_CODE", "分享码校验值无效"))?;
    if expected.as_slice() != Sha256::digest(&encoded).as_slice() {
        return Err(CommandError::new(
            "INVALID_SHARE_CODE",
            "分享码校验失败，内容可能不完整",
        ));
    }
    let mut decoder = ZlibDecoder::new(encoded.as_slice());
    let mut raw = Vec::new();
    decoder
        .by_ref()
        .take((MAX_SHARE_BYTES + 1) as u64)
        .read_to_end(&mut raw)?;
    if raw.len() > MAX_SHARE_BYTES {
        return Err(CommandError::new("INVALID_SHARE_CODE", "分享码解压后过大"));
    }
    if parts[0] == LEGACY_SHARE_PREFIX {
        let payload: SharePayload = serde_json::from_slice(&raw)?;
        if payload.version != 1 {
            return Err(CommandError::new(
                "UNSUPPORTED_SHARE_CODE",
                "此分享码版本暂不支持",
            ));
        }
        return Ok(payload);
    }
    let mut offset = 0;
    if *raw
        .first()
        .ok_or_else(|| CommandError::new("INVALID_SHARE_CODE", "分享码内容为空"))?
        != 2
    {
        return Err(CommandError::new(
            "UNSUPPORTED_SHARE_CODE",
            "此分享码版本暂不支持",
        ));
    }
    offset += 1;
    let name = read_share_string(&raw, &mut offset)?;
    let creator = read_share_string(&raw, &mut offset)?;
    let created_at = read_share_string(&raw, &mut offset)?;
    let exported_at = read_share_string(&raw, &mut offset)?;
    let count = read_uleb(&raw, &mut offset)?;
    if count > 100_000 {
        return Err(CommandError::new(
            "INVALID_SHARE_CODE",
            "分享码谱面数量超出限制",
        ));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = *raw
            .get(offset)
            .ok_or_else(|| CommandError::new("INVALID_SHARE_CODE", "分享码内容不完整"))?;
        offset += 1;
        let entry = if kind == 0 {
            let beatmapset_id = i32::try_from(read_uleb(&raw, &mut offset)?)
                .map_err(|_| CommandError::new("INVALID_SHARE_CODE", "谱面集 ID 无效"))?;
            let beatmap_id = i32::try_from(read_uleb(&raw, &mut offset)?)
                .map_err(|_| CommandError::new("INVALID_SHARE_CODE", "谱面 ID 无效"))?;
            CollectionEntry {
                id: Uuid::new_v4().to_string(),
                beatmap_id: Some(beatmap_id),
                beatmapset_id: Some(beatmapset_id),
                checksum: None,
                ruleset: None,
                difficulty_name: format!("#{beatmap_id}"),
                title: format!("谱面集 #{beatmapset_id}"),
                artist: String::new(),
                creator: String::new(),
                resolved: false,
            }
        } else if kind == 1 {
            let checksum = read_share_string(&raw, &mut offset)?;
            let ruleset = read_share_string(&raw, &mut offset)?;
            CollectionEntry {
                id: Uuid::new_v4().to_string(),
                beatmap_id: None,
                beatmapset_id: None,
                checksum: (!checksum.is_empty()).then_some(checksum),
                ruleset: (!ruleset.is_empty()).then_some(ruleset),
                difficulty_name: read_share_string(&raw, &mut offset)?,
                title: read_share_string(&raw, &mut offset)?,
                artist: read_share_string(&raw, &mut offset)?,
                creator: read_share_string(&raw, &mut offset)?,
                resolved: false,
            }
        } else {
            return Err(CommandError::new(
                "INVALID_SHARE_CODE",
                "分享码条目类型无效",
            ));
        };
        entries.push(entry);
    }
    Ok(SharePayload {
        version: 1,
        name,
        creator,
        created_at,
        exported_at,
        entries,
    })
}

fn preview_payload(payload: SharePayload, state: &AppState) -> CollectionSharePreview {
    let checksums = payload
        .entries
        .iter()
        .filter_map(|entry| {
            entry
                .checksum
                .as_ref()
                .map(|value| value.to_ascii_lowercase())
        })
        .collect::<BTreeSet<_>>();
    let resolved = [LocalClient::Stable, LocalClient::Lazer]
        .into_iter()
        .filter_map(|client| {
            state
                .local_analysis
                .find_beatmaps_by_md5(client, &checksums)
                .ok()
        })
        .flat_map(|found| found.into_keys())
        .collect::<HashSet<_>>();
    let entries = payload
        .entries
        .into_iter()
        .map(|mut entry| {
            entry.resolved = entry
                .checksum
                .as_deref()
                .is_some_and(|checksum| resolved.contains(&checksum.to_ascii_lowercase()));
            entry
        })
        .collect::<Vec<_>>();
    let available_count = entries.iter().filter(|entry| entry.resolved).count();
    let downloadable_count = entries
        .iter()
        .filter(|entry| !entry.resolved && entry.beatmapset_id.is_some())
        .count();
    let unresolved_count = entries
        .len()
        .saturating_sub(available_count + downloadable_count);
    CollectionSharePreview {
        name: payload.name,
        creator: payload.creator,
        created_at: payload.created_at,
        exported_at: payload.exported_at,
        entries,
        available_count,
        downloadable_count,
        unresolved_count,
    }
}

#[tauri::command(async)]
pub fn export_collection_share(
    folder_id: String,
    creator: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let file = state
        .collections
        .value
        .lock()
        .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?;
    let folder = file
        .folders
        .iter()
        .find(|folder| folder.id == folder_id)
        .ok_or_else(|| CommandError::new("COLLECTION_NOT_FOUND", "未找到收藏夹"))?;
    encode_share(&SharePayload {
        version: 1,
        name: folder.name.clone(),
        creator: if creator.trim().is_empty() {
            folder.creator.clone()
        } else {
            creator.trim().into()
        },
        created_at: folder.created_at.clone(),
        exported_at: Utc::now().to_rfc3339(),
        entries: folder.entries.clone(),
    })
}

#[tauri::command(async)]
pub fn preview_collection_share(
    code: String,
    state: State<'_, AppState>,
) -> CommandResult<CollectionSharePreview> {
    Ok(preview_payload(decode_share(&code)?, &state))
}

#[tauri::command(async)]
pub fn import_collection_share(
    code: String,
    state: State<'_, AppState>,
) -> CommandResult<CollectionFolder> {
    let payload = decode_share(&code)?;
    state.collections.update(|file| {
        let now = Utc::now().to_rfc3339();
        let folder = CollectionFolder {
            id: Uuid::new_v4().to_string(),
            name: payload.name,
            creator: payload.creator,
            created_at: payload.created_at,
            updated_at: now,
            source: CollectionSource::Opp,
            read_only: false,
            pending_write: true,
            entries: payload.entries,
            external_id: None,
            external_fingerprint: None,
            last_read_at: None,
            backup_path: None,
            backup_fingerprint: None,
            backup_confirmed_at: None,
        };
        file.folders.push(folder.clone());
        Ok(folder)
    })
}
