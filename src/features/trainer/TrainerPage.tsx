import { useEffect, useMemo, useState } from "react";
import { FolderCheck, Gauge, Music4, SlidersHorizontal, Timer, WandSparkles } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";

import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, DataLine, EmptyState, SectionTitle } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { OsuClient, TrainerRequest } from "../../shared/types/osu";
import { useLocalBeatmapDetail } from "../local-analysis/api";

const TRAINER_DRAFT_KEY = "opp.trainer-draft.v1";

interface TrainerDraft {
  client: OsuClient;
  resourceId: string;
  rate: string;
  ar: string;
  od: string;
  cs: string;
  hp: string;
  minBpm: string;
  maxBpm: string;
  start: string;
  end: string;
  result: { directory: string; included_objects: number } | null;
}

function loadDraft(): TrainerDraft | null {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(TRAINER_DRAFT_KEY) ?? "null");
    if (!value || typeof value !== "object" || !("resourceId" in value) || typeof value.resourceId !== "string") return null;
    return value as TrainerDraft;
  } catch {
    return null;
  }
}

function numeric(value: string, fallback: number) {
  if (!value.trim()) return fallback;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function optionalMilliseconds(value: string) {
  const parsed = Number(value);
  return value.trim() && Number.isFinite(parsed) && parsed >= 0 ? parsed * 1000 : null;
}

function Field({ label, value, onChange, min, max, step = "0.1" }: { label: string; value: string; onChange: (value: string) => void; min?: number; max?: number; step?: string }) {
  return <label className="block text-xs text-slate-400"><span className="mb-1.5 block">{label}</span><input className="opp-input w-full" max={max} min={min} onChange={(event) => onChange(event.target.value)} step={step} type="number" value={value} /></label>;
}

export function TrainerPage() {
  const { client: activeClient } = useMode();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const savedDraft = useMemo(() => loadDraft(), []);
  const requestedClient = params.get("client") === "lazer" ? "lazer" : params.get("client") === "stable" ? "stable" : null;
  const requestedResourceId = params.get("resource");
  const draft = savedDraft && (!requestedResourceId || (savedDraft.resourceId === requestedResourceId && (!requestedClient || savedDraft.client === requestedClient))) ? savedDraft : null;
  const client = (requestedClient ?? draft?.client ?? activeClient) as OsuClient;
  const resourceId = requestedResourceId ?? draft?.resourceId ?? "";
  const detail = useLocalBeatmapDetail(client, resourceId || null);
  const map = detail.data?.summary;
  const [rate, setRate] = useState(() => draft?.rate ?? "1");
  const [ar, setAr] = useState(() => draft?.ar ?? "");
  const [od, setOd] = useState(() => draft?.od ?? "");
  const [cs, setCs] = useState(() => draft?.cs ?? "");
  const [hp, setHp] = useState(() => draft?.hp ?? "");
  const [minBpm, setMinBpm] = useState(() => draft?.minBpm ?? "");
  const [maxBpm, setMaxBpm] = useState(() => draft?.maxBpm ?? "");
  const [start, setStart] = useState(() => draft?.start ?? "");
  const [end, setEnd] = useState(() => draft?.end ?? "");
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [result, setResult] = useState<{ directory: string; included_objects: number } | null>(() => draft?.result ?? null);

  const values = useMemo(() => ({
    rate: numeric(rate, 1), ar: numeric(ar, map?.ar ?? 5), od: numeric(od, map?.od ?? 5), cs: numeric(cs, map?.cs ?? 4), hp: numeric(hp, map?.hp ?? 5),
  }), [ar, cs, hp, map?.ar, map?.cs, map?.hp, map?.od, od, rate]);

  useEffect(() => {
    if (!resourceId) return;
    const next: TrainerDraft = { client, resourceId, rate, ar, od, cs, hp, minBpm, maxBpm, start, end, result };
    try { sessionStorage.setItem(TRAINER_DRAFT_KEY, JSON.stringify(next)); } catch { /* Session storage is optional. */ }
  }, [ar, client, cs, end, hp, maxBpm, minBpm, od, rate, resourceId, result, start]);

  const generate = async () => {
    if (!map) return;
    setGenerating(true); setError(null); setResult(null);
    try {
      const request: TrainerRequest = { client, resource_id: resourceId, ...values, min_bpm: minBpm.trim() ? numeric(minBpm, 0) : null, max_bpm: maxBpm.trim() ? numeric(maxBpm, 0) : null, start_time_ms: optionalMilliseconds(start), end_time_ms: optionalMilliseconds(end) };
      const generated = await desktopApi.generateTrainerBeatmap(request);
      setResult(generated);
    } catch (caught) { setError(caught); } finally { setGenerating(false); }
  };

  if (!resourceId) return <><PageHeader eyebrow="Core feature" title="谱面练习生成器" description="从本地谱面难度一键带入，再生成可直接被 osu! Songs 识别的训练副本。" /><EmptyState action={<Button onClick={() => navigate("/local/maps")}><Music4 className="size-4" />前往本地谱面</Button>} icon={<WandSparkles className="size-6" />} title="先选择一个本地难度" description="在“本地谱面”展开任意难度，点击“导入 Trainer”即可开始。" /></>;
  if (detail.isLoading) return <><PageHeader eyebrow="Core feature" title="谱面练习生成器" description="正在读取谱面参数…" /><Card className="h-64 animate-pulse" /></>;
  if (!map || detail.error) return <><PageHeader eyebrow="Core feature" title="谱面练习生成器" description="无法读取要训练的谱面。" />{detail.error ? <ErrorPanel error={detail.error} /> : null}</>;

  return <><PageHeader eyebrow="Core feature · osu!Trainer" title="谱面练习生成器" description="调节速度、难度、BPM 区间和时间段；生成结果会放入该本地库对应的 Songs 目录。" actions={<Badge tone="pink"><WandSparkles className="size-3.5" />Trainer</Badge>} />
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]"><div className="space-y-5"><Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 place-items-center rounded-2xl border border-pink-300/20 bg-pink-300/10 text-pink-100"><Music4 className="size-5" /></div><SectionTitle title={map.title_unicode || map.title} description={`${map.artist_unicode || map.artist} · [${map.difficulty_name}]`} /></div><div className="mt-5 grid grid-cols-2 gap-3 sm:grid-cols-4"><DataLine label="BPM" value={map.bpm.toFixed(0)} /><DataLine label="AR / OD" value={`${map.ar.toFixed(1)} / ${map.od.toFixed(1)}`} /><DataLine label="CS / HP" value={`${map.cs.toFixed(1)} / ${map.hp.toFixed(1)}`} /><DataLine label="Objects" value={map.object_count.toLocaleString()} /></div></Card>
      <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 place-items-center rounded-2xl border border-cyan-300/20 bg-cyan-300/10 text-cyan-100"><Gauge className="size-5" /></div><SectionTitle title="速度与难度" description="Rate 会重采样音频并同步谱面时间轴；变速或截取时首次使用需要系统已安装 ffmpeg。" /></div><div className="mt-6 grid gap-4 sm:grid-cols-2 lg:grid-cols-5"><Field label="Rate (0.75–2.00×)" max={2} min={0.75} onChange={setRate} step="0.01" value={rate} /><Field label={`AR（原 ${map.ar.toFixed(1)}）`} max={10} min={0} onChange={setAr} value={ar} /><Field label={`OD（原 ${map.od.toFixed(1)}）`} max={10} min={0} onChange={setOd} value={od} />{map.ruleset === "mania" ? <div className="block text-xs text-slate-400"><span className="mb-1.5 block">键数（固定）</span><div className="opp-input flex items-center text-slate-300">{map.cs.toFixed(0)}K（保持原谱）</div></div> : <Field label={`CS（原 ${map.cs.toFixed(1)}）`} max={10} min={0} onChange={setCs} value={cs} />}<Field label={`HP（原 ${map.hp.toFixed(1)}）`} max={10} min={0} onChange={setHp} value={hp} /></div></Card>
      <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 place-items-center rounded-2xl border border-violet-300/20 bg-violet-300/10 text-violet-100"><SlidersHorizontal className="size-5" /></div><SectionTitle title="区间训练" description="BPM 只保留落在指定速度区间的物件；时间段按源音频秒数截取。留空即不限制。" /></div><div className="mt-6 grid gap-4 sm:grid-cols-2"><Field label="最低 BPM" min={1} onChange={setMinBpm} value={minBpm} /><Field label="最高 BPM" min={1} onChange={setMaxBpm} value={maxBpm} /><Field label="开始时间（秒）" min={0} onChange={setStart} value={start} /><Field label="结束时间（秒）" min={0} onChange={setEnd} value={end} /></div></Card></div>
      <aside className="space-y-5"><Card className="sticky top-[calc(var(--titlebar-height)+12px)] p-6"><div className="flex items-start gap-3"><Timer className="mt-0.5 size-5 text-[var(--theme-primary)]" /><div><h2 className="font-semibold text-white">生成训练谱面</h2><p className="mt-1 text-xs leading-5 text-slate-500">原谱面不会修改。新副本仅包含当前难度，并会写入 Songs 中独立文件夹。</p></div></div><Button className="mt-6 w-full" disabled={generating} loading={generating} onClick={() => void generate()} variant="primary"><WandSparkles className="size-4" />一键生成到 Songs</Button>{result ? <div className="mt-4 rounded-xl border border-emerald-300/20 bg-emerald-300/[0.07] p-4 text-sm text-emerald-100"><div className="flex items-center gap-2 font-semibold"><FolderCheck className="size-4" />已生成 {result.included_objects} 个物件</div><p className="mt-2 break-all text-xs leading-5 text-emerald-100/75">{result.directory}</p></div> : null}{error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}</Card></aside></div></>;
}
