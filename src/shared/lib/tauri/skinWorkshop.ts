import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { LocalSkinSummary, OsuClient, SkinConfigDocument, SkinPartPreview, SkinTree, SkinWorkshopAction, SkinWorkshopAssetPayload, SkinWorkshopMutationResult, SkinWorkshopPreset, SkinWorkshopWriteMode } from "../../types/osu";
import { call, isTauri } from "./invoke";

// skinWorkshop 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const skinWorkshopApi = {
  openSkinWorkshopPackage: (path: string) =>
    call<LocalSkinSummary>("open_skin_workshop_package", { path }),

  chooseSkinWorkshopPackage: async () => {
    if (!isTauri()) return null;
    const selected = await openDialog({ multiple: false, title: "打开 Skin 安装包", filters: [{ name: "osu! Skin", extensions: ["osk"] }] });
    return typeof selected === "string" ? selected : null;
  },

  getSkinWorkshopTree: (client: OsuClient, skinResourceId: string) =>
    call<SkinTree>("get_skin_workshop_tree", { client, skinResourceId }),

  getSkinWorkshopPartPreview: (client: OsuClient, skinResourceId: string, partKey: string) =>
    call<SkinPartPreview>("get_skin_workshop_part_preview", { client, skinResourceId, partKey }),

  getSkinWorkshopAsset: (client: OsuClient, skinResourceId: string, assetId: string) =>
    call<SkinWorkshopAssetPayload>("get_skin_workshop_asset", { client, skinResourceId, assetId }),

  getSkinWorkshopConfig: (client: OsuClient, skinResourceId: string) =>
    call<SkinConfigDocument>("get_skin_workshop_config", { client, skinResourceId }),

  executeSkinWorkshopAction: (targetSkinResourceId: string, mode: SkinWorkshopWriteMode, action: SkinWorkshopAction) =>
    call<SkinWorkshopMutationResult>("execute_skin_workshop_action", { targetSkinResourceId, mode, action }),

  executeSkinWorkshopPreset: (targetSkinResourceId: string, mode: SkinWorkshopWriteMode, preset: SkinWorkshopPreset) =>
    call<SkinWorkshopMutationResult>("execute_skin_workshop_preset", { targetSkinResourceId, mode, preset }),
};
