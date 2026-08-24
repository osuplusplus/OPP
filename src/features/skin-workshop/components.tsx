import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  AudioLines,
  ChevronDown,
  ChevronRight,
  CircleOff,
  Code2,
  FileImage,
  FolderGit2,
  LoaderCircle,
  Layers3,
  Play,
  Save,
  Settings2,
  X,
} from "lucide-react";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { Badge, Button, Card, EmptyState, Skeleton } from "../../shared/components/ui";
import { cn } from "../../shared/lib/cn";
import type {
  LocalSkinSummary,
  SkinAssetVariant,
  SkinConfigDocument,
  SkinTreeNode,
  SkinWorkshopAction,
} from "../../shared/types/osu";
import { useLocalSkinDetail } from "../local-analysis/api";
import { useWorkshopAsset, useWorkshopConfig } from "./api";

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB"];
  let size = value / 1024;
  let unit = units[0];
  for (let index = 1; size >= 1024 && index < units.length; index += 1) {
    size /= 1024;
    unit = units[index];
  }
  return `${size.toFixed(size >= 10 ? 1 : 2)} ${unit}`;
}

export interface StagedItem {
  id: string;
  type: "part" | "asset" | "config";
  sourceSkinId: string;
  sourceSkinName: string;
  partKey: string;
  label: string;
  asset?: SkinAssetVariant;
  config?: { section: string; key: string; occurrence: number; value: string };
}

export function PartCard({ node, selected, onSelect, onStage }: {
  node: SkinTreeNode;
  selected: string | null;
  onSelect: (node: SkinTreeNode) => void;
  onStage: (node: SkinTreeNode) => void;
}) {
  return (
    <motion.article
      className={cn(
        "group relative overflow-hidden rounded-lg border bg-transparent transition-colors duration-300",
        selected === node.part_key ? "border-[var(--theme-primary)] bg-[var(--theme-primary-muted)]" : "border-white/[0.08] hover:border-white/20 hover:bg-white/[0.045]",
      )}
      draggable
      layout
      onDragStart={(event) => {
        const transfer = (event as unknown as React.DragEvent<HTMLElement>).dataTransfer;
        transfer.setData("application/x-opp-skin-part", node.part_key);
        transfer.effectAllowed = "copy";
      }}
    >
      <button className="w-full p-4 text-left" onClick={() => onSelect(node)} type="button">
        <div className="flex items-center gap-3">
          <span className="grid size-9 shrink-0 place-items-center border-y border-pink-300/20 text-pink-200"><Layers3 className="size-4" /></span>
          <div className="min-w-0 flex-1">
            <h3 className="truncate text-base font-semibold text-white">{node.label}</h3>
            <p className="mt-1 text-xs text-slate-500">{node.asset_count} 个资源</p>
          </div>
        </div>
        <div className="mt-3 flex gap-3 border-t border-white/[0.06] pt-2 text-xs text-slate-400">
          <span>{node.image_count} 图片</span>
          <span>{node.audio_count} 音效</span>
        </div>
      </button>
      <button aria-label={`暂存 ${node.label}`} className="absolute right-3 top-3 grid size-8 place-items-center rounded-lg bg-black/30 text-slate-400 opacity-0 transition-opacity duration-300 hover:text-white group-hover:opacity-100 focus:opacity-100" onClick={() => onStage(node)} type="button"><FolderGit2 className="size-4" /></button>
    </motion.article>
  );
}

export function AssetVisual({ asset, client, skinId, active, onSelect, onStage = () => undefined }: {
  asset: SkinAssetVariant;
  client: "stable" | "lazer";
  skinId: string;
  active: boolean;
  onSelect: () => void;
  onStage?: () => void;
}) {
  const [requested, setRequested] = useState(asset.kind === "image");
  const payload = useWorkshopAsset(client, skinId, asset.asset_id, requested);
  return (
    <motion.div
      className={cn(
        "group relative flex min-h-40 flex-col overflow-hidden rounded-lg border text-left transition-colors duration-300",
        active ? "border-[var(--theme-primary)] bg-[var(--theme-primary-muted)]" : "border-white/[0.07] bg-black/15 hover:border-white/15",
      )}
      draggable
      layout
      onDragStart={(event) => {
        const transfer = (event as unknown as React.DragEvent<HTMLElement>).dataTransfer;
        transfer.setData("application/x-opp-skin-asset", asset.asset_id);
        transfer.effectAllowed = "copy";
      }}
    >
      <button className="flex min-h-40 flex-1 flex-col text-left" onClick={() => { onSelect(); setRequested(true); }} type="button">
      <div className="grid min-h-28 w-full flex-1 place-items-center bg-[linear-gradient(45deg,rgba(255,255,255,.035)_25%,transparent_25%),linear-gradient(-45deg,rgba(255,255,255,.035)_25%,transparent_25%)] bg-[length:16px_16px] p-4">
        {payload.isLoading ? <LoaderCircle className="size-4 animate-spin text-slate-600" /> : payload.data?.kind === "image" ? (
          <img alt={asset.name} className="max-h-32 max-w-full object-contain" src={payload.data.data_url} />
        ) : payload.data?.kind === "audio" ? (
          <audio controls className="max-w-full" onClick={(event) => event.stopPropagation()} src={payload.data.data_url}><track kind="captions" /></audio>
        ) : asset.kind === "audio" ? <Play className="size-5 text-cyan-200" /> : <FileImage className="size-5 text-slate-600" />}
      </div>
      <div className="w-full border-t border-white/[0.06] px-3 py-2">
        <p className="truncate text-sm font-medium text-slate-100">{asset.name}</p>
        <p className="mt-1 text-xs text-slate-500">{asset.scale > 1 ? `${asset.scale}x · ` : ""}{asset.frame !== null ? `帧 ${asset.frame} · ` : ""}{formatBytes(asset.size)}</p>
      </div>
      </button>
      <button aria-label={`暂存 ${asset.name}`} className="absolute right-2 top-2 grid size-8 place-items-center rounded-lg bg-black/55 text-slate-300 opacity-0 backdrop-blur transition-opacity duration-300 hover:text-white group-hover:opacity-100 focus:opacity-100" onClick={onStage} type="button"><FolderGit2 className="size-4" /></button>
    </motion.div>
  );
}

export function AnimatedAssetVisual({ assets, frameRate, ...props }: {
  assets: SkinAssetVariant[];
  frameRate: number;
  client: "stable" | "lazer";
  skinId: string;
  active: boolean;
  onSelect: (asset: SkinAssetVariant) => void;
  onStage: (asset: SkinAssetVariant) => void;
}) {
  const reduceMotion = useReducedMotion();
  const [frameIndex, setFrameIndex] = useState(0);
  useEffect(() => {
    if (reduceMotion || assets.length < 2) return;
    const timer = window.setInterval(
      () => setFrameIndex((current) => (current + 1) % assets.length),
      Math.max(40, Math.round(1000 / frameRate)),
    );
    return () => window.clearInterval(timer);
  }, [assets.length, frameRate, reduceMotion]);
  const asset = assets[frameIndex] ?? assets[0];
  return (
    <div className="relative">
      <AssetVisual
        {...props}
        asset={asset}
        onSelect={() => props.onSelect(asset)}
        onStage={() => props.onStage(asset)}
      />
      <Badge className="pointer-events-none absolute bottom-12 right-2" tone="cyan">
        {assets.length} 帧 · {frameRate} FPS
      </Badge>
    </div>
  );
}

export function StagingTray({ items, collapsed, canApply, onApply, onDropItem, onRemove, onToggle }: {
  items: StagedItem[];
  collapsed: boolean;
  canApply: (item: StagedItem) => boolean;
  onApply: (item: StagedItem) => void;
  onDropItem: (type: "part" | "asset", id: string) => void;
  onRemove: (id: string) => void;
  onToggle: () => void;
}) {
  const reduceMotion = useReducedMotion();
  if (typeof document === "undefined") return null;
  return createPortal(
    <motion.aside
      animate={{ width: collapsed ? 238 : 390, height: collapsed ? 58 : Math.min(480, 238 + items.length * 76) }}
      className="fixed bottom-5 right-5 z-[1000] overflow-hidden rounded-lg border border-white/15 bg-[#111725]/95 shadow-[0_18px_48px_rgba(0,0,0,.42)] backdrop-blur-xl"
      onDragOver={(event) => { event.preventDefault(); event.dataTransfer.dropEffect = "copy"; }}
      onDrop={(event) => {
        event.preventDefault();
        const part = event.dataTransfer.getData("application/x-opp-skin-part");
        const asset = event.dataTransfer.getData("application/x-opp-skin-asset");
        if (part) onDropItem("part", part);
        else if (asset) onDropItem("asset", asset);
      }}
      transition={reduceMotion ? { duration: 0 } : { duration: 0.18, ease: "easeOut" }}
    >
      <button className="flex h-[58px] w-full items-center gap-3 px-4 text-left" onClick={onToggle} type="button">
        <span className="grid size-9 place-items-center border-y border-[var(--theme-primary-soft)] text-[var(--theme-primary-light)]"><FolderGit2 className="size-4" /></span>
        <div className="min-w-0 flex-1"><p className="text-sm font-semibold text-white">组件暂存区</p><p className="text-[10px] text-slate-500">拖到这里 · {items.length} 项</p></div>
        {collapsed ? <ChevronDown className="size-4 text-slate-400" /> : <ChevronRight className="size-4 rotate-90 text-slate-400" />}
      </button>
      <AnimatePresence initial={false}>
        {!collapsed ? <motion.div animate={{ opacity: 1 }} className="border-t border-white/[0.08]" exit={{ opacity: 0 }} initial={{ opacity: 0 }}>
          {!items.length ? <div className="flex min-h-36 flex-col items-center justify-center gap-2 px-6 py-6 text-center"><p className="text-sm font-medium text-slate-300">拖入部分、组件或配置项</p><p className="max-w-[280px] text-xs leading-5 text-slate-500">稍后可将这些内容应用到另一套 Skin。</p></div> : <div className="max-h-64 space-y-2 overflow-y-auto p-3">{items.map((item) => <div className="flex items-center gap-3 rounded-xl border border-white/[0.07] bg-white/[0.035] p-3" key={item.id}><span className="grid size-9 shrink-0 place-items-center rounded-lg bg-black/20 text-pink-200">{item.type === "part" ? <Layers3 className="size-4" /> : item.type === "config" ? <Settings2 className="size-4" /> : item.asset?.kind === "audio" ? <AudioLines className="size-4" /> : <FileImage className="size-4" />}</span><div className="min-w-0 flex-1"><p className="truncate text-sm font-medium text-slate-100">{item.label}</p><p className="mt-1 truncate text-[10px] text-slate-500">来自 {item.sourceSkinName}</p></div>{canApply(item) ? <Button onClick={() => onApply(item)} size="sm" variant="primary">{item.type === "config" ? "应用" : "添加/替换"}</Button> : <Badge>选择目标</Badge>}<button aria-label={`移除 ${item.label}`} className="grid size-7 place-items-center rounded-lg text-slate-600 hover:bg-white/[0.06] hover:text-white" onClick={() => onRemove(item.id)} type="button"><X className="size-3.5" /></button></div>)}</div>}
          <p className="border-t border-white/[0.06] px-4 py-2.5 text-[10px] text-slate-500">返回皮肤列表并打开目标 Skin，系统会自动定位对应部分。</p>
        </motion.div> : null}
      </AnimatePresence>
    </motion.aside>,
    document.body,
  );
}

export function ConfigWorkspace({ client, skinId, onStageEntry, onApply }: {
  client: "stable" | "lazer";
  skinId: string;
  onStageEntry?: (section: string, entry: SkinConfigDocument["sections"][number]["entries"][number]) => void;
  onApply: (action: SkinWorkshopAction) => Promise<void>;
}) {
  const [mode, setMode] = useState<"structured" | "source">("structured");
  const query = useWorkshopConfig(client, skinId, true);
  const [sourceOverride, setSourceOverride] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const apply = async (action: SkinWorkshopAction) => {
    setSaving(true); setError(null);
    try {
      await onApply(action);
      if (action.type === "update_config_source") setSourceOverride(null);
    }
    catch (value) { setError(value); }
    finally { setSaving(false); }
  };
  if (query.isLoading) return <Skeleton className="h-[480px]" />;
  if (query.error || !query.data) return <ErrorPanel error={query.error} onRetry={() => query.refetch()} />;
  const document: SkinConfigDocument = query.data;
  const source = sourceOverride ?? document.source;
  return (
    <Card className="min-h-[560px] overflow-hidden">
      <div className="flex items-center gap-2 border-b border-white/[0.07] px-5 py-3">
        <Settings2 className="size-4 text-cyan-200" />
        <h2 className="text-sm font-semibold text-white">skin.ini</h2>
        <Badge>{document.encoding} · {document.newline.toUpperCase()}</Badge>
        <div className="ml-auto flex gap-1">
          <Button onClick={() => setMode("structured")} size="sm" variant={mode === "structured" ? "primary" : "ghost"}><Settings2 className="size-3.5" />结构化</Button>
          <Button onClick={() => setMode("source")} size="sm" variant={mode === "source" ? "primary" : "ghost"}><Code2 className="size-3.5" />源码</Button>
        </div>
      </div>
      {document.errors.length ? <div className="border-b border-rose-300/15 bg-rose-400/10 px-5 py-3 text-xs text-rose-200">{document.errors.map((item) => `第 ${item.line} 行：${item.message}`).join("；")}</div> : null}
      {error ? <div className="p-4"><ErrorPanel error={error} onRetry={() => setError(null)} /></div> : null}
      {mode === "source" ? (
        <div className="p-4">
          <textarea className="min-h-[430px] w-full resize-y rounded-xl border border-white/10 bg-black/25 p-4 font-mono text-xs leading-6 text-slate-200 outline-none focus:border-[var(--theme-primary)]" onChange={(event) => setSourceOverride(event.target.value)} spellCheck={false} value={source} />
          <div className="mt-3 flex justify-end"><Button loading={saving} onClick={() => void apply({ type: "update_config_source", source })} variant="primary"><Save className="size-4" />应用源码</Button></div>
        </div>
      ) : (
        <div className="max-h-[560px] space-y-3 overflow-y-auto p-4">
          {document.errors.length ? <EmptyState icon={<CircleOff className="size-5" />} title="结构化编辑已暂停" description="请切换到源码模式修复语法错误。" /> : document.sections.map((section) => (
            <details className="rounded-xl border border-white/[0.07] bg-white/[0.02]" key={section.name} open={section.name === "General"}>
              <summary className="cursor-pointer px-4 py-3 text-xs font-semibold text-slate-200">[{section.name}] <span className="ml-2 text-slate-600">{section.entries.length}</span></summary>
              <div className="divide-y divide-white/[0.05] border-t border-white/[0.06] px-4">
                {section.entries.map((entry) => <ConfigEntryEditor entry={entry} key={`${entry.key}:${entry.occurrence}:${entry.value}`} onApply={(value) => apply({ type: "update_config_entry", section: section.name, key: entry.key, occurrence: entry.occurrence, value })} onStage={onStageEntry ? () => onStageEntry(section.name, entry) : undefined} saving={saving} />)}
              </div>
            </details>
          ))}
        </div>
      )}
    </Card>
  );
}

function ConfigEntryEditor({ entry, onApply, onStage, saving }: {
  entry: SkinConfigDocument["sections"][number]["entries"][number];
  onApply: (value: string) => Promise<void>;
  onStage?: () => void;
  saving: boolean;
}) {
  const [value, setValue] = useState(entry.value);
  return <div className="grid grid-cols-[150px_minmax(0,1fr)_auto] items-center gap-3 py-2.5"><span className="truncate font-mono text-[10px] text-slate-500">{entry.key}</span><input className="min-w-0 rounded-lg border border-white/[0.07] bg-black/20 px-3 py-2 text-xs text-slate-200 outline-none focus:border-[var(--theme-primary)]" onChange={(event) => setValue(event.target.value)} value={value} /><div className="flex gap-2">{onStage ? <Button onClick={onStage} size="sm"><FolderGit2 className="size-3.5" />暂存</Button> : null}<Button disabled={value === entry.value} loading={saving} onClick={() => void onApply(value)} size="sm">应用</Button></div></div>;
}

export function LazerReadOnly({ skins, selected, onSelect }: { skins: LocalSkinSummary[]; selected: string | null; onSelect: (id: string) => void }) {
  const detail = useLocalSkinDetail("lazer", selected);
  return <div className="grid grid-cols-[320px_minmax(0,1fr)] gap-4"><Card className="max-h-[calc(100vh-250px)] overflow-y-auto p-3">{skins.map((skin) => <button className={cn("mb-2 w-full rounded-xl border p-3 text-left", selected === skin.resource.resource_id ? "border-amber-300/30 bg-amber-300/10" : "border-white/[0.06] bg-white/[0.02]")} key={skin.resource.resource_id} onClick={() => onSelect(skin.resource.resource_id)} type="button"><p className="truncate text-sm font-semibold text-white">{skin.name}</p><p className="mt-1 text-[10px] text-slate-500">{skin.author} · {skin.version}</p></button>)}</Card><Card className="p-5"><Badge tone="warning">Lazer 只读</Badge><h2 className="mt-4 text-xl font-semibold text-white">{detail.data?.summary.name ?? "选择一套 Skin"}</h2><p className="mt-2 text-sm leading-6 text-slate-400">Lazer Realm 尚不能可靠确定资源归属，因此首版只展示可识别的 legacy skin.ini 配置，不开放融合和写入。</p><div className="mt-5 space-y-2">{detail.data?.sections.map((section) => <details className="rounded-xl border border-white/[0.07] p-3" key={section.name}><summary className="cursor-pointer text-xs font-semibold text-slate-200">[{section.name}]</summary><div className="mt-3 space-y-2">{section.entries.map((entry, index) => <div className="grid grid-cols-[140px_1fr] gap-3 text-xs" key={`${entry.key}:${index}`}><span className="font-mono text-slate-600">{entry.key}</span><span className="break-all text-slate-300">{entry.value}</span></div>)}</div></details>)}</div></Card></div>;
}

