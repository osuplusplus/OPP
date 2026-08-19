use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::error::{CommandError, CommandResult};

use super::{
    models::{LocalCapabilities, LocalClient, LocalSourceStatus, SourceMode},
    parser::decode_text,
};

#[derive(Debug, Clone)]
pub struct ResolvedSource {
    pub status: LocalSourceStatus,
    pub beatmap_root: Option<PathBuf>,
    pub skin_root: Option<PathBuf>,
    pub repository_root: Option<PathBuf>,
}

pub trait LocalSourceAdapter: Send + Sync {
    fn resolve(&self, configured_path: Option<&Path>) -> ResolvedSource;
}

#[derive(Default)]
pub struct StableAdapter;

#[derive(Default)]
pub struct LazerAdapter;

impl LocalSourceAdapter for StableAdapter {
    fn resolve(&self, configured_path: Option<&Path>) -> ResolvedSource {
        resolve_stable(configured_path)
    }
}

impl LocalSourceAdapter for LazerAdapter {
    fn resolve(&self, configured_path: Option<&Path>) -> ResolvedSource {
        resolve_lazer(configured_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SourceOverrides {
    stable: Option<PathBuf>,
    lazer: Option<PathBuf>,
}

impl SourceOverrides {
    fn get(&self, client: LocalClient) -> Option<&Path> {
        match client {
            LocalClient::Stable => self.stable.as_deref(),
            LocalClient::Lazer => self.lazer.as_deref(),
        }
    }

    fn set(&mut self, client: LocalClient, path: Option<PathBuf>) {
        match client {
            LocalClient::Stable => self.stable = path,
            LocalClient::Lazer => self.lazer = path,
        }
    }
}

pub struct SourceResolver {
    path: PathBuf,
    overrides: Mutex<SourceOverrides>,
    stable: StableAdapter,
    lazer: LazerAdapter,
}

impl SourceResolver {
    pub fn load(directory: &Path) -> CommandResult<Self> {
        // 用户覆盖路径独立保存；自动发现逻辑始终可在覆盖被重置后重新生效。
        fs::create_dir_all(directory)?;
        let path = directory.join("sources.json");
        let overrides = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        Ok(Self {
            path,
            overrides: Mutex::new(overrides),
            stable: StableAdapter,
            lazer: LazerAdapter,
        })
    }

    pub fn resolve(&self, client: LocalClient) -> CommandResult<ResolvedSource> {
        let overrides = self
            .overrides
            .lock()
            .map_err(|_| CommandError::new("LOCAL_SOURCE_STATE_ERROR", "本地资源路径状态已损坏"))?;
        let configured = overrides.get(client);
        Ok(self.adapter(client).resolve(configured))
    }

    pub fn set_override(
        &self,
        client: LocalClient,
        selected_path: &Path,
    ) -> CommandResult<ResolvedSource> {
        if selected_path.as_os_str().is_empty() {
            return Err(CommandError::new(
                "INVALID_LOCAL_SOURCE",
                "请选择有效的 osu! 文件夹",
            ));
        }

        let canonical = selected_path.canonicalize().map_err(|error| {
            CommandError::new("INVALID_LOCAL_SOURCE", format!("无法访问所选目录：{error}"))
        })?;
        let resolved = self.adapter(client).resolve(Some(&canonical));
        if !resolved.status.valid {
            return Err(CommandError::new(
                "INVALID_LOCAL_SOURCE",
                resolved.status.validation_errors.join("；"),
            ));
        }

        let mut overrides = self
            .overrides
            .lock()
            .map_err(|_| CommandError::new("LOCAL_SOURCE_STATE_ERROR", "本地资源路径状态已损坏"))?;
        overrides.set(client, Some(canonical));
        self.persist(&overrides)?;
        Ok(resolved)
    }

    pub fn reset(&self, client: LocalClient) -> CommandResult<ResolvedSource> {
        let mut overrides = self
            .overrides
            .lock()
            .map_err(|_| CommandError::new("LOCAL_SOURCE_STATE_ERROR", "本地资源路径状态已损坏"))?;
        overrides.set(client, None);
        self.persist(&overrides)?;
        drop(overrides);
        self.resolve(client)
    }

    fn adapter(&self, client: LocalClient) -> &dyn LocalSourceAdapter {
        match client {
            LocalClient::Stable => &self.stable,
            LocalClient::Lazer => &self.lazer,
        }
    }

    fn persist(&self, overrides: &SourceOverrides) -> CommandResult<()> {
        // 使用临时文件替换覆盖配置，防止系统中断造成 JSON 截断。
        let bytes = serde_json::to_vec_pretty(overrides)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, bytes)?;
        atomic_replace(&temporary, &self.path)?;
        Ok(())
    }
}

fn atomic_replace(temporary: &Path, target: &Path) -> std::io::Result<()> {
    if target.exists() {
        let backup = target.with_extension("json.bak");
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(target, &backup)?;
        match fs::rename(temporary, target) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                Ok(())
            }
            Err(error) => {
                let _ = fs::rename(backup, target);
                Err(error)
            }
        }
    } else {
        fs::rename(temporary, target)
    }
}

fn resolve_stable(configured_path: Option<&Path>) -> ResolvedSource {
    // Stable 的 Songs 目录可在配置中改名或改为相对路径，不能假设固定位置。
    let mode = if configured_path.is_some() {
        SourceMode::Override
    } else {
        SourceMode::Auto
    };
    let detected = configured_path.map(Path::to_path_buf).or_else(|| {
        crate::platform::stable_install_candidates()
            .into_iter()
            .find(|path| path.exists())
    });
    let configured_path = configured_path.map(display_path);
    let mut errors = Vec::new();

    let install_root = detected.and_then(|path| {
        let candidate = if path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("Songs"))
        {
            path.parent().unwrap_or(&path).to_path_buf()
        } else {
            path
        };
        match candidate.canonicalize() {
            Ok(path) => Some(path),
            Err(error) => {
                errors.push(format!("无法访问 stable 目录：{error}"));
                None
            }
        }
    });

    if install_root.is_none() && errors.is_empty() {
        errors.push("未自动检测到 osu!stable，请手动选择安装目录".into());
    }

    let mut beatmap_root = None;
    let mut skin_root = None;
    let mut version = None;
    if let Some(root) = install_root.as_deref() {
        let has_client = root.join("osu!.exe").is_file() || root.join("Songs").is_dir();
        if !has_client {
            errors.push("该目录不包含 osu!.exe 或 Songs，无法识别为 stable".into());
        } else {
            beatmap_root = resolve_stable_beatmap_directory(root);
            if beatmap_root.as_ref().is_none_or(|path| !path.is_dir()) {
                errors.push("未找到有效的 stable 谱面目录".into());
                beatmap_root = None;
            }
            let skins = root.join("Skins");
            if skins.is_dir() {
                skin_root = Some(skins);
            }
            version = read_stable_version(root);
        }
    }

    ResolvedSource {
        status: LocalSourceStatus {
            client: LocalClient::Stable,
            mode,
            configured_path,
            install_root: install_root.as_deref().map(display_path),
            data_root: install_root.as_deref().map(display_path),
            version,
            valid: errors.is_empty(),
            validation_errors: errors,
            capabilities: LocalCapabilities::for_client(LocalClient::Stable),
            last_scanned_at: None,
        },
        beatmap_root,
        skin_root,
        repository_root: None,
    }
}

fn resolve_lazer(configured_path: Option<&Path>) -> ResolvedSource {
    // lazer 的数据根目录与安装目录不同；优先识别含 client.realm 的数据根目录。
    let mode = if configured_path.is_some() {
        SourceMode::Override
    } else {
        SourceMode::Auto
    };
    let registry_install = crate::platform::lazer_install_candidates()
        .into_iter()
        .find(|path| looks_like_lazer_install(path));
    // 自动识别的数据根以 storage.ini 的 FullPath
    let default_data = crate::platform::resolve_lazer_data_root();
    let configured_path_string = configured_path.map(display_path);

    let (install_root, data_candidate) = match configured_path {
        Some(path) if is_lazer_data_root(path) => {
            (registry_install.clone(), Some(path.to_path_buf()))
        }
        Some(path) if looks_like_lazer_install(path) => (Some(path.to_path_buf()), default_data),
        Some(path) => (registry_install.clone(), Some(path.to_path_buf())),
        None => (registry_install.clone(), default_data),
    };

    let mut errors = Vec::new();
    let data_root = data_candidate.and_then(|path| match path.canonicalize() {
        Ok(path) => Some(path),
        Err(error) => {
            errors.push(format!("无法访问 lazer 数据目录：{error}"));
            None
        }
    });
    if data_root.is_none() && errors.is_empty() {
        errors.push("未自动检测到 lazer 数据目录，请手动选择数据目录".into());
    }
    if let Some(root) = data_root.as_deref()
        && !is_lazer_data_root(root)
    {
        errors.push("lazer 数据目录必须同时包含 client.realm 和 files".into());
    }
    let repository_root = data_root
        .as_ref()
        .map(|root| root.join("files"))
        .filter(|path| path.is_dir());
    let install_root = install_root.and_then(|path| path.canonicalize().ok().or(Some(path)));
    let version = install_root.as_deref().and_then(read_lazer_version);

    ResolvedSource {
        status: LocalSourceStatus {
            client: LocalClient::Lazer,
            mode,
            configured_path: configured_path_string,
            install_root: install_root.as_deref().map(display_path),
            data_root: data_root.as_deref().map(display_path),
            version,
            valid: errors.is_empty() && repository_root.is_some(),
            validation_errors: errors,
            capabilities: LocalCapabilities::for_client(LocalClient::Lazer),
            last_scanned_at: None,
        },
        beatmap_root: None,
        skin_root: None,
        repository_root,
    }
}

fn resolve_stable_beatmap_directory(root: &Path) -> Option<PathBuf> {
    if let Some(value) = latest_stable_config_value(root, "BeatmapDirectory") {
        let path = PathBuf::from(value);
        return Some(if path.is_absolute() {
            path
        } else {
            root.join(path)
        });
    }
    Some(root.join("Songs"))
}

fn read_stable_version(root: &Path) -> Option<String> {
    latest_stable_config_value(root, "LastVersion")
}

fn latest_stable_config_value(root: &Path, key: &str) -> Option<String> {
    let mut configs = fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("osu!.") && name.ends_with(".cfg")
        })
        .collect::<Vec<_>>();
    configs.sort_by_key(|entry| {
        std::cmp::Reverse(
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH),
        )
    });
    configs.into_iter().find_map(|entry| {
        let text = decode_text(&fs::read(entry.path()).ok()?);
        config_value(&text, key)
    })
}

fn config_value(text: &str, wanted: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case(wanted)
            .then(|| value.trim().trim_matches('"').to_string())
    })
}

fn is_lazer_data_root(path: &Path) -> bool {
    path.join("client.realm").is_file() && path.join("files").is_dir()
}

fn looks_like_lazer_install(path: &Path) -> bool {
    path.join("current").is_dir()
        && (path.join("current").join("osu!.exe").is_file()
            || path.join("current").join("sq.version").is_file())
}

fn read_lazer_version(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("current").join("sq.version")).ok()?;
    let (_, after) = text.split_once("<version>")?;
    let (version, _) = after.split_once("</version>")?;
    Some(version.trim().to_string())
}

fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(network_path) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{network_path}")
    } else if let Some(local_path) = value.strip_prefix(r"\\?\") {
        local_path.to_string()
    } else {
        value.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_and_absolute_beatmap_directories() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::write(
            directory.path().join("osu!.user.cfg"),
            "BeatmapDirectory = CustomSongs",
        )
        .expect("write config");
        assert_eq!(
            resolve_stable_beatmap_directory(directory.path()),
            Some(directory.path().join("CustomSongs"))
        );

        let absolute = directory.path().join("ExternalSongs");
        fs::write(
            directory.path().join("osu!.user.cfg"),
            format!("BeatmapDirectory = {}", absolute.display()),
        )
        .expect("write config");
        assert_eq!(
            resolve_stable_beatmap_directory(directory.path()),
            Some(absolute)
        );
    }

    #[test]
    fn validates_stable_and_lazer_features() {
        let stable = tempfile::tempdir().expect("stable");
        fs::write(stable.path().join("osu!.exe"), []).expect("exe");
        fs::create_dir(stable.path().join("Songs")).expect("songs");
        assert!(resolve_stable(Some(stable.path())).status.valid);

        let lazer = tempfile::tempdir().expect("lazer");
        fs::write(lazer.path().join("client.realm"), []).expect("realm");
        fs::create_dir(lazer.path().join("files")).expect("files");
        assert!(resolve_lazer(Some(lazer.path())).status.valid);
    }

    #[test]
    fn manual_override_round_trips() {
        let app_data = tempfile::tempdir().expect("app data");
        let stable = tempfile::tempdir().expect("stable");
        fs::write(stable.path().join("osu!.exe"), []).expect("exe");
        fs::create_dir(stable.path().join("Songs")).expect("songs");
        let resolver = SourceResolver::load(app_data.path()).expect("resolver");
        let status = resolver
            .set_override(LocalClient::Stable, stable.path())
            .expect("override");
        assert_eq!(status.status.mode, SourceMode::Override);

        let reloaded = SourceResolver::load(app_data.path()).expect("reload");
        assert_eq!(
            reloaded
                .resolve(LocalClient::Stable)
                .expect("resolve")
                .status
                .mode,
            SourceMode::Override
        );
    }

    #[test]
    #[ignore = "machine acceptance test"]
    fn auto_detects_opt_in_machine_sources() {
        let app_data = tempfile::tempdir().expect("app data");
        let resolver = SourceResolver::load(app_data.path()).expect("resolver");
        for client in [LocalClient::Stable, LocalClient::Lazer] {
            let source = resolver.resolve(client).expect("source");
            println!(
                "{}",
                serde_json::to_string_pretty(&source.status).expect("status json")
            );
            assert!(
                source.status.valid,
                "{}",
                source.status.validation_errors.join("; ")
            );
        }
    }
}
