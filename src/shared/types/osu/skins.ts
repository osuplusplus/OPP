// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { LocalResourceRef } from "./localAnalysis";
import type { Completeness } from "./localResources";

export interface SkinConfigEntry {
  key: string;
  value: string;
  color?: number[] | null;
}

export interface SkinConfigSection {
  name: string;
  entries: SkinConfigEntry[];
}

export interface SkinInventory {
  file_count: number;
  total_bytes: number;
  by_extension: Record<string, number>;
}

export interface LocalSkinSummary {
  resource: LocalResourceRef;
  completeness: Completeness;
  name: string;
  author: string;
  version: string;
  section_count: number;
  has_mania_config: boolean;
  resource_count: number | null;
  total_bytes: number | null;
  modified_at: string | null;
  accent_colors: number[][];
}

export interface LocalSkinDetail {
  summary: LocalSkinSummary;
  sections: SkinConfigSection[];
  inventory: SkinInventory | null;
  notice: string | null;
}

export type SkinAssetKind = "image" | "audio";

export interface LocalSkinAssetSummary {
  resource_id: string;
  kind: SkinAssetKind;
  name: string;
  logical_path: string;
  extension: string;
  size: number;
  category: string;
}

export interface LocalSkinPreview {
  skin_resource_id: string;
  completeness: Completeness;
  images: LocalSkinAssetSummary[];
  sounds: LocalSkinAssetSummary[];
}

export interface LocalSkinAssetPayload {
  resource_id: string;
  kind: SkinAssetKind;
  mime_type: string;
  data_url: string;
}

export type WorkshopAssetKind = "image" | "audio";

export interface SkinAssetVariant {
  asset_id: string;
  kind: WorkshopAssetKind;
  name: string;
  logical_path: string;
  extension: string;
  size: number;
  scale: number;
  frame: number | null;
}

export interface SkinTreeNode {
  part_id: string;
  part_key: string;
  label: string;
  path_segments: string[];
  asset_count: number;
  image_count: number;
  audio_count: number;
  children: SkinTreeNode[];
}

export interface SkinTree {
  skin_resource_id: string;
  roots: SkinTreeNode[];
}

export interface SkinPartPreview {
  skin_resource_id: string;
  part_key: string;
  assets: SkinAssetVariant[];
}

export interface SkinWorkshopAssetPayload {
  asset_id: string;
  kind: WorkshopAssetKind;
  mime_type: string;
  data_url: string;
}

export interface SkinWorkshopConfigEntry {
  key: string;
  value: string;
  occurrence: number;
  line: number;
}

export interface SkinWorkshopConfigSection {
  name: string;
  entries: SkinWorkshopConfigEntry[];
}

export interface SkinConfigDocument {
  source: string;
  sections: SkinWorkshopConfigSection[];
  errors: Array<{ line: number; message: string }>;
  encoding: string;
  newline: string;
}

export type SkinWorkshopAction =
  | { type: "replace_component"; target_logical_path: string; replacement_path: string }
  | { type: "replace_part"; target_part_key: string; source_skin_resource_id: string }
  | { type: "copy_component"; target_logical_path: string; source_skin_resource_id: string; source_logical_path: string }
  | { type: "copy_config_entry"; source_skin_resource_id: string; section: string; key: string; occurrence: number }
  | { type: "update_config_source"; source: string }
  | { type: "update_config_entry"; section: string; key: string; occurrence: number; value: string };

export type SkinWorkshopWriteMode =
  | { mode: "overwrite" }
  | { mode: "create_copy"; name: string };

export type SkinWorkshopPreset =
  | { type: "migrate_mania"; source_skin_resource_id: string };

export interface SkinWorkshopMutationResult {
  name: string;
  path: string;
  created_copy: boolean;
}
