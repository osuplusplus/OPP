use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{CommandError, CommandResult};

use super::models::{SkinAssetVariant, SkinTreeNode, WorkshopAssetKind};

#[derive(Debug, Clone)]
pub(crate) struct IndexedAsset {
    pub summary: SkinAssetVariant,
    pub node_segments: Vec<String>,
    pub part_key: String,
    pub physical_path: PathBuf,
}

#[derive(Default)]
struct NodeBuilder {
    label: String,
    segments: Vec<String>,
    direct_assets: Vec<SkinAssetVariant>,
    children: BTreeMap<String, NodeBuilder>,
}

pub(crate) fn index_assets(root: &Path) -> CommandResult<Vec<IndexedAsset>> {
    let canonical_root = root.canonicalize().map_err(|error| {
        CommandError::new(
            "SKIN_WORKSHOP_ROOT_INVALID",
            format!("无法访问 Skin 目录：{error}"),
        )
    })?;
    let mut assets = Vec::new();
    for entry in WalkDir::new(&canonical_root).follow_links(false) {
        let entry = entry
            .map_err(|error| CommandError::new("SKIN_WORKSHOP_SCAN_ERROR", error.to_string()))?;
        if !entry.file_type().is_file() || entry.path_is_symlink() {
            continue;
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = match extension.as_str() {
            "bmp" | "gif" | "jpeg" | "jpg" | "png" | "webp" => WorkshopAssetKind::Image,
            "mp3" | "ogg" | "wav" => WorkshopAssetKind::Audio,
            _ => continue,
        };
        let relative = entry
            .path()
            .strip_prefix(&canonical_root)
            .map_err(|error| CommandError::new("SKIN_WORKSHOP_PATH_ERROR", error.to_string()))?;
        let logical_path = relative.to_string_lossy().replace('\\', "/");
        let (node_segments, scale, frame) = asset_segments(relative);
        let part_key = canonical_key(&node_segments);
        let asset_id = format!(
            "workshop-asset:{}",
            digest(&logical_path.to_ascii_lowercase())
        );
        assets.push(IndexedAsset {
            summary: SkinAssetVariant {
                asset_id,
                kind,
                name: entry.file_name().to_string_lossy().to_string(),
                logical_path,
                extension,
                size: entry.metadata().map(|value| value.len()).unwrap_or(0),
                scale,
                frame,
            },
            node_segments,
            part_key,
            physical_path: entry.path().to_path_buf(),
        });
    }
    assets.sort_by(|left, right| left.summary.logical_path.cmp(&right.summary.logical_path));
    Ok(assets)
}

pub(crate) fn build_tree(assets: &[IndexedAsset]) -> Vec<SkinTreeNode> {
    let mut root = NodeBuilder::default();
    for asset in assets {
        let mut current = &mut root;
        for (index, segment) in asset.node_segments.iter().enumerate() {
            let key = segment.to_ascii_lowercase();
            let mut segments = asset.node_segments[..=index].to_vec();
            if segments.is_empty() {
                segments.push("未分组".into());
            }
            current = current.children.entry(key).or_insert_with(|| NodeBuilder {
                label: segment.clone(),
                segments,
                ..NodeBuilder::default()
            });
        }
        current.direct_assets.push(asset.summary.clone());
    }
    root.children
        .into_values()
        .map(NodeBuilder::finish)
        .collect()
}

impl NodeBuilder {
    fn finish(self) -> SkinTreeNode {
        let children = self
            .children
            .into_values()
            .map(NodeBuilder::finish)
            .collect::<Vec<_>>();
        let direct_images = self
            .direct_assets
            .iter()
            .filter(|item| item.kind == WorkshopAssetKind::Image)
            .count();
        let direct_audio = self
            .direct_assets
            .iter()
            .filter(|item| item.kind == WorkshopAssetKind::Audio)
            .count();
        let image_count =
            direct_images + children.iter().map(|item| item.image_count).sum::<usize>();
        let audio_count =
            direct_audio + children.iter().map(|item| item.audio_count).sum::<usize>();
        let asset_count =
            self.direct_assets.len() + children.iter().map(|item| item.asset_count).sum::<usize>();
        let part_key = canonical_key(&self.segments);
        SkinTreeNode {
            part_id: format!("skin-part:{}", digest(&part_key)),
            part_key,
            label: self.label,
            path_segments: self.segments,
            asset_count,
            image_count,
            audio_count,
            children,
        }
    }
}

fn asset_segments(relative: &Path) -> (Vec<String>, u8, Option<u32>) {
    let mut segments = relative
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let raw_stem = relative
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("resource");
    let lower = raw_stem.to_ascii_lowercase();
    let scale = if lower.ends_with("@2x") { 2 } else { 1 };
    let stem = if scale == 2 {
        &raw_stem[..raw_stem.len() - 3]
    } else {
        raw_stem
    };
    let has_hyphen = stem.contains('-');
    let mut filename_segments = stem
        .split('-')
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let frame = filename_segments
        .last()
        .and_then(|value| value.parse::<u32>().ok());
    if frame.is_some() && filename_segments.len() > 1 {
        filename_segments.pop();
    }
    if !has_hyphen {
        segments.push("未分组".into());
    }
    segments.extend(filename_segments);
    if segments.is_empty() {
        segments.push("未分组".into());
    }
    (segments, scale, frame)
}

pub(crate) fn canonical_key(segments: &[String]) -> String {
    segments
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn belongs_to(asset_key: &str, part_key: &str) -> bool {
    asset_key == part_key
        || asset_key
            .strip_prefix(part_key)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn digest(value: &str) -> String {
    let hash = Sha256::digest(value.as_bytes());
    hash.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn groups_hyphens_scales_frames_and_unclassified_files() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("selection-mod-easy.png"), b"a").expect("asset");
        fs::write(root.path().join("cursor-0.png"), b"b").expect("asset");
        fs::write(root.path().join("cursor-1@2x.png"), b"c").expect("asset");
        fs::write(root.path().join("welcome.wav"), b"d").expect("asset");
        let assets = index_assets(root.path()).expect("index");
        assert_eq!(
            assets
                .iter()
                .find(|item| item.summary.name == "cursor-1@2x.png")
                .unwrap()
                .summary
                .scale,
            2
        );
        assert_eq!(
            assets
                .iter()
                .find(|item| item.summary.name == "cursor-0.png")
                .unwrap()
                .summary
                .frame,
            Some(0)
        );
        assert!(
            assets
                .iter()
                .find(|item| item.summary.name == "welcome.wav")
                .unwrap()
                .part_key
                .starts_with("未分组/")
        );
        let tree = build_tree(&assets);
        assert!(tree.iter().any(|node| node.label == "selection"));
        assert!(tree.iter().any(|node| node.label == "未分组"));
    }

    #[test]
    fn ids_are_stable_across_reindexing() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("cursor-middle.png"), b"a").expect("asset");
        let first = build_tree(&index_assets(root.path()).expect("first"));
        let second = build_tree(&index_assets(root.path()).expect("second"));
        assert_eq!(first, second);
    }
}
