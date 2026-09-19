use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderValue, REFERER};
use serde_json::Value;
use tokio::sync::Mutex;
use url::Url;

use crate::{
    error::{CommandError, CommandResult},
    infrastructure::logging::global,
};

const MAX_BACKGROUND_BYTES: usize = 20 * 1024 * 1024;
const MISSING_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const SAYOBOT_INFO_URL: &str = "https://api.sayobot.cn/v2/beatmapinfo";
const SAYOBOT_FILES_URL: &str = "https://dl.sayobot.cn/beatmaps/files";

pub struct OnlineArtworkCache {
    directory: PathBuf,
    client: reqwest::Client,
    request_lock: Mutex<()>,
}

impl OnlineArtworkCache {
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
            request_lock: Mutex::new(()),
        })
    }

    pub async fn load_or_fetch(&self, beatmapset_id: u64) -> CommandResult<Option<String>> {
        let mut span = global().map(|logger| {
            logger.operation("online_beatmaps", format!("background:{beatmapset_id}"))
        });
        let _guard = self.request_lock.lock().await;
        let image_path = self
            .directory
            .join(format!("background-{beatmapset_id}.bin"));
        let mime_path = self
            .directory
            .join(format!("background-{beatmapset_id}.mime"));
        let missing_path = self
            .directory
            .join(format!("background-{beatmapset_id}.missing"));

        if let Some(data_url) = self.read_cached(&image_path, &mime_path, span.as_ref()) {
            if let Some(ref mut current) = span {
                current.finish_ok(Some(serde_json::json!({ "cache": "hit" })));
            }
            return Ok(Some(data_url));
        }
        if missing_cache_is_fresh(&missing_path) {
            if let Some(ref mut current) = span {
                current.finish_ok(Some(serde_json::json!({ "cache": "missing" })));
            }
            return Ok(None);
        }

        let result = self.fetch(beatmapset_id, span.as_ref()).await;
        match result {
            Ok(Some((bytes, mime))) => {
                let write_image = fs::write(&image_path, &bytes);
                if let Some(ref current) = span {
                    current.fs_op("write", &image_path, &write_image);
                }
                write_image?;
                let write_mime = fs::write(&mime_path, &mime);
                if let Some(ref current) = span {
                    current.fs_op("write", &mime_path, &write_mime);
                }
                write_mime?;
                let _ = fs::remove_file(&missing_path);
                let value = Some(to_data_url(&bytes, &mime));
                if let Some(ref mut current) = span {
                    current.finish_ok(Some(serde_json::json!({
                        "cache": "stored",
                        "bytes": bytes.len()
                    })));
                }
                Ok(value)
            }
            Ok(None) => {
                let write_missing = fs::write(&missing_path, b"not-found");
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

    fn read_cached(
        &self,
        image_path: &Path,
        mime_path: &Path,
        span: Option<&crate::infrastructure::logging::LogSpan>,
    ) -> Option<String> {
        let image = fs::read(image_path);
        if let Some(current) = span {
            current.fs_op("read", image_path, &image);
        }
        let bytes = image.ok()?;
        if bytes.is_empty() || bytes.len() > MAX_BACKGROUND_BYTES {
            return None;
        }
        let mime = fs::read_to_string(mime_path).unwrap_or_else(|_| sniff_mime(&bytes).into());
        Some(to_data_url(&bytes, &mime))
    }

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
        let response = self
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
        let bytes = response
            .bytes()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?
            .to_vec();
        if bytes.is_empty() || bytes.len() > MAX_BACKGROUND_BYTES {
            return Err(CommandError::new(
                "BACKGROUND_TOO_LARGE",
                "在线谱面背景文件为空或过大",
            ));
        }
        Ok(Some((bytes, mime)))
    }
}

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

fn original_background_url(beatmapset_id: u64, file_name: &str) -> CommandResult<Url> {
    let mut url = Url::parse(SAYOBOT_FILES_URL)
        .map_err(|error| CommandError::new("INVALID_URL", error.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| CommandError::new("INVALID_URL", "静态资源地址无效"))?
        .push(&beatmapset_id.to_string())
        .push(file_name);
    Ok(url)
}

fn missing_cache_is_fresh(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < MISSING_CACHE_TTL)
}

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

fn to_data_url(bytes: &[u8], mime: &str) -> String {
    let mime = normalize_image_mime(mime).unwrap_or_else(|| sniff_mime(bytes));
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

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
