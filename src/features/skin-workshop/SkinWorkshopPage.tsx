import { useDeferredValue, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  ArrowLeft,
  AudioLines,
  Copy,
  FileImage,
  FolderGit2,
  FolderOpen,
  LoaderCircle,
  PackageOpen,
  Palette,
  RefreshCw,
  Search,
  Settings2,
  ShieldAlert,
  WandSparkles,
  X,
} from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, EmptyState, Skeleton } from "../../shared/components/ui";
import { cn } from "../../shared/lib/cn";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  CommandError,
  LocalSkinSummary,
  SkinAssetVariant,
  SkinTreeNode,
  SkinWorkshopAction,
  SkinWorkshopWriteMode,
} from "../../shared/types/osu";
import { useLocalSkins } from "../local-analysis/api";
import {
  useWorkshopConfig,
  useWorkshopPart,
  useWorkshopTree,
} from "./api";
import {
  AnimatedAssetVisual,
  AssetVisual,
  ConfigWorkspace,
  LazerReadOnly,
  PartCard,
  StagingTray,
  type StagedItem,
} from "./components";

function samePart(node: SkinTreeNode, key: string) {
  return node.part_key === key || key.startsWith(`${node.part_key}/`);
}

function errorMessage(error: unknown) {
  return (error as CommandError)?.message ?? String(error);
}

function SkinCover({ skin, onOpen, onReveal, imported = false }: { skin: LocalSkinSummary; onOpen: () => void; onReveal: () => void; imported?: boolean }) {
  return (
    <article className="flex min-h-64 h-full flex-col overflow-hidden rounded-3xl border border-white/[0.09] bg-[radial-gradient(circle_at_80%_10%,rgba(255,106,167,.16),transparent_42%),radial-gradient(circle_at_10%_90%,rgba(92,225,230,.11),transparent_38%),rgba(255,255,255,.025)] p-6 transition-colors duration-200 hover:border-white/20">
      <button className="text-left" onClick={onOpen} type="button"><div className="flex items-start justify-between gap-4">
        <span className="grid size-14 place-items-center rounded-2xl border border-pink-300/15 bg-pink-300/10 text-pink-200"><Palette className="size-6" /></span>
        <Badge tone={imported ? "cyan" : skin.completeness === "complete" ? "success" : "warning"}>{imported ? "OSK" : skin.completeness === "complete" ? "完整" : "部分索引"}</Badge>
      </div>
      <h2 className="mt-7 truncate text-xl font-semibold tracking-tight text-white">{skin.name}</h2>
      <p className="mt-2 truncate text-sm text-slate-400">{skin.author} · {skin.version}</p>
      <div className="mt-6 flex items-center gap-3 text-xs text-slate-500"><span>{skin.resource_count ?? 0} 个资源</span><span>·</span><span>{skin.section_count} 配置节</span></div></button>
      <div className="mt-auto grid grid-cols-2 gap-3 pt-6">
        <Button className="justify-center" onClick={onOpen} variant="primary">进入预览</Button>
        <Button className="justify-center" onClick={onReveal}><FolderOpen className="size-4" />本地打开</Button>
      </div>
    </article>
  );
}

function preferHighResolution(assets: SkinAssetVariant[]) {
  const selected = new Map<string, SkinAssetVariant>();
  for (const asset of assets) {
    const key = asset.logical_path.toLowerCase().replace(/@2x(?=\.[^.]+$)/i, "");
    const current = selected.get(key);
    if (!current || asset.scale > current.scale) selected.set(key, asset);
  }
  return { assets: [...selected.values()], hidden: assets.length - selected.size };
}

function groupAnimationFrames(assets: SkinAssetVariant[]) {
  const groups = new Map<string, SkinAssetVariant[]>();
  for (const asset of assets) {
    const key = asset.frame === null
      ? asset.logical_path.toLowerCase()
      : asset.logical_path.toLowerCase().replace(/\d+(?=(?:@2x)?\.[^.]+$)/i, "{frame}");
    groups.set(key, [...(groups.get(key) ?? []), asset]);
  }
  return [...groups.values()].map((group) => group.sort((left, right) => (left.frame ?? 0) - (right.frame ?? 0)));
}

function WriteModeDialog({ copyName, imported, skinName, onCancel, onChangeName, onCreateCopy, onOverwrite }: {
  copyName: string;
  imported: boolean;
  skinName: string;
  onCancel: () => void;
  onChangeName: (name: string) => void;
  onCreateCopy: () => void;
  onOverwrite: () => void;
}) {
  return (
    <div className="fixed inset-0 z-[1300] grid place-items-center bg-black/60 p-6 backdrop-blur-sm">
      <Card className="w-full max-w-lg overflow-hidden border-white/15 shadow-[0_28px_100px_rgba(0,0,0,.65)]">
        <div className="flex items-start gap-4 border-b border-white/[0.08] p-6">
          <span className="grid size-12 shrink-0 place-items-center rounded-2xl bg-[var(--theme-primary-muted)] text-[var(--theme-primary-light)]"><ShieldAlert className="size-5" /></span>
          <div className="min-w-0 flex-1"><h2 className="text-xl font-semibold text-white">保存 Skin 修改</h2><p className="mt-2 text-sm leading-6 text-slate-400">选择复制为新 Skin，或直接替换“{skinName}”。所有操作都会在完整校验后一次性写入。</p></div>
          <button aria-label="取消保存" className="grid size-9 place-items-center rounded-xl text-slate-500 hover:bg-white/[0.06] hover:text-white" onClick={onCancel} type="button"><X className="size-4" /></button>
        </div>
        <div className="space-y-5 p-6">
          <label className="block"><span className="mb-2 block text-sm font-semibold text-slate-200">新 Skin 名称</span><input autoFocus className="w-full rounded-xl border border-white/[0.1] bg-black/25 px-4 py-3 text-base text-white outline-none focus:border-[var(--theme-primary)]" onChange={(event) => onChangeName(event.target.value)} value={copyName} /></label>
          {imported ? <p className="rounded-xl border border-amber-300/15 bg-amber-300/[0.07] px-4 py-3 text-sm text-amber-100">临时打开的 OSK 只能新建副本，不能直接覆盖安装包。</p> : null}
          <div className="grid gap-3 sm:grid-cols-2"><Button className="min-h-11 justify-center" disabled={imported} onClick={onOverwrite} variant="danger">直接替换当前 Skin</Button><Button className="min-h-11 justify-center" disabled={!copyName.trim()} onClick={onCreateCopy} variant="primary"><Copy className="size-4" />新建副本并保存</Button></div>
        </div>
      </Card>
    </div>
  );
}

export function SkinWorkshopPage() {
  const { client } = useMode();
  const reduceMotion = useReducedMotion();
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const skinsQuery = useLocalSkins({ client, search: deferredSearch, sort: "name", direction: "asc", offset: 0, limit: 200 }, true);
  const [importedSkin, setImportedSkin] = useState<LocalSkinSummary | null>(null);
  const skins = useMemo(() => {
    const local = skinsQuery.data?.items ?? [];
    return importedSkin && client === "stable" ? [importedSkin, ...local.filter((skin) => skin.resource.resource_id !== importedSkin.resource.resource_id)] : local;
  }, [client, importedSkin, skinsQuery.data?.items]);
  const [openedSkinId, setOpenedSkinId] = useState<string | null>(null);
  const openedSkin = skins.find((skin) => skin.resource.resource_id === openedSkinId) ?? null;
  const [manualPartKey, setManualPartKey] = useState<string | null>(null);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [showConfig, setShowConfig] = useState(false);
  const [staged, setStaged] = useState<StagedItem[]>([]);
  const [trayCollapsed, setTrayCollapsed] = useState(true);
  const [working, setWorking] = useState(false);
  const [actionError, setActionError] = useState<unknown>(null);
  const [maniaSource, setManiaSource] = useState("");
  const [writeDialog, setWriteDialog] = useState<{ resolve: (mode: SkinWorkshopWriteMode | null) => void; skinName: string; imported: boolean } | null>(null);
  const [copyName, setCopyName] = useState("");

  const treeQuery = useWorkshopTree(client, openedSkinId);
  const autoPartKey = openedSkinId ? staged.find((item) => item.type !== "config" && item.sourceSkinId !== openedSkinId)?.partKey ?? null : null;
  const selectedPart = treeQuery.data?.roots.find((node) => samePart(node, manualPartKey ?? autoPartKey ?? "")) ?? null;
  const partQuery = useWorkshopPart(client, openedSkinId, selectedPart?.part_key ?? null);
  const configQuery = useWorkshopConfig(client, openedSkinId, Boolean(openedSkinId));
  const selectedAsset = partQuery.data?.assets.find((asset) => asset.asset_id === selectedAssetId) ?? null;
  const imageResolution = preferHighResolution(partQuery.data?.assets.filter((asset) => asset.kind === "image") ?? []);
  const imageGroups = groupAnimationFrames(imageResolution.assets);
  const sounds = partQuery.data?.assets.filter((asset) => asset.kind === "audio") ?? [];
  const frameRate = Math.min(60, Math.max(1, Number(configQuery.data?.sections
    .find((section) => section.name.toLowerCase() === "general")
    ?.entries.find((entry) => entry.key.toLowerCase() === "animationframerate")?.value) || 12));

  const transition = reduceMotion ? { duration: 0 } : { duration: 0.18, ease: "easeOut" as const };
  const refreshWorkshop = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["skin-workshop-tree"] }),
      queryClient.invalidateQueries({ queryKey: ["skin-workshop-part"] }),
      queryClient.invalidateQueries({ queryKey: ["skin-workshop-asset"] }),
      queryClient.invalidateQueries({ queryKey: ["skin-workshop-config"] }),
    ]);
  };
  const refreshLibrary = async () => {
    setWorking(true);
    setActionError(null);
    try {
      await desktopApi.scanLocalSource(client, false);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["local-skins"] }),
        queryClient.invalidateQueries({ queryKey: ["local-skin-detail"] }),
        queryClient.invalidateQueries({ queryKey: ["local-skin-preview"] }),
        queryClient.invalidateQueries({ queryKey: ["local-skin-asset"] }),
        refreshWorkshop(),
      ]);
    } catch (error) {
      setActionError(error);
    } finally {
      setWorking(false);
    }
  };
  const openSkin = (skin: LocalSkinSummary) => {
    setOpenedSkinId(skin.resource.resource_id);
    setManualPartKey(null);
    setSelectedAssetId(null);
    setShowConfig(false);
  };
  const returnToLibrary = () => {
    setOpenedSkinId(null); setManualPartKey(null); setSelectedAssetId(null); setShowConfig(false);
  };
  const openPackage = async () => {
    const path = await desktopApi.chooseSkinWorkshopPackage();
    if (!path) return;
    setWorking(true); setActionError(null);
    try { const skin = await desktopApi.openSkinWorkshopPackage(path); setImportedSkin(skin); openSkin(skin); }
    catch (error) { setActionError(error); }
    finally { setWorking(false); }
  };
  const revealSkin = async (skin: LocalSkinSummary) => {
    setActionError(null);
    try {
      const path = skin.resource.logical_path;
      if (!path) throw { code: "SKIN_PATH_UNAVAILABLE", message: "该 Skin 没有可打开的本地路径。" } satisfies CommandError;
      if (skin.resource.resource_id.startsWith("package:skin:")) await desktopApi.openDownloadedPath(path);
      else await desktopApi.openLocalResourceInExplorer(client, path);
    } catch (error) { setActionError(error); }
  };
  const addStaged = (item: StagedItem) => {
    setStaged((current) => current.some((value) => value.id === item.id) ? current : [...current, item]);
    setTrayCollapsed(false);
  };
  const stagePart = (node: SkinTreeNode) => {
    if (!openedSkin) return;
    addStaged({ id: `part:${openedSkin.resource.resource_id}:${node.part_key}`, type: "part", sourceSkinId: openedSkin.resource.resource_id, sourceSkinName: openedSkin.name, partKey: node.part_key, label: node.label });
  };
  const stageAsset = (asset: SkinAssetVariant) => {
    if (!openedSkin || !selectedPart) return;
    addStaged({ id: `asset:${openedSkin.resource.resource_id}:${asset.logical_path}`, type: "asset", sourceSkinId: openedSkin.resource.resource_id, sourceSkinName: openedSkin.name, partKey: selectedPart.part_key, label: asset.name, asset });
  };
  const stageConfig = (section: string, entry: { key: string; occurrence: number; value: string }) => {
    if (!openedSkin) return;
    addStaged({
      id: `config:${openedSkin.resource.resource_id}:${section}:${entry.key}:${entry.occurrence}`,
      type: "config",
      sourceSkinId: openedSkin.resource.resource_id,
      sourceSkinName: openedSkin.name,
      partKey: "",
      label: `[${section}] ${entry.key}`,
      config: { section, key: entry.key, occurrence: entry.occurrence, value: entry.value },
    });
  };
  const handleDrop = (type: "part" | "asset", id: string) => {
    if (type === "part") {
      const node = treeQuery.data?.roots.find((value) => value.part_key === id);
      if (node) stagePart(node);
    } else {
      const asset = partQuery.data?.assets.find((value) => value.asset_id === id);
      if (asset) stageAsset(asset);
    }
  };
  const canApply = (item: StagedItem) => Boolean(openedSkin && openedSkin.resource.resource_id !== item.sourceSkinId);
  const chooseWriteMode = (): Promise<SkinWorkshopWriteMode | null> => {
    if (!openedSkin) return Promise.resolve(null);
    setCopyName(`${openedSkin.name} - Workshop`);
    return new Promise((resolve) => setWriteDialog({
      resolve,
      skinName: openedSkin.name,
      imported: openedSkin.resource.resource_id.startsWith("package:skin:"),
    }));
  };
  const closeWriteDialog = (mode: SkinWorkshopWriteMode | null) => {
    writeDialog?.resolve(mode);
    setWriteDialog(null);
  };
  const executeAction = async (action: SkinWorkshopAction) => {
    if (!openedSkin) return;
    const mode = await chooseWriteMode();
    if (!mode) return;
    const result = await desktopApi.executeSkinWorkshopAction(openedSkin.resource.resource_id, mode, action);
    await Promise.all([refreshWorkshop(), queryClient.invalidateQueries({ queryKey: ["local-skins"] })]);
    setActionError({ code: "MUTATION_COMPLETE", message: result.created_copy ? `已创建并修改 ${result.name}` : `已直接修改 ${result.name}` } satisfies CommandError);
  };
  const applyDirectAction = async (action: SkinWorkshopAction) => {
    setWorking(true); setActionError(null);
    try { await executeAction(action); }
    catch (error) { setActionError(error); }
    finally { setWorking(false); }
  };
  const applyStaged = async (item: StagedItem) => {
    if (!openedSkin || !canApply(item)) return;
    setWorking(true); setActionError(null);
    try {
      let action: SkinWorkshopAction;
      if (item.type === "part") {
        action = { type: "replace_part", target_part_key: item.partKey, source_skin_resource_id: item.sourceSkinId };
      } else if (item.type === "config" && item.config) {
        action = { type: "copy_config_entry", source_skin_resource_id: item.sourceSkinId, section: item.config.section, key: item.config.key, occurrence: item.config.occurrence };
      } else if (item.asset) {
        const target = partQuery.data?.assets.find((asset) => asset.logical_path.toLowerCase() === item.asset?.logical_path.toLowerCase())
          ?? partQuery.data?.assets.find((asset) => asset.name.toLowerCase() === item.asset?.name.toLowerCase());
        action = { type: "copy_component", target_logical_path: target?.logical_path ?? item.asset.logical_path, source_skin_resource_id: item.sourceSkinId, source_logical_path: item.asset.logical_path };
      } else return;
      const mode = await chooseWriteMode();
      if (!mode) return;
      const result = await desktopApi.executeSkinWorkshopAction(openedSkin.resource.resource_id, mode, action);
      await Promise.all([refreshWorkshop(), queryClient.invalidateQueries({ queryKey: ["local-skins"] })]);
      setActionError({ code: "MUTATION_COMPLETE", message: result.created_copy ? `已创建并修改 ${result.name}` : `已直接修改 ${result.name}` } satisfies CommandError);
      setStaged((current) => current.filter((value) => value.id !== item.id));
    } catch (error) { setActionError(error); }
    finally { setWorking(false); }
  };
  const migrateMania = async () => {
    if (!openedSkin || !maniaSource) return;
    const mode = await chooseWriteMode();
    if (!mode) return;
    setWorking(true); setActionError(null);
    try {
      const result = await desktopApi.executeSkinWorkshopPreset(openedSkin.resource.resource_id, mode, { type: "migrate_mania", source_skin_resource_id: maniaSource });
      await Promise.all([refreshWorkshop(), queryClient.invalidateQueries({ queryKey: ["local-skins"] })]);
      setActionError({ code: "MUTATION_COMPLETE", message: `Mania 模式已迁移到 ${result.name}` } satisfies CommandError);
    } catch (error) { setActionError(error); }
    finally { setWorking(false); }
  };

  if (skinsQuery.isLoading) return <><PageHeader title="Skin Workshop" /><Skeleton className="h-[650px] rounded-3xl" /></>;
  if (skinsQuery.error) return <><PageHeader title="Skin Workshop" /><ErrorPanel error={skinsQuery.error} onRetry={() => skinsQuery.refetch()} /></>;

  if (client === "lazer") {
    return <div><PageHeader title="Skin Workshop" />{openedSkinId ? <LazerReadOnly onSelect={setOpenedSkinId} selected={openedSkinId} skins={skins} /> : <div className="grid auto-rows-fr gap-4 md:grid-cols-2 xl:grid-cols-3">{skins.map((skin) => <SkinCover key={skin.resource.resource_id} onOpen={() => setOpenedSkinId(skin.resource.resource_id)} onReveal={() => void revealSkin(skin)} skin={skin} />)}</div>}</div>;
  }

  return (
    <div>
      <PageHeader title="Skin Workshop" actions={openedSkin ? <Button className="min-h-11 border-white/20 px-5 text-base" onClick={returnToLibrary} variant="primary"><ArrowLeft className="size-5" />返回全部皮肤</Button> : <Badge tone="cyan">选择 · 暂存 · 融合</Badge>} />
      {actionError ? <div className={cn("mb-4 rounded-xl border px-4 py-3 text-sm", (actionError as CommandError).code === "MUTATION_COMPLETE" ? "border-emerald-300/20 bg-emerald-400/10 text-emerald-100" : "border-rose-300/20 bg-rose-400/10 text-rose-100")}>{errorMessage(actionError)}</div> : null}
      <AnimatePresence mode="wait">
        {!openedSkin ? (
          <motion.section animate={{ opacity: 1 }} exit={{ opacity: 0 }} initial={{ opacity: 0 }} key="library" transition={transition}>
            <Card className="mb-4 overflow-hidden p-5"><div className="flex flex-col gap-4 lg:flex-row lg:items-center"><div className="min-w-0 flex-1"><p className="text-xs font-semibold text-[var(--theme-primary-light)]">Skin Workshop</p><h2 className="mt-1 text-2xl font-semibold tracking-tight text-white">选择要编辑的 Skin</h2><p className="mt-2 max-w-3xl text-sm leading-6 text-slate-400">从本地皮肤库选择，或打开一个 .osk 安装包。</p></div><Button loading={working} onClick={() => void openPackage()} variant="primary"><PackageOpen className="size-4" />打开 .osk</Button></div></Card>
            <div className="mb-5 flex items-center gap-3"><div className="relative min-w-0 flex-1"><Search className="absolute left-4 top-1/2 size-5 -translate-y-1/2 text-slate-600" /><input aria-label="搜索 Skin" className="w-full rounded-2xl border border-white/[0.08] bg-black/20 py-3.5 pl-12 pr-4 text-base text-white outline-none transition-colors duration-300 focus:border-[var(--theme-primary)]" onChange={(event) => setSearch(event.target.value)} placeholder="搜索名称、作者或版本" value={search} /></div><Button aria-label="重新扫描 Skin" disabled={working} onClick={() => void refreshLibrary()} size="icon" title="重新扫描 Skin"><RefreshCw className={cn("size-4", working && "animate-spin")} /></Button></div>
            {!skins.length ? <EmptyState icon={<Palette className="size-5" />} title="没有可用的 Stable Skin" description="请配置并扫描 Stable 目录，或者直接打开一个 .osk Skin 包。" /> : <div className="grid auto-rows-fr gap-5 md:grid-cols-2 xl:grid-cols-3">{skins.map((skin) => <SkinCover imported={skin.resource.resource_id.startsWith("package:skin:")} key={skin.resource.resource_id} onOpen={() => openSkin(skin)} onReveal={() => void revealSkin(skin)} skin={skin} />)}</div>}
          </motion.section>
        ) : (
          <motion.section animate={{ opacity: 1 }} exit={{ opacity: 0 }} initial={{ opacity: 0 }} key={openedSkin.resource.resource_id} transition={transition}>
            <Card className="mb-4 overflow-hidden p-5"><div className="flex items-start gap-4"><span className="grid size-11 shrink-0 place-items-center border-y border-pink-300/20 text-pink-200"><Palette className="size-5" /></span><div className="min-w-0 flex-1"><div className="flex flex-wrap gap-2"><Badge tone="cyan">浏览与组合</Badge>{openedSkin.resource.resource_id.startsWith("package:skin:") ? <Badge tone="warning">临时 OSK 只能新建副本</Badge> : null}</div><h2 className="mt-2 truncate text-2xl font-semibold tracking-tight text-white">{openedSkin.name}</h2><p className="mt-1 text-sm text-slate-400">{openedSkin.author} · {openedSkin.version}</p><p className="mt-2 text-xs text-slate-500">选择下方分类，查看并暂存想要组合的组件。</p></div></div></Card>
            {treeQuery.isLoading ? <Skeleton className="h-[520px] rounded-3xl" /> : treeQuery.error ? <ErrorPanel error={treeQuery.error} onRetry={() => treeQuery.refetch()} /> : showConfig ? <ConfigWorkspace client="stable" onApply={applyDirectAction} onStageEntry={stageConfig} skinId={openedSkin.resource.resource_id} /> : selectedPart ? <div><div className="mb-5 grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-4"><Button className="min-h-10 border-white/15" onClick={() => { setManualPartKey(null); setSelectedAssetId(null); }}><ArrowLeft className="size-4" />总体预览</Button><div><h3 className="text-2xl font-semibold text-white">{selectedPart.label}</h3></div><Button onClick={() => stagePart(selectedPart)}><FolderGit2 className="size-4" />暂存整个部分</Button></div>{autoPartKey && !manualPartKey ? <div className="mb-4 rounded-2xl border border-cyan-300/20 bg-cyan-300/[0.07] px-5 py-4 text-sm text-cyan-100">已根据暂存区自动定位到对应部分。即使目标中没有该部分或组件，也可以从右下角直接添加。</div> : null}<section className="mb-7"><div className="mb-3 flex items-center gap-2"><FileImage className="size-5 text-pink-200" /><h4 className="text-lg font-semibold text-white">图片组件</h4><Badge>{imageGroups.length}</Badge>{imageResolution.hidden ? <span className="text-xs text-slate-500">优先显示 @2x 高清贴图，已隐藏 {imageResolution.hidden} 个普通分辨率副本</span> : null}</div>{imageGroups.length ? <div className="grid auto-rows-fr gap-4 md:grid-cols-2 xl:grid-cols-3">{imageGroups.map((group) => group.length > 1 ? <AnimatedAssetVisual active={group.some((asset) => selectedAsset?.asset_id === asset.asset_id)} assets={group} client="stable" frameRate={frameRate} key={group[0].logical_path} onSelect={(asset) => setSelectedAssetId(asset.asset_id)} onStage={stageAsset} skinId={openedSkin.resource.resource_id} /> : <AssetVisual active={selectedAsset?.asset_id === group[0].asset_id} asset={group[0]} client="stable" key={group[0].asset_id} onSelect={() => setSelectedAssetId(group[0].asset_id)} onStage={() => stageAsset(group[0])} skinId={openedSkin.resource.resource_id} />)}</div> : <Card className="p-6 text-sm text-slate-500">这个部分没有图片资源。</Card>}</section><section><div className="mb-3 flex items-center gap-2"><AudioLines className="size-5 text-cyan-200" /><h4 className="text-lg font-semibold text-white">音效组件</h4><Badge>{sounds.length}</Badge></div>{sounds.length ? <div className="grid auto-rows-fr gap-4 md:grid-cols-2">{sounds.map((asset) => <AssetVisual active={selectedAsset?.asset_id === asset.asset_id} asset={asset} client="stable" key={asset.asset_id} onSelect={() => setSelectedAssetId(asset.asset_id)} onStage={() => stageAsset(asset)} skinId={openedSkin.resource.resource_id} />)}</div> : <Card className="p-6 text-sm text-slate-500">这个部分没有音效资源。</Card>}</section></div> : <div><div className="mb-5 flex items-end justify-between gap-5"><div><p className="text-sm font-semibold text-[var(--theme-primary-light)]">总体预览</p><h3 className="mt-1 text-2xl font-semibold text-white">选择一个组成</h3><p className="mt-2 text-base text-slate-400">打开分类查看组件，或将整组内容拖到右下角。</p></div><Button onClick={() => setShowConfig(true)}><Settings2 className="size-4" />皮肤配置</Button></div><Card className="mb-5 grid gap-4 p-5 lg:grid-cols-[minmax(0,1fr)_minmax(240px,340px)_auto] lg:items-center"><div><div className="flex items-center gap-2"><WandSparkles className="size-5 text-pink-200" /><h4 className="text-lg font-semibold text-white">快速迁移 Mania 模式</h4></div><p className="mt-2 text-sm text-slate-500">迁移全部 Mania 配置节，并跟随 NoteImage、KeyImage、Stage、Lighting 引用复制自定义命名贴图、@2x 与动画帧。</p></div><select aria-label="Mania 来源 Skin" className="rounded-xl border border-white/[0.08] bg-black/25 px-4 py-3 text-sm text-white outline-none focus:border-[var(--theme-primary)]" onChange={(event) => setManiaSource(event.target.value)} value={maniaSource}><option value="">选择来源 Skin</option>{skins.filter((skin) => skin.resource.resource_id !== openedSkin.resource.resource_id).map((skin) => <option key={skin.resource.resource_id} value={skin.resource.resource_id}>{skin.name}</option>)}</select><Button disabled={!maniaSource} loading={working} onClick={() => void migrateMania()} variant="primary">迁移 Mania</Button></Card><div className="grid auto-rows-fr gap-4 md:grid-cols-2 xl:grid-cols-3">{treeQuery.data?.roots.map((node) => <PartCard key={node.part_id} node={node} onSelect={(value) => { setManualPartKey(value.part_key); setSelectedAssetId(null); }} onStage={stagePart} selected={null} />)}</div></div>}
          </motion.section>
        )}
      </AnimatePresence>
      <StagingTray canApply={canApply} collapsed={trayCollapsed} items={staged} onApply={(item) => void applyStaged(item)} onDropItem={handleDrop} onRemove={(id) => setStaged((current) => current.filter((item) => item.id !== id))} onToggle={() => setTrayCollapsed((value) => !value)} />
      {writeDialog ? <WriteModeDialog copyName={copyName} imported={writeDialog.imported} onCancel={() => closeWriteDialog(null)} onChangeName={setCopyName} onCreateCopy={() => closeWriteDialog({ mode: "create_copy", name: copyName.trim() })} onOverwrite={() => closeWriteDialog({ mode: "overwrite" })} skinName={writeDialog.skinName} /> : null}
      {working ? <div className="fixed inset-0 z-[105] grid place-items-center bg-black/20 pointer-events-none"><LoaderCircle className="size-8 animate-spin text-[var(--theme-primary-light)]" /></div> : null}
    </div>
  );
}
