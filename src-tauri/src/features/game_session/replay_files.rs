use std::{fs, io::Read, path::Path};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::{media_path_for_ui, models::GameMediaItem, parse_replay_metadata};
use crate::{
    domain::Ruleset,
    error::{CommandError, CommandResult},
    features::local_analysis::LocalClient,
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

const MAX_REPLAY_BYTES: u64 = 32 * 1024 * 1024;

// Check the complete fixed header and declared replay payload without decoding
// LZMA. Rendering performs the full decode; selection must not accept a truncated
// file that happens to contain a valid map hash and player name.
fn validate_header(bytes: &[u8]) -> CommandResult<()> {
    fn string_end(bytes: &[u8], cursor: &mut usize) -> Option<()> {
        let marker = *bytes.get(*cursor)?;
        *cursor += 1;
        if marker == 0 {
            return Some(());
        }
        if marker != 0x0b {
            return None;
        }
        let mut length = 0usize;
        for shift in (0..usize::BITS).step_by(7) {
            let byte = *bytes.get(*cursor)?;
            *cursor += 1;
            let part = usize::from(byte & 0x7f);
            if part > (usize::MAX >> shift) {
                return None;
            }
            length |= part << shift;
            if byte & 0x80 == 0 {
                *cursor = cursor.checked_add(length)?;
                return (*cursor <= bytes.len()).then_some(());
            }
        }
        None
    }
    let valid = (|| -> Option<()> {
        let mut cursor = 5;
        for _ in 0..3 {
            string_end(bytes, &mut cursor)?;
        }
        cursor = cursor.checked_add(23)?; // hit counts, score, combo, perfect, mods
        string_end(bytes, &mut cursor)?; // life bar graph
        cursor = cursor.checked_add(8)?; // timestamp
        let end = cursor.checked_add(4)?;
        let length = i32::from_le_bytes(bytes.get(cursor..end)?.try_into().ok()?);
        if length <= 0 {
            return None;
        }
        bytes.get(end..end.checked_add(length as usize)?)?;
        Some(())
    })();
    valid.ok_or_else(|| {
        CommandError::new(
            "REPLAY_PARSE_FAILED",
            "回放头结构不完整，或缺少回放输入数据",
        )
    })
}

#[derive(Serialize)]
pub struct ReplayFileFailure {
    path: String,
    error: CommandError,
}

#[derive(Default, Serialize)]
pub struct ReplayFileSelection {
    items: Vec<GameMediaItem>,
    failures: Vec<ReplayFileFailure>,
}

pub(super) fn replay_ruleset(bytes: &[u8]) -> CommandResult<Ruleset> {
    match bytes.first() {
        Some(0) => Ok(Ruleset::Osu),
        Some(1) => Ok(Ruleset::Taiko),
        Some(2) => Ok(Ruleset::Fruits),
        Some(3) => Ok(Ruleset::Mania),
        _ => Err(CommandError::new("REPLAY_PARSE_FAILED", "回放模式无效")),
    }
}

pub(crate) fn read_replay(path: &Path) -> CommandResult<Vec<u8>> {
    let span = global().map(|logger| logger.operation("game_session", "read_replay"));
    let result = (|| {
        if !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("osr"))
        {
            return Err(CommandError::new(
                "REPLAY_EXTENSION_INVALID",
                "请选择 .osr 回放文件",
            ));
        }
        if !fs::metadata(path)?.is_file() {
            return Err(CommandError::new("REPLAY_NOT_FILE", "所选路径不是普通文件"));
        }
        let file = fs::File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(CommandError::new("REPLAY_NOT_FILE", "所选路径不是普通文件"));
        }
        if metadata.len() > MAX_REPLAY_BYTES {
            return Err(CommandError::new("REPLAY_TOO_LARGE", "回放文件超过 32 MB"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_REPLAY_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_REPLAY_BYTES {
            return Err(CommandError::new("REPLAY_TOO_LARGE", "回放文件超过 32 MB"));
        }
        replay_ruleset(&bytes)?;
        validate_header(&bytes)?;
        let (hash, _) = parse_replay_metadata(&bytes)?;
        if hash.len() != 32 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CommandError::new(
                "REPLAY_PARSE_FAILED",
                "回放谱面校验值无效",
            ));
        }
        Ok(bytes)
    })();
    if let Some(ref span) = span {
        span.fs_op("read", path, &result);
    }
    finish_span(span, result)
}

#[tauri::command(async)]
pub fn choose_game_replay_files(
    client: LocalClient,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<ReplayFileSelection> {
    let span = global().map(|logger| logger.operation("game_session", "choose_game_replay_files"));
    let result = (|| {
        let Some(files) = app
            .dialog()
            .file()
            .set_title("选择回放文件")
            .add_filter("osu! replay", &["osr"])
            .blocking_pick_files()
        else {
            return Ok(ReplayFileSelection::default());
        };
        let mut selection = ReplayFileSelection::default();
        for file in files {
            let display = file.to_string();
            let result = (|| -> CommandResult<GameMediaItem> {
                let path = file
                    .into_path()
                    .map_err(|e| CommandError::new("REPLAY_PATH_INVALID", e.to_string()))?
                    .canonicalize()?;
                let bytes = read_replay(&path)?;
                state
                    .game_session
                    .selected_replays
                    .lock()
                    .map_err(|_| CommandError::new("SESSION_LOCKED", "素材状态不可用"))?
                    .insert(path.clone());
                Ok(GameMediaItem {
                    client,
                    path: media_path_for_ui(&path),
                    kind: "replay".into(),
                    modified_at: None,
                    size: bytes.len() as u64,
                })
            })();
            match result {
                Ok(item)
                    if !selection
                        .items
                        .iter()
                        .any(|existing| existing.path == item.path) =>
                {
                    selection.items.push(item)
                }
                Ok(_) => {}
                Err(error) => selection.failures.push(ReplayFileFailure {
                    path: display,
                    error,
                }),
            }
        }
        Ok(selection)
    })();
    finish_span(span, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn replay_fixture() -> Vec<u8> {
        let mut bytes = vec![0, 0, 0, 0, 0, 0x0b, 32];
        bytes.extend_from_slice(b"0123456789abcdef0123456789abcdef");
        bytes.extend_from_slice(&[0x0b, 6]);
        bytes.extend_from_slice(b"player");
        bytes.push(0); // replay hash
        bytes.extend_from_slice(&[0; 23]);
        bytes.push(0); // life bar
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.push(0); // compressed data is checked by the renderer
        bytes
    }
    #[test]
    fn accepts_external_unicode_files_and_rejects_changed_or_deleted_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("自选 回放.OSR");
        let bytes = replay_fixture();
        fs::write(&path, &bytes).unwrap();
        assert_eq!(read_replay(&path).unwrap(), bytes);
        fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
        assert!(read_replay(&path).is_err());
        fs::remove_file(&path).unwrap();
        assert!(read_replay(&path).is_err());
    }
    #[test]
    fn rejects_truncated_headers_and_invalid_modes() {
        let bytes = replay_fixture();
        for end in 0..bytes.len() {
            assert!(validate_header(&bytes[..end]).is_err());
        }
        assert!(replay_ruleset(&[4]).is_err());
        assert!(matches!(replay_ruleset(&[3]), Ok(Ruleset::Mania)));
    }
    #[test]
    fn rejects_directories_wrong_extensions_and_corrupt_replays() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_replay(dir.path()).is_err());
        let bad = dir.path().join("损坏 回放.osr");
        fs::write(&bad, [0, 1, 2]).unwrap();
        assert!(read_replay(&bad).is_err());
        let large = dir.path().join("large.osr");
        fs::File::create(&large)
            .unwrap()
            .set_len(MAX_REPLAY_BYTES + 1)
            .unwrap();
        assert_eq!(read_replay(&large).unwrap_err().code, "REPLAY_TOO_LARGE");
    }
}
