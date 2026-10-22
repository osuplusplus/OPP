//! 皮肤工坊业务编排：OSK 导入、资源浏览、组件级修改与发布写入。
//! 所有修改先在临时副本上完成，再一次性换入原位（或另存为新皮肤）；
//! 服务内互斥锁把工坊操作串行化，避免并发写坏皮肤目录。

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

/// 皮肤工坊服务；所有操作共用一把互斥锁串行执行。
pub struct SkinWorkshopService {
    /// `.osk` 解包内容与临时包状态的存放根目录。
    package_root: PathBuf,
    local_analysis: Arc<LocalAnalysisService>,
    /// 包导入与各类写入共用的互斥锁，任一时刻仅允许一个工坊任务运行。
    lock: Mutex<()>,
}

impl SkinWorkshopService {
    /// 创建服务并准备解包目录；在应用启动组装依赖时调用一次。
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

    /// 打开 `.osk` 皮肤包：清空旧解包目录、解压、定位 `skin.ini` 并生成摘要。
    ///
    /// 解析结果以 `package:skin:<uuid>` 标识并落盘为临时包状态；它可作为
    /// 修改来源参与融合，但不允许被原地覆写（见 `execute_action`）。
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
        // 每次导入都是全新会话：直接清空旧解包目录，旧临时包 ID 随之失效。
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
            // enclosed_name 拒绝 `../` 等逃逸路径（zip-slip 防护）。
            let Some(relative) = entry.enclosed_name() else {
                continue;
            };
            // 跳过压缩包内的符号链接条目，避免解出指向皮肤外的引用。
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
        // osk 可能带一层包装目录，以 skin.ini 所在目录作为皮肤根目录。
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
        // 赋予临时包独立资源 ID，`package:skin:` 前缀是 skin_source 识别依据。
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

    /// 构建指定皮肤的资源分组目录树。
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

    /// 列出某个分组（含子分组）下的全部资源变体。
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

    /// 读取单个资源并以 base64 `data:` URL 返回，供前端直接预览。
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
        // 图片/音频分别设 32MB 与 24MB 上限，防止超大文件撑爆前端内存。
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

    /// 读取 `skin.ini` 的结构化文档（节、条目、错误与原始源码）。
    pub fn config(
        &self,
        client: LocalClient,
        skin_resource_id: &str,
    ) -> CommandResult<SkinConfigDocument> {
        let _guard = self.guard()?;
        let (_, root) = self.skin_source(client, skin_resource_id)?;
        read_config(&root)
    }

    /// 在目标皮肤上执行单条修改动作，按写入模式覆写原位或另存副本。
    pub fn execute_action(
        &self,
        target_skin_resource_id: &str,
        mode: SkinWorkshopWriteMode,
        action: SkinWorkshopAction,
    ) -> CommandResult<SkinWorkshopMutationResult> {
        let _guard = self.guard()?;
        let (summary, target_root) =
            self.skin_source(LocalClient::Stable, target_skin_resource_id)?;
        // 临时包只存在于工坊目录且随时会被下次导入清空，禁止原地覆写。
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

    /// 执行预设工作流（当前仅 Mania 迁移），写入模式约束与单条动作一致。
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

    /// 统一的发布写入入口，所有修改都在临时副本上完成后才换入。
    ///
    /// `Overwrite`：复制原皮肤到临时目录，在副本上执行修改，成功后
    /// “原目录改名备份、临时目录顶替”；顶替失败把备份改回原位回滚。
    /// `CreateCopy`：在 Stable 皮肤根目录下以新名落一个副本，重名即拒绝，
    /// 副本的 `skin.ini` 中 `Name` 会被改写为新目录名。
    /// 两种模式成功后都刷新 Stable 索引，让前端立即看到新状态。
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
                // 修改全部发生在副本上，成功之前原目录保持原样可用。
                copy_directory(target_root, &temporary)?;
                if let Err(error) = operation(&temporary) {
                    let _ = fs::remove_dir_all(&temporary);
                    return Err(error);
                }
                // 先把原目录移走作为回滚凭据，再让副本顶替原位置。
                fs::rename(target_root, &backup)?;
                if let Err(error) = fs::rename(&temporary, target_root) {
                    // 顶替失败：备份改回原位恢复原皮肤，并清掉残留副本。
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
                // 复制 → 修改 → 改名 → 原子换入；任一步失败都清理临时目录，
                // 目标名只在最终 rename 时出现，不会留下半成品皮肤。
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

    /// Mania 迁移预设：清空目标的全部 Mania 资源后，整块搬运来源的
    /// Mania 资源与所有 `[Mania]` 配置节。
    ///
    /// Mania 资源由两部分判定：文件名前缀（`mania-`、`lightingn-`、
    /// `lightingl-`）以及 `[Mania]` 节中引用的自定义资源名，后者覆盖
    /// `NoteImage1: orb` 这类换皮配置，不迁移会留下指向缺失文件的键。
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
        // 先删后拷：来源中已不存在的旧变体（如多余帧）不会残留在目标里。
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

    /// 在给定根目录上落地单条动作；由 `execute_write` 在临时副本上调用。
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

    /// 用本地文件替换皮肤内一个现有组件。
    ///
    /// 要求目标存在且为普通文件、扩展名一致（防止把 png 换成 jpg 破坏
    /// 皮肤引用）；写入走“同目录临时文件 + 改名”，失败不留半截内容。
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

    /// 从来源皮肤复制组件到目标逻辑路径；目标已存在时退化为替换。
    ///
    /// 目标不存在时允许新建（含父目录），但父目录规范化后必须仍位于
    /// 皮肤根目录内，扩展名同样必须一致。
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
            // 新建父目录后再校验其规范化结果仍在皮肤根内，双重防穿越。
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

    /// 整组替换：删除目标中该分组的全部资源，再复制来源的同分组资源。
    ///
    /// 先删后拷可清掉来源里已不存在的旧变体（如多余帧文件），
    /// 其他分组的资源不受影响。
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

    /// 解析资源 ID 到（摘要, 皮肤根目录）。
    ///
    /// `package:skin:` 前缀走临时包状态文件，其余交给本地分析索引；
    /// 临时包只认最近一次导入的 ID，重启或再次导入后旧 ID 失效。
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

    /// 组件浏览与融合只支持 Stable 的目录结构，Lazer 仅提供配置摘要。
    fn ensure_stable(&self, client: LocalClient) -> CommandResult<()> {
        if client != LocalClient::Stable {
            return Err(CommandError::new(
                "SKIN_WORKSHOP_LAZER_READ_ONLY",
                "Lazer Skin Workshop 当前仅提供配置摘要，暂不支持组件浏览和融合",
            ));
        }
        Ok(())
    }

    /// 获取全局互斥；锁中毒说明上一个任务异常崩溃，状态不可信。
    fn guard(&self) -> CommandResult<std::sync::MutexGuard<'_, ()>> {
        self.lock
            .lock()
            .map_err(|_| CommandError::new("SKIN_WORKSHOP_STATE_ERROR", "Skin Workshop 状态已损坏"))
    }

    fn package_state_path(&self) -> PathBuf {
        self.package_root.join("package.json")
    }
}

/// 扩展名到 MIME 的映射，未知扩展退回 `application/octet-stream`。
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

/// 按 osu! 命名约定以文件名前缀判定 Mania 专属资源。
fn is_mania_asset(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("mania-")
        || lower.starts_with("lightingn-")
        || lower.starts_with("lightingl-")
}

/// 从 `[Mania]` 节收集被引用的资源名集合。
///
/// 值可能带引号或路径，统一取文件名去扩展名并剥掉 `@2x`；
/// 只认已知会引用文件的键，避免把数值配置误当作文件名。
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

/// 判断文件是否命中引用集合：文件名相等，或为“引用名 + 数字后缀”
/// （osu! 动画帧命名，如 `orb-0.png` 命中 `NoteImage1: orb`）。
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

/// 校验新皮肤名：拒绝空值、`.`/`..`、结尾点/空格与 Windows 非法字符，
/// 防止拼出非法或越界的目录路径。
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

    // 发布名拒绝路径穿越与非法字符。
    #[test]
    fn rejects_unsafe_publish_names() {
        assert!(validate_name("../bad").is_err());
        assert!(validate_name("bad:name").is_err());
        assert_eq!(validate_name("My Skin").unwrap(), "My Skin");
    }

    // 整组替换会清掉旧变体且不影响其他分组。
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

    // 组件替换拒绝路径穿越与扩展名变更。
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

    // 目标缺失时允许新建组件，同时仍拦截越界路径。
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

    // Mania 迁移只替换 Mania 资源与配置节，其余内容原样保留。
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
