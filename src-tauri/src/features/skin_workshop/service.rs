use std::{
    collections::HashSet,
    fs,
    io::{BufReader, copy},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use uuid::Uuid;

use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::{LocalAnalysisService, LocalClient},
};

use super::{
    config::{
        copy_directory, read_config, replace_config_sections, safe_child, set_skin_name,
        update_config_entry, upsert_config_entry, write_config_source,
    },
    models::{
        PackageState, SkinAssetPayload, SkinConfigDocument, SkinPartPreview, SkinTree,
        SkinWorkshopAction, SkinWorkshopMutationResult, SkinWorkshopPreset, SkinWorkshopWriteMode,
        WorkshopAssetKind,
    },
    tree::{belongs_to, build_tree, index_assets},
};

pub struct SkinWorkshopService {
    package_root: PathBuf,
    local_analysis: Arc<LocalAnalysisService>,
    lock: Mutex<()>,
}

impl SkinWorkshopService {
    pub fn new(
        app_data_dir: &Path,
        local_analysis: Arc<LocalAnalysisService>,
    ) -> CommandResult<Self> {
        let package_root = app_data_dir.join("skin-workshop").join("package-preview");
        fs::create_dir_all(&package_root)?;
        Ok(Self {
            package_root,
            local_analysis,
            lock: Mutex::new(()),
        })
    }

    pub fn open_package(
        &self,
        path: &Path,
    ) -> CommandResult<crate::features::local_analysis::LocalSkinSummary> {
        let _guard = self.guard()?;
        let source = path.canonicalize().map_err(|error| {
            CommandError::new(
                "SKIN_PACKAGE_READ_ERROR",
                format!("无法读取 Skin 包：{error}"),
            )
        })?;
        if source
            .extension()
            .and_then(|value| value.to_str())
            .is_none_or(|value| !value.eq_ignore_ascii_case("osk"))
        {
            return Err(CommandError::new(
                "SKIN_PACKAGE_FORMAT_ERROR",
                "请选择 .osk Skin 文件",
            ));
        }
        if self.package_root.exists() {
            fs::remove_dir_all(&self.package_root)?;
        }
        let extracted = self.package_root.join("files");
        fs::create_dir_all(&extracted)?;
        let file = fs::File::open(&source)?;
        let mut archive = zip::ZipArchive::new(BufReader::new(file)).map_err(|error| {
            CommandError::new(
                "SKIN_PACKAGE_FORMAT_ERROR",
                format!("无法解析 .osk：{error}"),
            )
        })?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| {
                CommandError::new("SKIN_PACKAGE_FORMAT_ERROR", error.to_string())
            })?;
            let Some(relative) = entry.enclosed_name() else {
                continue;
            };
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                continue;
            }
            let destination = extracted.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(destination)?;
                continue;
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = fs::File::create(destination)?;
            copy(&mut entry, &mut output)?;
        }
        let config = walkdir::WalkDir::new(&extracted)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .find(|entry| {
                entry.file_type().is_file()
                    && entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("skin.ini")
            })
            .ok_or_else(|| {
                CommandError::new("SKIN_PACKAGE_CONFIG_MISSING", ".osk 中没有 skin.ini")
            })?;
        let skin_root = config.path().parent().unwrap_or(&extracted).to_path_buf();
        let bytes = fs::read(config.path())?;
        let mut summary = crate::features::local_analysis::parser::parse_skin(
            LocalClient::Stable,
            &bytes,
            "skin.ini",
            None,
            None,
            Some(&skin_root),
        )
        .map_err(|error| CommandError::new("SKIN_PACKAGE_CONFIG_INVALID", error))?
        .summary;
        summary.resource.resource_id = format!("package:skin:{}", Uuid::new_v4());
        summary.resource.logical_path = Some(source.to_string_lossy().to_string());
        let state = PackageState {
            summary: summary.clone(),
            root: skin_root.to_string_lossy().to_string(),
            source_path: source.to_string_lossy().to_string(),
        };
        fs::write(
            self.package_state_path(),
            serde_json::to_vec_pretty(&state)?,
        )?;
        Ok(summary)
    }

    pub fn tree(&self, client: LocalClient, skin_resource_id: &str) -> CommandResult<SkinTree> {
        let _guard = self.guard()?;
        self.ensure_stable(client)?;
        let (_, root) = self.skin_source(client, skin_resource_id)?;
        let assets = index_assets(&root)?;
        Ok(SkinTree {
            skin_resource_id: skin_resource_id.to_string(),
            roots: build_tree(&assets),
        })
    }

    pub fn part_preview(
        &self,
        client: LocalClient,
        skin_resource_id: &str,
        part_key: &str,
    ) -> CommandResult<SkinPartPreview> {
        let _guard = self.guard()?;
        self.ensure_stable(client)?;
        let (_, root) = self.skin_source(client, skin_resource_id)?;
        let assets = index_assets(&root)?
            .into_iter()
            .filter(|asset| belongs_to(&asset.part_key, part_key))
            .map(|asset| asset.summary)
            .collect::<Vec<_>>();
        if assets.is_empty() {
            return Err(CommandError::new(
                "SKIN_WORKSHOP_PART_NOT_FOUND",
                "未找到该 Skin 部分",
            ));
        }
        Ok(SkinPartPreview {
            skin_resource_id: skin_resource_id.to_string(),
            part_key: part_key.to_string(),
            assets,
        })
    }

    pub fn asset(
        &self,
        client: LocalClient,
        skin_resource_id: &str,
        asset_id: &str,
    ) -> CommandResult<SkinAssetPayload> {
        let _guard = self.guard()?;
        self.ensure_stable(client)?;
        let (_, root) = self.skin_source(client, skin_resource_id)?;
        let asset = index_assets(&root)?
            .into_iter()
            .find(|asset| asset.summary.asset_id == asset_id)
            .ok_or_else(|| {
                CommandError::new("SKIN_WORKSHOP_ASSET_NOT_FOUND", "未找到该 Skin 资源")
            })?;
        let limit = match asset.summary.kind {
            WorkshopAssetKind::Image => 32 * 1024 * 1024,
            WorkshopAssetKind::Audio => 24 * 1024 * 1024,
        };
        if asset.summary.size > limit {
            return Err(CommandError::new(
                "SKIN_WORKSHOP_ASSET_TOO_LARGE",
                "资源超过预览大小限制",
            ));
        }
        let bytes = fs::read(&asset.physical_path)?;
        let mime_type = mime_type(&asset.summary.extension, asset.summary.kind).to_string();
        Ok(SkinAssetPayload {
            asset_id: asset.summary.asset_id,
            kind: asset.summary.kind,
            data_url: format!("data:{mime_type};base64,{}", BASE64_STANDARD.encode(bytes)),
            mime_type,
        })
    }

    pub fn config(
        &self,
        client: LocalClient,
        skin_resource_id: &str,
    ) -> CommandResult<SkinConfigDocument> {
        let _guard = self.guard()?;
        let (_, root) = self.skin_source(client, skin_resource_id)?;
        read_config(&root)
    }

    pub fn execute_action(
        &self,
        target_skin_resource_id: &str,
        mode: SkinWorkshopWriteMode,
        action: SkinWorkshopAction,
    ) -> CommandResult<SkinWorkshopMutationResult> {
        let _guard = self.guard()?;
        let (summary, target_root) =
            self.skin_source(LocalClient::Stable, target_skin_resource_id)?;
        if target_skin_resource_id.starts_with("package:skin:")
            && matches!(&mode, SkinWorkshopWriteMode::Overwrite)
        {
            return Err(CommandError::new(
                "SKIN_PACKAGE_READ_ONLY",
                "临时 OSK 不能直接修改，请新建 Skin 副本",
            ));
        }
        self.execute_write(&summary.name, &target_root, mode, |root| {
            self.mutate_at(root, &action)
        })
    }

    pub fn execute_preset(
        &self,
        target_skin_resource_id: &str,
        mode: SkinWorkshopWriteMode,
        preset: SkinWorkshopPreset,
    ) -> CommandResult<SkinWorkshopMutationResult> {
        let _guard = self.guard()?;
        let (summary, target_root) =
            self.skin_source(LocalClient::Stable, target_skin_resource_id)?;
        if target_skin_resource_id.starts_with("package:skin:")
            && matches!(&mode, SkinWorkshopWriteMode::Overwrite)
        {
            return Err(CommandError::new(
                "SKIN_PACKAGE_READ_ONLY",
                "临时 OSK 不能直接修改，请新建 Skin 副本",
            ));
        }
        match preset {
            SkinWorkshopPreset::MigrateMania {
                source_skin_resource_id,
            } => {
                let (_, source_root) =
                    self.skin_source(LocalClient::Stable, &source_skin_resource_id)?;
                self.execute_write(&summary.name, &target_root, mode, |root| {
                    self.migrate_mania(root, &source_root)
                })
            }
        }
    }

    fn execute_write(
        &self,
        current_name: &str,
        target_root: &Path,
        mode: SkinWorkshopWriteMode,
        operation: impl FnOnce(&Path) -> CommandResult<()>,
    ) -> CommandResult<SkinWorkshopMutationResult> {
        match mode {
            SkinWorkshopWriteMode::Overwrite => {
                let parent = target_root.parent().ok_or_else(|| {
                    CommandError::new("SKIN_WORKSHOP_PATH_INVALID", "目标 Skin 路径无效")
                })?;
                let transaction_id = Uuid::new_v4();
                let temporary = parent.join(format!(".opp-direct-{transaction_id}"));
                let backup = parent.join(format!(".opp-backup-{transaction_id}"));
                copy_directory(target_root, &temporary)?;
                if let Err(error) = operation(&temporary) {
                    let _ = fs::remove_dir_all(&temporary);
                    return Err(error);
                }
                fs::rename(target_root, &backup)?;
                if let Err(error) = fs::rename(&temporary, target_root) {
                    let _ = fs::rename(&backup, target_root);
                    let _ = fs::remove_dir_all(&temporary);
                    return Err(error.into());
                }
                let _ = fs::remove_dir_all(&backup);
                self.local_analysis.workshop_refresh_stable()?;
                Ok(SkinWorkshopMutationResult {
                    name: current_name.to_string(),
                    path: target_root.to_string_lossy().to_string(),
                    created_copy: false,
                })
            }
            SkinWorkshopWriteMode::CreateCopy { name } => {
                let name = validate_name(&name)?;
                let skins_root = self.local_analysis.workshop_stable_skins_root()?;
                fs::create_dir_all(&skins_root)?;
                let target = skins_root.join(&name);
                if target.exists() {
                    return Err(CommandError::new("SKIN_NAME_EXISTS", "同名 Skin 已存在"));
                }
                let temporary = skins_root.join(format!(".opp-workshop-{}", Uuid::new_v4()));
                let result = copy_directory(target_root, &temporary)
                    .and_then(|_| operation(&temporary))
                    .and_then(|_| set_skin_name(&temporary, &name))
                    .and_then(|_| fs::rename(&temporary, &target).map_err(Into::into));
                if let Err(error) = result {
                    let _ = fs::remove_dir_all(&temporary);
                    return Err(error);
                }
                self.local_analysis.workshop_refresh_stable()?;
                Ok(SkinWorkshopMutationResult {
                    name,
                    path: target.to_string_lossy().to_string(),
                    created_copy: true,
                })
            }
        }
    }

    fn migrate_mania(&self, target_root: &Path, source_root: &Path) -> CommandResult<()> {
        let source_references = mania_asset_references(source_root)?;
        let target_references = mania_asset_references(target_root)?;
        let source_assets = index_assets(source_root)?
            .into_iter()
            .filter(|asset| {
                is_mania_asset(&asset.summary.name)
                    || matches_asset_reference(&asset.summary.name, &source_references)
            })
            .collect::<Vec<_>>();
        if source_assets.is_empty() {
            return Err(CommandError::new(
                "SKIN_PRESET_SOURCE_MISSING",
                "来源 Skin 没有可迁移的 Mania 资源",
            ));
        }
        for asset in index_assets(target_root)?.into_iter().filter(|asset| {
            is_mania_asset(&asset.summary.name)
                || matches_asset_reference(&asset.summary.name, &target_references)
        }) {
            fs::remove_file(asset.physical_path)?;
        }
        for asset in source_assets {
            let destination = safe_child(target_root, &asset.summary.logical_path)?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(asset.physical_path, destination)?;
        }
        replace_config_sections(target_root, source_root, "Mania")
    }

    fn mutate_at(&self, target_root: &Path, action: &SkinWorkshopAction) -> CommandResult<()> {
        match action {
            SkinWorkshopAction::ReplaceComponent {
                target_logical_path,
                replacement_path,
            } => self.replace_component_at(
                target_root,
                target_logical_path,
                Path::new(replacement_path),
            ),
            SkinWorkshopAction::ReplacePart {
                target_part_key,
                source_skin_resource_id,
            } => {
                let (_, source) = self.skin_source(LocalClient::Stable, source_skin_resource_id)?;
                self.replace_part_from_root(target_root, &source, target_part_key)
            }
            SkinWorkshopAction::CopyComponent {
                target_logical_path,
                source_skin_resource_id,
                source_logical_path,
            } => {
                let (_, source_root) =
                    self.skin_source(LocalClient::Stable, source_skin_resource_id)?;
                let source = safe_child(&source_root, source_logical_path)?.canonicalize()?;
                let canonical_source_root = source_root.canonicalize()?;
                if !source.starts_with(&canonical_source_root) || !source.is_file() {
                    return Err(CommandError::new(
                        "SKIN_WORKSHOP_PATH_INVALID",
                        "来源组件路径不安全",
                    ));
                }
                self.copy_component_at(target_root, target_logical_path, &source)
            }
            SkinWorkshopAction::CopyConfigEntry {
                source_skin_resource_id,
                section,
                key,
                occurrence,
            } => {
                let (_, source_root) =
                    self.skin_source(LocalClient::Stable, source_skin_resource_id)?;
                let source_document = read_config(&source_root)?;
                let entry = source_document
                    .sections
                    .iter()
                    .find(|item| item.name.eq_ignore_ascii_case(section))
                    .and_then(|item| {
                        item.entries.iter().find(|entry| {
                            entry.key.eq_ignore_ascii_case(key) && entry.occurrence == *occurrence
                        })
                    })
                    .ok_or_else(|| {
                        CommandError::new(
                            "SKIN_CONFIG_ENTRY_NOT_FOUND",
                            "来源 Skin 中未找到该配置项",
                        )
                    })?;
                upsert_config_entry(target_root, section, key, *occurrence, &entry.value)
            }
            SkinWorkshopAction::UpdateConfigSource { source } => {
                write_config_source(target_root, source)
            }
            SkinWorkshopAction::UpdateConfigEntry {
                section,
                key,
                occurrence,
                value,
            } => update_config_entry(target_root, section, key, *occurrence, value),
        }
    }

    fn replace_component_at(
        &self,
        root: &Path,
        logical_path: &str,
        replacement: &Path,
    ) -> CommandResult<()> {
        let target = safe_child(root, logical_path)?;
        if !target.is_file()
            || target
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(CommandError::new(
                "SKIN_WORKSHOP_ASSET_NOT_FOUND",
                "未找到要替换的组件",
            ));
        }
        let source = replacement.canonicalize().map_err(|error| {
            CommandError::new(
                "SKIN_REPLACEMENT_READ_ERROR",
                format!("无法读取替换文件：{error}"),
            )
        })?;
        if !source.is_file() {
            return Err(CommandError::new(
                "SKIN_REPLACEMENT_INVALID",
                "替换目标必须是文件",
            ));
        }
        let source_extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let target_extension = target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !source_extension.eq_ignore_ascii_case(target_extension) {
            return Err(CommandError::new(
                "SKIN_REPLACEMENT_FORMAT_MISMATCH",
                format!("请选择 .{target_extension} 文件"),
            ));
        }
        let temporary = target.with_extension(format!("{target_extension}.opp-workshop"));
        fs::copy(source, &temporary)?;
        fs::remove_file(&target)?;
        fs::rename(temporary, target)?;
        Ok(())
    }

    fn copy_component_at(
        &self,
        root: &Path,
        logical_path: &str,
        source: &Path,
    ) -> CommandResult<()> {
        let target = safe_child(root, logical_path)?;
        if target
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(CommandError::new(
                "SKIN_WORKSHOP_PATH_INVALID",
                "目标组件不能是符号链接",
            ));
        }
        if target.exists() {
            return self.replace_component_at(root, logical_path, source);
        }
        let source_extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let target_extension = target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if source_extension.is_empty() || !source_extension.eq_ignore_ascii_case(target_extension) {
            return Err(CommandError::new(
                "SKIN_REPLACEMENT_FORMAT_MISMATCH",
                format!("目标组件必须保持 .{source_extension} 格式"),
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
            let canonical_parent = parent.canonicalize()?;
            let canonical_root = root.canonicalize()?;
            if !canonical_parent.starts_with(&canonical_root) {
                return Err(CommandError::new(
                    "SKIN_WORKSHOP_PATH_INVALID",
                    "目标组件目录不安全",
                ));
            }
        }
        fs::copy(source, target)?;
        Ok(())
    }

    fn replace_part_from_root(
        &self,
        target_root: &Path,
        source_root: &Path,
        part_key: &str,
    ) -> CommandResult<()> {
        let source_assets = index_assets(source_root)?
            .into_iter()
            .filter(|asset| belongs_to(&asset.part_key, part_key))
            .collect::<Vec<_>>();
        let target_assets = index_assets(target_root)?
            .into_iter()
            .filter(|asset| belongs_to(&asset.part_key, part_key))
            .collect::<Vec<_>>();
        if source_assets.is_empty() && target_assets.is_empty() {
            return Err(CommandError::new(
                "SKIN_WORKSHOP_PART_NOT_FOUND",
                "来源和目标中均不存在该部分",
            ));
        }
        for asset in target_assets {
            if asset.physical_path.is_file() {
                fs::remove_file(asset.physical_path)?;
            }
        }
        for asset in source_assets {
            let destination = safe_child(target_root, &asset.summary.logical_path)?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(asset.physical_path, destination)?;
        }
        Ok(())
    }

    fn skin_source(
        &self,
        client: LocalClient,
        skin_resource_id: &str,
    ) -> CommandResult<(crate::features::local_analysis::LocalSkinSummary, PathBuf)> {
        if skin_resource_id.starts_with("package:skin:") {
            let state: PackageState =
                serde_json::from_slice(&fs::read(self.package_state_path())?)?;
            if state.summary.resource.resource_id != skin_resource_id {
                return Err(CommandError::new(
                    "SKIN_PACKAGE_EXPIRED",
                    "临时 Skin 包已失效",
                ));
            }
            return Ok((state.summary, PathBuf::from(state.root)));
        }
        Ok((
            self.local_analysis
                .skin_detail(client, skin_resource_id)?
                .summary,
            self.local_analysis
                .workshop_skin_root(client, skin_resource_id)?,
        ))
    }

    fn ensure_stable(&self, client: LocalClient) -> CommandResult<()> {
        if client != LocalClient::Stable {
            return Err(CommandError::new(
                "SKIN_WORKSHOP_LAZER_READ_ONLY",
                "Lazer Skin 目前仅提供配置摘要，无法可靠定位资源归属",
            ));
        }
        Ok(())
    }

    fn guard(&self) -> CommandResult<std::sync::MutexGuard<'_, ()>> {
        self.lock
            .lock()
            .map_err(|_| CommandError::new("SKIN_WORKSHOP_STATE_ERROR", "Skin Workshop 状态已损坏"))
    }

    fn package_state_path(&self) -> PathBuf {
        self.package_root.join("package.json")
    }
}

fn mime_type(extension: &str, kind: WorkshopAssetKind) -> &'static str {
    match (kind, extension) {
        (WorkshopAssetKind::Image, "png") => "image/png",
        (WorkshopAssetKind::Image, "gif") => "image/gif",
        (WorkshopAssetKind::Image, "jpg" | "jpeg") => "image/jpeg",
        (WorkshopAssetKind::Image, "webp") => "image/webp",
        (WorkshopAssetKind::Image, "bmp") => "image/bmp",
        (WorkshopAssetKind::Audio, "wav") => "audio/wav",
        (WorkshopAssetKind::Audio, "ogg") => "audio/ogg",
        (WorkshopAssetKind::Audio, "mp3") => "audio/mpeg",
        (WorkshopAssetKind::Image, _) => "application/octet-stream",
        (WorkshopAssetKind::Audio, _) => "application/octet-stream",
    }
}

fn is_mania_asset(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("mania-")
        || lower.starts_with("lightingn-")
        || lower.starts_with("lightingl-")
}

fn mania_asset_references(root: &Path) -> CommandResult<HashSet<String>> {
    let document = read_config(root)?;
    if !document.errors.is_empty() {
        return Err(CommandError::new(
            "SKIN_CONFIG_INVALID",
            "Mania 迁移前必须先修复 skin.ini",
        ));
    }
    let mut references = HashSet::new();
    for entry in document
        .sections
        .iter()
        .filter(|section| section.name.eq_ignore_ascii_case("Mania"))
        .flat_map(|section| &section.entries)
    {
        let key = entry.key.to_ascii_lowercase();
        let references_file = key.starts_with("noteimage")
            || key.starts_with("keyimage")
            || key.starts_with("hit")
            || matches!(
                key.as_str(),
                "stageleft"
                    | "stageright"
                    | "stagebottom"
                    | "stagelight"
                    | "lightingn"
                    | "lightingl"
                    | "warningarrow"
                    | "scoreprefix"
                    | "comboprefix"
            );
        if !references_file {
            continue;
        }
        let value = entry.value.trim().trim_matches(['"', '\'']);
        let file_name = Path::new(value)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(value);
        let stem = Path::new(file_name)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(file_name)
            .trim_end_matches("@2x")
            .to_ascii_lowercase();
        if !stem.is_empty() {
            references.insert(stem);
        }
    }
    Ok(references)
}

fn matches_asset_reference(name: &str, references: &HashSet<String>) -> bool {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name)
        .trim_end_matches("@2x")
        .to_ascii_lowercase();
    references.iter().any(|reference| {
        if &stem == reference {
            return true;
        }
        stem.strip_prefix(reference)
            .and_then(|suffix| suffix.strip_prefix('-'))
            .is_some_and(|suffix| {
                !suffix.is_empty() && suffix.chars().all(|value| value.is_ascii_digit())
            })
    })
}

fn validate_name(value: &str) -> CommandResult<String> {
    let name = value.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name.chars().any(|value| "<>:\"/\\|?*".contains(value))
    {
        return Err(CommandError::new(
            "SKIN_NAME_INVALID",
            "请输入有效且不包含路径字符的 Skin 名称",
        ));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_publish_names() {
        assert!(validate_name("../bad").is_err());
        assert!(validate_name("bad:name").is_err());
        assert_eq!(validate_name("My Skin").unwrap(), "My Skin");
    }

    #[test]
    fn complete_part_replacement_removes_old_variants_and_keeps_other_parts() {
        let app = tempfile::tempdir().expect("app");
        let target = tempfile::tempdir().expect("target");
        let source = tempfile::tempdir().expect("source");
        fs::write(target.path().join("cursor-0.png"), b"old").expect("old");
        fs::write(target.path().join("cursor-9.png"), b"obsolete").expect("obsolete");
        fs::write(target.path().join("menu-back.png"), b"keep").expect("keep");
        fs::write(source.path().join("cursor-0.png"), b"new").expect("new");
        fs::write(source.path().join("cursor-1@2x.png"), b"new2x").expect("new2x");
        let local = Arc::new(LocalAnalysisService::new(app.path()).expect("local"));
        let service = SkinWorkshopService::new(app.path(), local).expect("service");
        service
            .replace_part_from_root(target.path(), source.path(), "cursor")
            .expect("replace");
        assert_eq!(
            fs::read(target.path().join("cursor-0.png")).unwrap(),
            b"new"
        );
        assert!(target.path().join("cursor-1@2x.png").is_file());
        assert!(!target.path().join("cursor-9.png").exists());
        assert!(target.path().join("menu-back.png").is_file());
    }

    #[test]
    fn component_replacement_rejects_path_escape_and_format_changes() {
        let app = tempfile::tempdir().expect("app");
        let target = tempfile::tempdir().expect("target");
        let local = Arc::new(LocalAnalysisService::new(app.path()).expect("local"));
        let service = SkinWorkshopService::new(app.path(), local).expect("service");
        fs::write(target.path().join("cursor.png"), b"old").expect("target asset");
        let replacement = app.path().join("replacement.jpg");
        fs::write(&replacement, b"new").expect("replacement");
        assert!(
            service
                .replace_component_at(target.path(), "../cursor.png", &replacement)
                .is_err()
        );
        assert!(
            service
                .replace_component_at(target.path(), "cursor.png", &replacement)
                .is_err()
        );
    }

    #[test]
    fn copied_component_can_be_added_when_target_is_missing() {
        let app = tempfile::tempdir().expect("app");
        let target = tempfile::tempdir().expect("target");
        let local = Arc::new(LocalAnalysisService::new(app.path()).expect("local"));
        let service = SkinWorkshopService::new(app.path(), local).expect("service");
        let source = app.path().join("cursor-0@2x.png");
        fs::write(&source, b"new").expect("source");
        service
            .copy_component_at(target.path(), "nested/cursor-0@2x.png", &source)
            .expect("copy missing component");
        assert_eq!(
            fs::read(target.path().join("nested/cursor-0@2x.png")).unwrap(),
            b"new"
        );
        assert!(
            service
                .copy_component_at(target.path(), "../escape.png", &source)
                .is_err()
        );
    }

    #[test]
    fn mania_preset_replaces_resources_and_all_mania_sections_only() {
        let app = tempfile::tempdir().expect("app");
        let target = tempfile::tempdir().expect("target");
        let source = tempfile::tempdir().expect("source");
        fs::write(target.path().join("mania-note1.png"), b"old").expect("old mania");
        fs::write(target.path().join("cursor.png"), b"keep").expect("other mode");
        fs::write(
            target.path().join("skin.ini"),
            "[General]\nName: Target\n\n[Mania]\nKeys: 4\nColumnWidth: 40,40,40,40\n",
        )
        .expect("target config");
        fs::write(source.path().join("mania-note1@2x.png"), b"new").expect("new mania");
        fs::write(source.path().join("LightingN-0.png"), b"light").expect("lighting");
        fs::write(source.path().join("orb.png"), b"custom note").expect("custom note");
        fs::write(source.path().join("orb@2x.png"), b"custom note hd").expect("custom note hd");
        fs::write(source.path().join("spark-0.png"), b"custom lighting").expect("custom lighting");
        fs::write(
            source.path().join("skin.ini"),
            "[General]\nName: Source\n\n[Mania]\nKeys: 4\nColumnWidth: 50,50,50,50\nNoteImage1: orb\nLightingN: spark\n\n[Mania]\nKeys: 7\nColumnWidth: 30,30,30,30,30,30,30\n",
        )
        .expect("source config");
        let local = Arc::new(LocalAnalysisService::new(app.path()).expect("local"));
        let service = SkinWorkshopService::new(app.path(), local).expect("service");
        service
            .migrate_mania(target.path(), source.path())
            .expect("migrate mania");
        assert!(!target.path().join("mania-note1.png").exists());
        assert!(target.path().join("mania-note1@2x.png").is_file());
        assert!(target.path().join("LightingN-0.png").is_file());
        assert!(target.path().join("orb.png").is_file());
        assert!(target.path().join("orb@2x.png").is_file());
        assert!(target.path().join("spark-0.png").is_file());
        assert!(target.path().join("cursor.png").is_file());
        let config = read_config(target.path()).expect("read target config");
        assert!(config.source.contains("Name: Target"));
        assert_eq!(
            config
                .sections
                .iter()
                .filter(|section| section.name.eq_ignore_ascii_case("mania"))
                .count(),
            2
        );
        assert!(config.source.contains("ColumnWidth: 50,50,50,50"));
        assert!(!config.source.contains("ColumnWidth: 40,40,40,40"));
    }
}
