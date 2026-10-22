//! 账号头像本地缓存：以 `avatar-{user_id}.bin` 与 `.url` 成对文件落盘于应用数据目录，
//! 同时服务前端的 Data URL 与渲染器的文件路径两种消费方式。
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use tokio::sync::Mutex;
use url::Url;

use crate::error::{CommandError, CommandResult};
use crate::infrastructure::http::{HttpClientSpec, http_client};

/// 单张头像的字节上限（2 MiB）；下载前按 `Content-Length` 预检，下载后再复核。
const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

/// 头像缓存器；`write_lock` 将查缓存、下载、落盘整体串行化，避免并发重复下载。
pub struct AvatarCache {
    directory: PathBuf,
    client: reqwest::Client,
    write_lock: Mutex<()>,
}

impl AvatarCache {
    /// 在应用数据目录下创建 `image-cache` 目录，并构建带独立超时的 HTTP 客户端。
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let directory = app_data_dir.join("image-cache");
        fs::create_dir_all(&directory)?;
        let client = http_client(HttpClientSpec {
            timeout: Some(Duration::from_secs(10)),
            ua_note: " (avatar cache)",
            connect_timeout: None,
        })?;
        Ok(Self {
            directory,
            client,
            write_lock: Mutex::new(()),
        })
    }

    /// 读取或下载头像并返回 Data URL；网络失败时回退旧缓存而不是让调用失败。
    pub async fn load_or_fetch(
        &self,
        user_id: u64,
        avatar_url: &str,
        force_refresh: bool,
    ) -> CommandResult<Option<String>> {
        // 缓存命中优先，强制刷新时才绕过本地副本重新请求头像。
        let _guard = self.write_lock.lock().await;
        let image_path = self.directory.join(format!("avatar-{user_id}.bin"));
        let url_path = self.directory.join(format!("avatar-{user_id}.url"));
        let cached = fs::read(&image_path).ok();
        let cached_url = fs::read_to_string(&url_path).ok();

        if !force_refresh && cached_url.as_deref() == Some(avatar_url) {
            return Ok(cached.as_deref().map(to_data_url));
        }

        match self.fetch(avatar_url).await {
            Ok(bytes) => {
                fs::write(&image_path, &bytes)?;
                fs::write(&url_path, avatar_url)?;
                Ok(Some(to_data_url(&bytes)))
            }
            Err(_) => Ok(cached.as_deref().map(to_data_url)),
        }
    }

    /// 已缓存的头像落盘路径(`load_or_fetch` 成功后有效)。
    /// 供实时预览/导出的结算屏头像使用(渲染端按内容嗅探格式)。
    pub fn file_path(&self, user_id: u64) -> PathBuf {
        self.directory.join(format!("avatar-{user_id}.bin"))
    }

    /// 删除并重建整个 `image-cache` 目录，用于切换凭据、注销与手动清理。
    pub fn clear(&self) -> CommandResult<()> {
        if self.directory.exists() {
            fs::remove_dir_all(&self.directory)?;
        }
        fs::create_dir_all(&self.directory)?;
        Ok(())
    }

    /// 下载头像；仅接受 `ppy.sh` 及其子域的 HTTPS 地址，并校验大小与内容类型。
    async fn fetch(&self, avatar_url: &str) -> CommandResult<Vec<u8>> {
        // 头像 URL 来自 API，仍限制响应大小以防异常服务占用大量内存。
        let url = Url::parse(avatar_url)
            .map_err(|_| CommandError::new("INVALID_AVATAR_URL", "头像地址无效"))?;
        let host = url.host_str().unwrap_or_default();
        if url.scheme() != "https" || !(host == "ppy.sh" || host.ends_with(".ppy.sh")) {
            return Err(CommandError::new(
                "INVALID_AVATAR_URL",
                "头像地址不属于 osu! 官方域名",
            ));
        }

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?;
        if !response.status().is_success() {
            return Err(CommandError::new(
                "AVATAR_DOWNLOAD_FAILED",
                format!("头像请求失败（{}）", response.status()),
            ));
        }
        if response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_AVATAR_BYTES)
        {
            return Err(CommandError::new("AVATAR_TOO_LARGE", "头像文件过大"));
        }
        if !response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|content_type| content_type.starts_with("image/"))
        {
            return Err(CommandError::new("INVALID_AVATAR_DATA", "头像响应不是图片"));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?;
        if bytes.len() > MAX_AVATAR_BYTES {
            return Err(CommandError::new("AVATAR_TOO_LARGE", "头像文件过大"));
        }
        Ok(bytes.to_vec())
    }
}

// 依据文件魔数推断 MIME 类型，无法识别时按 JPEG 兜底。
fn to_data_url(bytes: &[u8]) -> String {
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        "image/jpeg"
    };
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::to_data_url;

    #[test]
    fn cached_png_uses_a_png_data_url() {
        let value = to_data_url(b"\x89PNG\r\n\x1a\nimage");

        assert!(value.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn cached_jpeg_uses_a_jpeg_data_url() {
        let value = to_data_url(b"\xff\xd8\xffimage");

        assert!(value.starts_with("data:image/jpeg;base64,"));
    }
}
