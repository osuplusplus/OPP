//! 查询目标来源的解析与获取：ID / 链接解析、本地 `.osu` 读取与在线兜底下载。

use std::{fs, path::Path};

use url::Url;

use crate::{
    error::{CommandError, CommandResult},
    features::online_beatmaps::providers::ProviderRegistry,
};

/// 单个 `.osu` 文件的大小上限（16 MiB），防止异常大文件拖垮即时分析。
const MAX_OSU_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// 解析用户输入的 Beatmap ID：纯数字，或 osu.ppy.sh 链接（支持
/// `/beatmaps/{id}`、`/b/{id}` 以及谱面集页的 `#osu/{id}` fragment）。
pub fn parse_beatmap_id(input: &str) -> CommandResult<u64> {
    let value = input.trim();
    if let Ok(id) = value.parse::<u64>()
        && id > 0
    {
        return Ok(id);
    }
    let url = Url::parse(value).map_err(|_| {
        CommandError::new("INVALID_BEATMAP_ID", "请输入有效的 Beatmap ID 或 osu! 链接")
    })?;
    if !matches!(url.host_str(), Some("osu.ppy.sh" | "www.osu.ppy.sh")) {
        return Err(CommandError::new(
            "INVALID_BEATMAP_ID",
            "仅支持 osu.ppy.sh 的谱面链接",
        ));
    }
    if let Some(fragment) = url.fragment()
        && let Some(id) = fragment
            .split('/')
            .next_back()
            .and_then(|value| value.parse::<u64>().ok())
        && id > 0
    {
        return Ok(id);
    }
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    for prefix in ["beatmaps", "b"] {
        if let Some(index) = segments.iter().position(|segment| *segment == prefix)
            && let Some(id) = segments
                .get(index + 1)
                .and_then(|value| value.parse::<u64>().ok())
            && id > 0
        {
            return Ok(id);
        }
    }
    Err(CommandError::new(
        "INVALID_BEATMAP_ID",
        "链接中没有找到 Beatmap ID",
    ))
}

/// 读取本地 `.osu` 文件：校验扩展名、必须是普通文件且不超过 16 MiB。
pub fn read_local_osu(path: &str) -> CommandResult<Vec<u8>> {
    let path = Path::new(path);
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("osu"))
    {
        return Err(CommandError::new(
            "INVALID_BEATMAP_FILE",
            "请选择一个 .osu 谱面文件",
        ));
    }
    let metadata = fs::metadata(path)
        .map_err(|_| CommandError::new("BEATMAP_FILE_MISSING", "选择的谱面文件已不可用"))?;
    if !metadata.is_file() {
        return Err(CommandError::new(
            "INVALID_BEATMAP_FILE",
            "选择的路径不是谱面文件",
        ));
    }
    if metadata.len() > MAX_OSU_FILE_BYTES {
        return Err(CommandError::new(
            "BEATMAP_FILE_TOO_LARGE",
            "单个 .osu 文件不能超过 16 MiB",
        ));
    }
    fs::read(path).map_err(|_| CommandError::new("BEATMAP_READ_FAILED", "无法读取选择的谱面文件"))
}

/// 索引未命中时的在线兜底：从 osu! 官方源下载 `.osu` 并复检大小与内容。
pub async fn fetch_online_osu(
    providers: &ProviderRegistry,
    beatmap_id: u64,
) -> CommandResult<Vec<u8>> {
    let download = providers.official_osu(beatmap_id).await.map_err(|_| {
        CommandError::new(
            "BEATMAP_SOURCE_UNAVAILABLE",
            "目标谱面不在本地索引中，在线获取也失败了，请改用本地 .osu 文件",
        )
    })?;
    if download.bytes.len() as u64 > MAX_OSU_FILE_BYTES {
        return Err(CommandError::new(
            "BEATMAP_FILE_TOO_LARGE",
            "在线谱面文件超过 16 MiB，无法即时分析",
        ));
    }
    // 官方接口可能返回非谱面内容，必须校验 `.osu` 文件头。
    if !download.bytes.starts_with(b"osu file format v") {
        return Err(CommandError::new(
            "INVALID_BEATMAP_FILE",
            "osu! 官方返回的内容不是有效的 .osu 谱面文件",
        ));
    }
    Ok(download.bytes)
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    // 数字 ID 与三种受支持的链接格式都能解析。
    #[test]
    fn parses_supported_ids_and_urls() {
        assert_eq!(parse_beatmap_id("123").expect("numeric id"), 123);
        assert_eq!(
            parse_beatmap_id("https://osu.ppy.sh/beatmaps/456").expect("beatmaps url"),
            456
        );
        assert_eq!(
            parse_beatmap_id("https://osu.ppy.sh/b/789").expect("legacy url"),
            789
        );
        assert_eq!(
            parse_beatmap_id("https://osu.ppy.sh/beatmapsets/10#osu/321").expect("beatmapset url"),
            321
        );
    }

    // 拒绝外域链接与不含具体难度的谱面集链接。
    #[test]
    fn rejects_foreign_or_set_only_urls() {
        assert!(parse_beatmap_id("https://example.com/b/1").is_err());
        assert!(parse_beatmap_id("https://osu.ppy.sh/beatmapsets/10").is_err());
    }

    // 拒绝非 .osu 扩展名与超过大小上限的本地文件。
    #[test]
    fn rejects_non_osu_and_oversized_local_files() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let text_file = directory.path().join("reference.txt");
        fs::write(&text_file, b"osu file format v14").expect("text file");
        assert!(read_local_osu(text_file.to_str().expect("text path")).is_err());

        let oversized = directory.path().join("oversized.osu");
        File::create(&oversized)
            .expect("large file")
            .set_len(MAX_OSU_FILE_BYTES + 1)
            .expect("extend file");
        let error = read_local_osu(oversized.to_str().expect("osu path"))
            .expect_err("oversized file must be rejected");
        assert_eq!(error.code, "BEATMAP_FILE_TOO_LARGE");
    }
}
