//! 在线谱面背景图缓存：经 Sayobot 获取原图，落盘为 `.bin` + `.mime` 两个文件，
//! 以 base64 数据 URL 返回前端；查不到背景时写 `.missing` 负缓存避免反复请求。

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use crate::infrastructure::flights::Flights;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderValue, REFERER};
use serde_json::Value;
use tokio::sync::Semaphore;
use url::Url;

use crate::{
    error::{CommandError, CommandResult},
    infrastructure::logging::global,
};

// 背景图大小上限（20 MiB）：读取前后都做校验，防止大图拖垮内存与前端渲染。
const MAX_BACKGROUND_BYTES: usize = 20 * 1024 * 1024;
// 负缓存有效期：24 小时内不再请求已知没有背景的谱面。
const MISSING_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
// 背景文件名元数据与其静态原图均来自小夜镜像。
const SAYOBOT_INFO_URL: &str = "https://api.sayobot.cn/v2/beatmapinfo";
const SAYOBOT_FILES_URL: &str = "https://dl.sayobot.cn/beatmaps/files";

/// 谱面背景图缓存，由 `AppState` 全局持有一份。
pub struct OnlineArtworkCache {
    /// 缓存根目录（应用数据目录下的 `online-beatmap-artwork`）。
    directory: PathBuf,
    /// 独立 HTTP 客户端：20 秒超时，携带固定 UA 与 Referer。
    client: reqwest::Client,
    /// 远端请求许可（容量 1）：背景抓取全局串行，避免打满镜像。
    requests: Semaphore,
    /// 在途任务许可（容量 128）：限制同时处理的背景任务总数，超限快速失败。
    pending: Semaphore,
    /// 单飞合并：同一谱面集的并发请求只触发一次真实抓取。
    flights: Flights<u64, CommandResult<Option<String>>>,
}

impl OnlineArtworkCache {
    /// 创建缓存实例并确保缓存目录存在。
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let directory = app_data_dir.join("online-beatmap-artwork");
        fs::create_dir_all(&directory)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            REFERER,
            HeaderValue::from_static("https://github.com/osuplusplus/OPP"),
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .default_headers(headers)
            .user_agent(concat!(
                "OPP/",
                env!("CARGO_PKG_VERSION"),
                " (online beatmap artwork; https://github.com/osuplusplus/OPP)"
            ))
            .build()
            .map_err(|error| CommandError::network(error.to_string()))?;
        Ok(Self {
            directory,
            client,
            requests: Semaphore::new(1),
            pending: Semaphore::new(128),
            flights: Flights::default(),
        })
    }

    /// 读取或抓取背景图；同一 `beatmapset_id` 的并发调用会被合并为一次网络请求。
    pub async fn load_or_fetch(&self, beatmapset_id: u64) -> CommandResult<Option<String>> {
        self.flights
            .run(beatmapset_id, || async {
                self.load_once(beatmapset_id).await
            })
            .await
    }

    /// 主流程：命中 `.bin` 缓存直接返回数据 URL；命中最新的 `.missing` 负缓存返回
    /// `None`；否则取得全局串行许可后抓取远端，成功时写入图片与 MIME 并清除负缓存标记。
    async fn load_once(&self, beatmapset_id: u64) -> CommandResult<Option<String>> {
        // 在途任务达到上限时快速失败，而不是无界排队。
        let _pending = self
            .pending
            .try_acquire()
            .map_err(|_| CommandError::new("TASK_QUEUE_FULL", "背景加载任务过多，请稍后重试"))?;
        let mut span = global().map(|logger| {
            logger.operation("online_beatmaps", format!("background:{beatmapset_id}"))
        });
        let image_path = self
            .directory
            .join(format!("background-{beatmapset_id}.bin"));
        let mime_path = self
            .directory
            .join(format!("background-{beatmapset_id}.mime"));
        let missing_path = self
            .directory
            .join(format!("background-{beatmapset_id}.missing"));

        let cached_image = image_path.clone();
        let cached_mime = mime_path.clone();
        let missing = missing_path.clone();
        let (cached, absent) =
            crate::infrastructure::tasks::blocking_io("read_online_artwork", move || {
                let span = global()
                    .map(|logger| logger.operation("online_beatmaps", "read_artwork_cache"));
                (
                    Self::read_cached(&cached_image, &cached_mime, span.as_ref()),
                    missing_cache_is_fresh(&missing),
                )
            })
            .await?;
        if let Some(data_url) = cached {
            if let Some(ref mut current) = span {
                current.finish_ok(Some(serde_json::json!({ "cache": "hit" })));
            }
            return Ok(Some(data_url));
        }
        if absent {
            if let Some(ref mut current) = span {
                current.finish_ok(Some(serde_json::json!({ "cache": "missing" })));
            }
            return Ok(None);
        }

        // 缓存未命中才占用唯一的远端请求名额。
        let permit = self
            .requests
            .acquire()
            .await
            .map_err(|_| CommandError::new("ARTWORK_STOPPED", "背景加载已停止"))?;
        let result = self.fetch(beatmapset_id, span.as_ref()).await;
        drop(permit);
        match result {
            Ok(Some((bytes, mime))) => {
                let write_image = tokio::fs::write(&image_path, &bytes).await;
                if let Some(ref current) = span {
                    current.fs_op("write", &image_path, &write_image);
                }
                write_image?;
                let write_mime = tokio::fs::write(&mime_path, &mime).await;
                if let Some(ref current) = span {
                    current.fs_op("write", &mime_path, &write_mime);
                }
                write_mime?;
                let _ = tokio::fs::remove_file(&missing_path).await;
                let length = bytes.len();
                let value = Some(
                    crate::infrastructure::tasks::blocking_io("encode_online_artwork", move || {
                        to_data_url(&bytes, &mime)
                    })
                    .await?,
                );
                if let Some(ref mut current) = span {
                    current.finish_ok(Some(serde_json::json!({
                        "cache": "stored",
                        "bytes": length
                    })));
                }
                Ok(value)
            }
            Ok(None) => {
                let write_missing = tokio::fs::write(&missing_path, b"not-found").await;
                if let Some(ref current) = span {
                    current.fs_op("write", &missing_path, &write_missing);
                }
                write_missing?;
                if let Some(ref mut current) = span {
                    current.finish_ok(Some(serde_json::json!({ "cache": "missing-stored" })));
                }
                Ok(None)
            }
            Err(error) => {
                if let Some(ref mut current) = span {
                    current.finish_error(&error);
                }
                Err(error)
            }
        }
    }

    /// 读取缓存图片并转成数据 URL；超过大小上限或读取失败均视为未命中。
    fn read_cached(
        image_path: &Path,
        mime_path: &Path,
        span: Option<&crate::infrastructure::logging::LogSpan>,
    ) -> Option<String> {
        if fs::metadata(image_path).ok()?.len() > MAX_BACKGROUND_BYTES as u64 {
            return None;
        }
        let image = fs::read(image_path);
        if let Some(current) = span {
            current.fs_op("read", image_path, &image);
        }
        let bytes = image.ok()?;
        if bytes.is_empty() || bytes.len() > MAX_BACKGROUND_BYTES {
            return None;
        }
        // `.mime` 伴随文件缺失时用魔数嗅探兜底。
        let mime = fs::read_to_string(mime_path).unwrap_or_else(|_| sniff_mime(&bytes).into());
        Some(to_data_url(&bytes, &mime))
    }

    /// 从小夜抓取原图：先经 `v2/beatmapinfo` 取得背景文件名，再拼接静态文件地址下载。
    /// 元数据无背景或图片下载失败返回 `None`（记为未找到），网络异常则向上抛错。
    async fn fetch(
        &self,
        beatmapset_id: u64,
        span: Option<&crate::infrastructure::logging::LogSpan>,
    ) -> CommandResult<Option<(Vec<u8>, String)>> {
        let mut info_url = Url::parse(SAYOBOT_INFO_URL)
            .map_err(|error| CommandError::new("INVALID_URL", error.to_string()))?;
        info_url
            .query_pairs_mut()
            .append_pair("K", &beatmapset_id.to_string());
        let info_response = self
            .client
            .get(info_url.clone())
            .send()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?;
        if let Some(current) = span {
            current.http_request(
                "GET",
                info_url.as_str(),
                Some(info_response.status().as_u16()),
            );
        }
        if !info_response.status().is_success() {
            return Err(CommandError::network(format!(
                "小夜谱面信息请求失败（{}）",
                info_response.status()
            )));
        }
        let metadata = info_response
            .json::<Value>()
            .await
            .map_err(|error| CommandError::new("INVALID_DATA", error.to_string()))?;
        let Some(file_name) = background_filename(&metadata) else {
            return Ok(None);
        };
        let image_url = original_background_url(beatmapset_id, file_name)?;
        let mut response = self
            .client
            .get(image_url.clone())
            .send()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?;
        if let Some(current) = span {
            current.http_request("GET", image_url.as_str(), Some(response.status().as_u16()));
        }
        if !response.status().is_success() {
            return Ok(None);
        }
        // 先按 `Content-Length` 预检大小，流式读取时还会累计校验。
        if response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_BACKGROUND_BYTES)
        {
            return Err(CommandError::new(
                "BACKGROUND_TOO_LARGE",
                "在线谱面背景文件过大",
            ));
        }
        let mime = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(normalize_image_mime)
            .ok_or_else(|| CommandError::new("INVALID_BACKGROUND_DATA", "背景响应不是图片"))?
            .to_string();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_BACKGROUND_BYTES {
                return Err(CommandError::new(
                    "BACKGROUND_TOO_LARGE",
                    "在线谱面背景文件过大",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() || bytes.len() > MAX_BACKGROUND_BYTES {
            return Err(CommandError::new(
                "BACKGROUND_TOO_LARGE",
                "在线谱面背景文件为空或过大",
            ));
        }
        Ok(Some((bytes, mime)))
    }
}

/// 从小夜元数据中取第一个非空背景文件名；`status != 0` 表示查询失败，一律视为无背景。
fn background_filename(metadata: &Value) -> Option<&str> {
    if metadata.get("status").and_then(Value::as_i64) != Some(0) {
        return None;
    }
    metadata
        .get("data")?
        .get("bid_data")?
        .as_array()?
        .iter()
        .filter_map(|difficulty| difficulty.get("bg").and_then(Value::as_str))
        .find(|name| !name.trim().is_empty())
}

/// 拼接原图下载地址；`Url` 的路径段会自动做百分号编码以处理中文文件名。
fn original_background_url(beatmapset_id: u64, file_name: &str) -> CommandResult<Url> {
    let mut url = Url::parse(SAYOBOT_FILES_URL)
        .map_err(|error| CommandError::new("INVALID_URL", error.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| CommandError::new("INVALID_URL", "静态资源地址无效"))?
        .push(&beatmapset_id.to_string())
        .push(file_name);
    Ok(url)
}

/// 判断负缓存标记是否仍在有效期内（按文件修改时间计算）。
fn missing_cache_is_fresh(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < MISSING_CACHE_TTL)
}

/// 仅接受白名单内的图片 MIME，避免把错误页等非图片内容写入缓存。
fn normalize_image_mime(value: &str) -> Option<&str> {
    match value
        .split(';')
        .next()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/jpeg" => Some("image/jpeg"),
        "image/png" => Some("image/png"),
        "image/webp" => Some("image/webp"),
        "image/gif" => Some("image/gif"),
        _ => None,
    }
}

/// 按魔数嗅探图片类型，作为 `.mime` 伴随文件缺失时的兜底。
fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// 编码为 base64 数据 URL；MIME 不在白名单时退回魔数嗅探。
fn to_data_url(bytes: &[u8], mime: &str) -> String {
    let mime = normalize_image_mime(mime).unwrap_or_else(|| sniff_mime(bytes));
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

// 元数据解析、URL 编码与数据 URL 生成的单元测试。
#[cfg(test)]
mod tests {
    use super::{background_filename, original_background_url, to_data_url};

    #[test]
    fn finds_background_file_in_v2_metadata() {
        let value = serde_json::json!({
            "status": 0,
            "data": { "bid_data": [{ "bg": "背景 image.jpg" }] }
        });
        assert_eq!(background_filename(&value), Some("背景 image.jpg"));
    }

    #[test]
    fn original_url_encodes_background_file_name() {
        let url = original_background_url(886402, "背景 image.jpg").expect("url");
        assert_eq!(
            url.as_str(),
            "https://dl.sayobot.cn/beatmaps/files/886402/%E8%83%8C%E6%99%AF%20image.jpg"
        );
    }

    #[test]
    fn cached_image_becomes_a_data_url() {
        let value = to_data_url(b"\x89PNG\r\n\x1a\nimage", "image/png");
        assert!(value.starts_with("data:image/png;base64,"));
    }
}
