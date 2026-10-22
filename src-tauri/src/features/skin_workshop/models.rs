//! 皮肤工坊与前端共享的序列化契约：资源变体、目录树、配置文档与修改动作。

use serde::{Deserialize, Serialize};

use crate::features::local_analysis::LocalSkinSummary;

/// 工坊资产大类，决定预览时按图片还是音频呈现。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkshopAssetKind {
    Image,
    Audio,
}

/// 单个皮肤资源文件的索引条目；同一逻辑资源的 `@2x` 与各动画帧是独立变体。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinAssetVariant {
    /// 由逻辑路径哈希派生的稳定 ID，重新扫描后保持不变。
    pub asset_id: String,
    pub kind: WorkshopAssetKind,
    pub name: String,
    /// 相对皮肤根目录、以 `/` 分隔的规范化路径。
    pub logical_path: String,
    pub extension: String,
    pub size: u64,
    /// 分辨率倍率：`1` 为标准资源，`2` 为 `@2x` 高清变体。
    pub scale: u8,
    /// 动画帧序号，取自文件名末尾的数字段；非动画资源为 `None`。
    pub frame: Option<u32>,
}

/// 目录树节点；各类计数为该节点直属资源与全部后代资源的递归汇总。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinTreeNode {
    /// 由分组键哈希派生的节点 ID。
    pub part_id: String,
    /// 规范化分组键（路径段小写后以 `/` 连接），用于归属判断。
    pub part_key: String,
    pub label: String,
    pub path_segments: Vec<String>,
    pub asset_count: usize,
    pub image_count: usize,
    pub audio_count: usize,
    /// 子节点，按分组键字典序排列。
    pub children: Vec<SkinTreeNode>,
}

/// 一个皮肤的完整资源目录树。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinTree {
    pub skin_resource_id: String,
    pub roots: Vec<SkinTreeNode>,
}

/// 某个分组（含子分组）下的全部资源变体预览。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinPartPreview {
    pub skin_resource_id: String,
    pub part_key: String,
    pub assets: Vec<SkinAssetVariant>,
}

/// 资源内容载荷：文件字节以 base64 `data:` URL 内嵌，便于前端直接预览。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinAssetPayload {
    pub asset_id: String,
    pub kind: WorkshopAssetKind,
    pub mime_type: String,
    pub data_url: String,
}

/// skin.ini 中的一个键值条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinConfigEntry {
    pub key: String,
    pub value: String,
    /// 同一节内同名键的第几次出现（从 0 起），用于精确定位重复键。
    pub occurrence: usize,
    /// 条目在源文件中的行号（从 1 起），供前端定位展示。
    pub line: usize,
}

/// 一个 `[节]` 及其条目；首个节头之前的内容归入伪节 `Root`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinConfigSection {
    pub name: String,
    pub entries: Vec<SkinConfigEntry>,
}

/// 解析 skin.ini 时发现的一处语法问题。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinConfigError {
    pub line: usize,
    pub message: String,
}

/// skin.ini 的完整文档视图：结构化节、语法错误与原始源码并存。
///
/// `encoding` 与 `newline` 记录原文件的 BOM/编码与换行风格，回写时必须
/// 按它们还原，避免改变用户文件的原始风格。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinConfigDocument {
    /// 原始源码文本，供前端“源码模式”直接编辑。
    pub source: String,
    pub sections: Vec<SkinConfigSection>,
    pub errors: Vec<SkinConfigError>,
    pub encoding: String,
    pub newline: String,
}

/// 一次工坊修改动作；由 `execute_write` 在皮肤副本上原子落地。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkinWorkshopAction {
    /// 用本地文件替换皮肤内一个现有组件（扩展名必须一致）。
    ReplaceComponent {
        target_logical_path: String,
        replacement_path: String,
    },
    /// 用来源皮肤整组替换目标皮肤的某个分组。
    ReplacePart {
        target_part_key: String,
        source_skin_resource_id: String,
    },
    /// 从来源皮肤复制单个组件（目标不存在时允许新建）。
    CopyComponent {
        target_logical_path: String,
        source_skin_resource_id: String,
        source_logical_path: String,
    },
    /// 把来源皮肤的某个配置项值搬运到目标皮肤（缺失时插入）。
    CopyConfigEntry {
        source_skin_resource_id: String,
        section: String,
        key: String,
        occurrence: usize,
    },
    /// 整体覆写 skin.ini 源码。
    UpdateConfigSource { source: String },
    /// 修改目标皮肤中的一个配置项。
    UpdateConfigEntry {
        section: String,
        key: String,
        occurrence: usize,
        value: String,
    },
}

/// 发布写入模式：原地覆写或另存为新皮肤副本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SkinWorkshopWriteMode {
    /// 原地替换原皮肤目录（带备份回滚）。
    Overwrite,
    /// 复制为新皮肤，`name` 同时用作新目录名与 `Name` 配置值。
    CreateCopy { name: String },
}

/// 预设工作流：组合多条底层操作的一键流程。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkinWorkshopPreset {
    /// 把来源皮肤的 Mania 资源与 `[Mania]` 配置节整体迁移到目标皮肤。
    MigrateMania { source_skin_resource_id: String },
}

/// 一次发布写入的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinWorkshopMutationResult {
    pub name: String,
    pub path: String,
    /// 是否以 `CreateCopy` 模式新建了皮肤副本。
    pub created_copy: bool,
}

/// 最近一次打开的 `.osk` 临时包状态，落盘在工坊目录供后续操作读取。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PackageState {
    pub summary: LocalSkinSummary,
    /// 解包后 `skin.ini` 所在目录，作为该临时包的皮肤根目录。
    pub root: String,
    pub source_path: String,
}
