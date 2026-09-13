import { useEffect, useState } from "react";
import { FileCog, Info } from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { Badge, Card, SectionTitle } from "../../shared/components/ui";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { desktopApi, useCapabilities } from "../../shared/lib/tauri";
import type { DefaultFileClients, OsuClient } from "../../shared/types/osu";
import { BeatmapPreviewCard, DisplayGammaCard, FileAssociationCard, LazerDedupeCard, LazerDiskUsageCard, ManiaConverterCard, SpeedTestCard } from "./ToolsPage";
import { OtdPanel } from "./OtdPanel";
import { TosuPage } from "./TosuPage";

export function ToolPageHeader({ title, description }: { title: string; description: string }) {
  return <PageHeader eyebrow="Tools" title={title} description={description} actions={<Badge tone="cyan">工具集合</Badge>} />;
}

export function GameToolsPage() {
  return <><ToolPageHeader title="游戏与设备" description="测试输入设备并管理 OpenTabletDriver。" /><div className="space-y-5"><SpeedTestCard /><OtdPanel /></div></>;
}

export function BeatmapToolsPage() {
  return <><ToolPageHeader title="谱面与预览" description="生成谱面预览并转换外部谱面格式。" /><div className="space-y-5"><BeatmapPreviewCard /><ManiaConverterCard /></div></>;
}

export function SystemToolsPage() {
  const { client } = useMode();
  const capabilities = useCapabilities();
  const [defaults, setDefaults] = useState<DefaultFileClients>({ beatmap: client, skin: client });
  const [error, setError] = useState<unknown>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState<"beatmap" | "skin" | null>(null);
  useEffect(() => { void desktopApi.getDefaultFileClients().then(setDefaults).catch(setError); }, []);
  const save = async (kind: "beatmap" | "skin", target: OsuClient) => { setSaving(kind); setNotice(null); setError(null); try { await desktopApi.setDefaultFileClient(kind, target); setDefaults((current) => ({ ...current, [kind]: target })); setNotice(`${kind === "beatmap" ? "谱面" : "Skin"} 默认打开端已设为 ${target === "stable" ? "Stable" : "Lazer"}`); } catch (caught) { setError(caught); } finally { setSaving(null); } };
  return <><ToolPageHeader title="系统与文件" description="管理文件关联、显示效果和 lazer 存储。" />{error ? <div className="mb-5"><ErrorPanel error={error} /></div> : null}{notice ? <div className="mb-5 rounded-xl border border-emerald-300/20 bg-emerald-300/[0.08] px-4 py-3 text-sm text-emerald-100">{notice}</div> : null}<div className="space-y-5">{capabilities.data?.file_association ? <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><FileCog className="size-5" /></div><SectionTitle title="文件默认打开端" description="设置 Windows 双击 .osz 和 .osk 文件时使用的客户端。" /></div><div className="mt-6 grid gap-4 lg:grid-cols-2"><FileAssociationCard title="谱面包文件 (.osz)" description="双击 .osz 文件时交给选中的客户端读取。" value={defaults.beatmap} saving={saving === "beatmap"} onSave={(target) => void save("beatmap", target)} /><FileAssociationCard title="Skin 文件 (.osk)" description="双击 .osk 文件时使用选中的客户端导入。" value={defaults.skin} saving={saving === "skin"} onSave={(target) => void save("skin", target)} /></div><div className="mt-5 flex items-start gap-2 rounded-xl border border-amber-300/15 bg-amber-300/[0.06] p-3 text-sm leading-5 text-amber-100"><Info className="mt-0.5 size-4 shrink-0" />请先在设置中确认 Stable 或 Lazer 的游戏目录。</div></Card> : null}{capabilities.data?.display_gamma ? <DisplayGammaCard /> : null}<LazerDiskUsageCard /><LazerDedupeCard /></div></>;
}

export function LiveToolsPage() {
  return <TosuPage />;
}
