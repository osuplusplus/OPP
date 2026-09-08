use std::{fs, future::Future, path::Path};

use url::Url;

use crate::{
    error::{CommandError, CommandResult},
    features::online_beatmaps::providers::{ProviderBytes, ProviderRegistry},
};

const MAX_OSU_FILE_BYTES: u64 = 16 * 1024 * 1024;

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

pub async fn fetch_online_osu(
    providers: &ProviderRegistry,
    beatmap_id: u64,
) -> CommandResult<Vec<u8>> {
    let download = catboy_then_nerinyan(
        || providers.catboy_osu(beatmap_id),
        || providers.nerinyan_osu(beatmap_id),
    )
    .await
    .map_err(|_| {
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
    Ok(download.bytes)
}

async fn catboy_then_nerinyan<Primary, PrimaryFuture, Fallback, FallbackFuture>(
    primary: Primary,
    fallback: Fallback,
) -> CommandResult<ProviderBytes>
where
    Primary: FnOnce() -> PrimaryFuture,
    PrimaryFuture: Future<Output = CommandResult<ProviderBytes>>,
    Fallback: FnOnce() -> FallbackFuture,
    FallbackFuture: Future<Output = CommandResult<ProviderBytes>>,
{
    match primary().await {
        Ok(download) => Ok(download),
        Err(_) => fallback().await,
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, fs::File};

    use super::*;

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

    #[test]
    fn rejects_foreign_or_set_only_urls() {
        assert!(parse_beatmap_id("https://example.com/b/1").is_err());
        assert!(parse_beatmap_id("https://osu.ppy.sh/beatmapsets/10").is_err());
    }

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

    #[tokio::test]
    async fn falls_back_from_catboy_to_nerinyan_in_memory() {
        let fallback_called = Cell::new(false);
        let download = catboy_then_nerinyan(
            || async {
                Err(CommandError::new(
                    "CATBOY_OSU_DOWNLOAD_FAILED",
                    "synthetic failure",
                ))
            },
            || async {
                fallback_called.set(true);
                Ok(ProviderBytes {
                    bytes: b"osu file format v14".to_vec(),
                    suggested_filename: Some("synthetic.osu".into()),
                    source: "nerinyan".into(),
                })
            },
        )
        .await
        .expect("fallback result");

        assert!(fallback_called.get());
        assert_eq!(download.source, "nerinyan");
        assert_eq!(download.bytes, b"osu file format v14");
    }
}
