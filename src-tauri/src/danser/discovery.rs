use std::{
    fs,
    path::{Path, PathBuf},
};

fn is_danser_executable(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                #[cfg(windows)]
                {
                    name.eq_ignore_ascii_case("danser-cli.exe")
                }
                #[cfg(not(windows))]
                {
                    name.eq_ignore_ascii_case("danser") || name.eq_ignore_ascii_case("danser-cli")
                }
            })
}

/// PATH 中查找 danser（仅类 Unix 生效；Windows 上 `find_in_path` 返回 `None`）。
fn danser_in_path() -> Option<PathBuf> {
    crate::platform::find_in_path("danser")
        .or_else(|| crate::platform::find_in_path("danser-cli"))
        .filter(|path| is_danser_executable(path))
}

fn saved_danser(saved: Option<&str>) -> Option<PathBuf> {
    saved
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .filter(|path| is_danser_executable(path))
}

/// 状态展示：用户配置优先，回退 PATH（与 tosu 状态一致）。
pub(super) fn find_danser(saved: Option<&str>) -> Option<PathBuf> {
    saved_danser(saved).or_else(danser_in_path)
}

/// 启动解析：PATH 优先，回退用户配置（与 tosu 启动一致）。
pub(super) fn resolve_danser_path(saved: Option<&str>) -> Option<PathBuf> {
    danser_in_path().or_else(|| saved_danser(saved))
}

pub(super) fn list_profiles_for(executable: &Path) -> Vec<String> {
    // Linux：danser-go 的 settings 在 XDG 配置目录（~/.config/danser/*.json）；
    // Windows：发行包自带的 settings/ 子目录。
    #[cfg(not(windows))]
    let settings_dir = {
        let _ = executable;
        crate::platform::danser_config_dir()
    };
    #[cfg(windows)]
    let settings_dir = executable.parent().map(|root| root.join("settings"));
    let Some(settings_dir) = settings_dir else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(settings_dir) else {
        return Vec::new();
    };
    let mut profiles: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("json"));
            if !is_json
                || serde_json::from_slice::<serde_json::Value>(&fs::read(&path).ok()?).is_err()
            {
                return None;
            }
            path.file_stem()?.to_str().map(str::to_string)
        })
        .collect();
    profiles.sort_by_key(|value| value.to_ascii_lowercase());
    profiles
}

pub(super) fn ffmpeg_available(executable: &Path) -> bool {
    // Danser 发行包可能自带 ffmpeg（同级或 ffmpeg/ 子目录），名称随平台不同；
    // 此外可直接使用 PATH 中的系统 ffmpeg。
    let bundled = executable.parent().is_some_and(|root| {
        ["ffmpeg.exe", "ffmpeg"]
            .iter()
            .any(|name| root.join(name).is_file() || root.join("ffmpeg").join(name).is_file())
    });
    bundled || crate::platform::find_in_path("ffmpeg").is_some()
}
