import * as Dialog from "@radix-ui/react-dialog";
import { useState } from "react";
import {
  AreaChart,
  ChevronRight,
  FileArchive,
  Gauge,
  LoaderCircle,
  Palette,
  X,
} from "lucide-react";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { Badge, Card, DataLine } from "../../shared/components/ui";
import { dateTime, fullNumber } from "../../shared/lib/format";
import type { OsuClient } from "../../shared/types/osu";
import {
  useLocalBeatmapDetail,
  useLocalSkinDetail,
} from "./api";

const chartColors = ["#73e5ea", "#ff79ad", "#a98bff", "#f5c66f"];

function editorTimestamp(milliseconds: number) {
  const totalMilliseconds = Math.max(0, Math.round(milliseconds));
  const minutes = Math.floor(totalMilliseconds / 60_000);
  const seconds = Math.floor((totalMilliseconds % 60_000) / 1_000);
  const remainder = totalMilliseconds % 1_000;
  return `${minutes}:${String(seconds).padStart(2, "0")}:${String(remainder).padStart(3, "0")}`;
}

async function copyTimestamp(milliseconds: number) {
  const timestamp = editorTimestamp(milliseconds);
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(timestamp);
    return;
  }

  const field = document.createElement("textarea");
  field.value = timestamp;
  field.setAttribute("readonly", "");
  field.style.cssText = "position:fixed;opacity:0";
  document.body.append(field);
  field.select();
  document.execCommand("copy");
  field.remove();
}

function timestampFromChartLabel(value: unknown) {
  const timestamp = Number(value);
  return Number.isFinite(timestamp) ? timestamp : null;
}

function formatBytes(value?: number | null) {
  if (value === null || value === undefined) return "—";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = value / 1024;
  let unit = units[0];
  for (let index = 1; size >= 1024 && index < units.length; index += 1) {
    size /= 1024;
    unit = units[index];
  }
  return `${size.toFixed(size >= 10 ? 1 : 2)} ${unit}`;
}

function DrawerFrame({
  children,
  title,
  description,
}: {
  children: React.ReactNode;
  title: string;
  description: string;
}) {
  return (
    <Dialog.Portal>
      <Dialog.Overlay className="fixed inset-0 z-[80] bg-black/65 backdrop-blur-sm" />
      <Dialog.Content className="fixed left-1/2 top-1/2 z-[90] w-[min(720px,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto p-6 outline-none">
        <Dialog.Title className="pr-14 text-2xl font-semibold text-white">
          {title}
        </Dialog.Title>
        <Dialog.Description className="mt-2 pr-14 text-sm text-slate-400">
          {description}
        </Dialog.Description>
        <Dialog.Close
          aria-label="关闭详情"
          className="absolute right-5 top-5 grid size-10 place-items-center rounded-xl border border-white/10 bg-white/[0.04] text-slate-400 transition hover:bg-white/[0.08] hover:text-white"
        >
          <X className="size-4" />
        </Dialog.Close>
        <div className="mt-7">{children}</div>
      </Dialog.Content>
    </Dialog.Portal>
  );
}

export function BeatmapDetailDrawer({
  client,
  resourceId,
  onClose,
}: {
  client: OsuClient;
  resourceId: string | null;
  onClose: () => void;
}) {
  const query = useLocalBeatmapDetail(client, resourceId);
  const detail = query.data;
  const [hoveredStrainTime, setHoveredStrainTime] = useState<number | null>(null);
  const strainRows = (() => {
    const strains = detail?.strains;
    if (!strains) return [];
    const pointCount = Math.max(0, ...strains.series.map((series) => series.values.length));
    return Array.from({ length: pointCount }, (_, index) => {
      const row: Record<string, number> = {
        time:
          (strains.section_start_time_ms ?? strains.first_object_time_ms) +
          (index + 1) * strains.section_length_ms,
      };
      strains.series.forEach((series) => {
        row[series.key] = series.values[index] ?? 0;
      });
      return row;
    });
  })();

  return (
    <Dialog.Root onOpenChange={(open) => !open && onClose()} open={Boolean(resourceId)}>
      {resourceId ? (
        <DrawerFrame
          description={
            detail
              ? `${detail.summary.artist} · mapped by ${detail.summary.creator}`
              : "正在读取结构指标和完整 strain 序列"
          }
          title={
            detail
              ? `${detail.summary.title} [${detail.summary.difficulty_name}]`
              : "谱面详情"
          }
        >
          {query.isLoading ? (
            <div className="grid min-h-64 place-items-center text-sm text-slate-400">
              <LoaderCircle className="mb-3 size-6 animate-spin text-cyan-200" />
            </div>
          ) : query.error || !detail ? (
            <ErrorPanel error={query.error} onRetry={() => query.refetch()} />
          ) : (
            <div className="space-y-5">
              <div className="flex flex-wrap gap-2">
                <Badge tone="cyan">{detail.summary.ruleset}</Badge>
                  <DifficultyIcon mode={detail.summary.ruleset} stars={detail.summary.stars} />
                <Badge>
                  {detail.calculation.engine} {detail.calculation.engine_version} ·{" "}
                  {detail.calculation.engine_released_at}
                </Badge>
                <Badge>
                  {detail.calculation.upstream_repository}{" "}
                  {detail.calculation.upstream_revision.slice(0, 7)} ·{" "}
                  {detail.calculation.upstream_date}
                </Badge>
              </div>

              <Card className="border-violet-300/20 bg-violet-300/[0.035] p-5 shadow-[0_14px_40px_rgba(169,139,255,.08)]">
                <div className="mb-5 flex items-center justify-between">
                  <div>
                    <div className="flex items-center gap-2 text-sm font-semibold text-white">
                      <AreaChart className="size-4 text-violet-200" />
                      Strain 时间序列
                    </div>
                    <p className="mt-1 text-xs text-slate-500">
                      {hoveredStrainTime === null ? (
                        "悬停曲线后点击图表，即可复制编辑器时间戳。"
                      ) : (
                        <>
                          当前 <code className="font-mono text-cyan-200">{editorTimestamp(hoveredStrainTime)}</code>
                          <span className="ml-1">· 点击图表复制</span>
                        </>
                      )}
                    </p>
                  </div>
                  <span className="text-xs text-slate-500">
                    原生 section {(detail.strains?.section_length_ms ?? 0).toFixed(0)} ms
                  </span>
                </div>
                {strainRows.length ? (
                  <div
                    className="h-80 w-full cursor-crosshair"
                    onClick={() => {
                      if (hoveredStrainTime !== null) void copyTimestamp(hoveredStrainTime);
                    }}
                    onKeyDown={(event) => {
                      if ((event.key === "Enter" || event.key === " ") && hoveredStrainTime !== null) {
                        event.preventDefault();
                        void copyTimestamp(hoveredStrainTime);
                      }
                    }}
                    role="button"
                    tabIndex={0}
                    title="悬停曲线后点击复制时间戳"
                  >
                    <ResponsiveContainer height="100%" width="100%">
                      <LineChart
                        data={strainRows}
                        margin={{ left: 4, right: 12 }}
                        onMouseLeave={() => setHoveredStrainTime(null)}
                        onMouseMove={({ activeLabel }) => {
                          const timestamp = timestampFromChartLabel(activeLabel);
                          setHoveredStrainTime((current) => current === timestamp ? current : timestamp);
                        }}
                      >
                        <CartesianGrid stroke="rgba(255,255,255,.05)" vertical={false} />
                        <XAxis
                          dataKey="time"
                          domain={["dataMin", "dataMax"]}
                          minTickGap={56}
                          stroke="#586174"
                          tickFormatter={(value) => editorTimestamp(Number(value))}
                          tickLine={false}
                          type="number"
                        />
                        <YAxis stroke="#586174" tickLine={false} width={38} />
                        <Tooltip
                          contentStyle={{
                            background: "#111927",
                            border: "1px solid rgba(255,255,255,.1)",
                            borderRadius: 12,
                          }}
                          labelFormatter={(value) => editorTimestamp(Number(value))}
                          wrapperStyle={{ pointerEvents: "none" }}
                        />
                        {detail.strains?.series.map((series, index) => (
                          <Line
                            dataKey={series.key}
                            dot={false}
                            key={series.key}
                            name={series.key}
                            stroke={chartColors[index % chartColors.length]}
                            strokeWidth={1.7}
                            type="monotone"
                          />
                        ))}
                      </LineChart>
                    </ResponsiveContainer>
                  </div>
                ) : (
                  <p className="py-16 text-center text-sm text-slate-500">
                    该谱面没有可绘制的 strain 数据
                  </p>
                )}
              </Card>

              <div className="grid grid-cols-2 gap-5">
                <Card className="p-5">
                  <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-white">
                    <Gauge className="size-4 text-cyan-200" />
                    难度与密度
                  </div>
                  <DataLine label="CS / AR" value={`${detail.cs.toFixed(1)} / ${detail.ar.toFixed(1)}`} />
                  <DataLine label="OD / HP" value={`${detail.od.toFixed(1)} / ${detail.hp.toFixed(1)}`} />
                  <DataLine label="BPM" value={detail.summary.bpm.toFixed(2)} />
                  <DataLine label="平均 NPS" value={detail.average_nps.toFixed(2)} />
                  <DataLine label="1 秒峰值 NPS" value={detail.peak_nps.toFixed(2)} />
                  <DataLine
                    label="NoMod 满分 PP"
                    value={
                      detail.summary.max_pp === null
                        ? "—"
                        : `${detail.summary.max_pp.toFixed(2)} pp`
                    }
                  />
                  <DataLine label="最大连击" value={fullNumber(detail.summary.max_combo)} />
                </Card>
                <Card className="p-5">
                  <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-white">
                    <FileArchive className="size-4 text-pink-200" />
                    结构
                  </div>
                  <DataLine label="物件总数" value={fullNumber(detail.hit_objects.total)} />
                  <DataLine label="Circle / Slider" value={`${detail.hit_objects.circles} / ${detail.hit_objects.sliders}`} />
                  <DataLine label="Spinner / Hold" value={`${detail.hit_objects.spinners} / ${detail.hit_objects.holds}`} />
                  <DataLine label="Timing 点" value={detail.timing_point_count} />
                  <DataLine label="Break" value={`${detail.break_count} · ${(detail.break_duration_ms / 1000).toFixed(1)} 秒`} />
                  <DataLine label="格式版本" value={`v${detail.summary.format_version}`} />
                </Card>
              </div>

              <Card className="p-5">
                <DataLine label="谱面 ID" value={detail.summary.beatmap_id ?? "未提交"} />
                <DataLine label="谱面集 ID" value={detail.summary.beatmap_set_id ?? "未提交"} />
                <DataLine label="音频引用" value={detail.audio_file || "—"} />
                <DataLine label="背景引用" value={detail.background_file || "—"} />
                <DataLine label="修改时间" value={dateTime(detail.summary.modified_at)} />
                <DataLine
                  label="规则集算法版本"
                  value={
                    detail.calculation.ruleset_versions[detail.summary.ruleset] ?? "—"
                  }
                />
                <DataLine
                  label="计算条件"
                  value={`${detail.calculation.modifiers} · ${detail.calculation.performance_assumption}`}
                />
                <DataLine label="计算时间" value={dateTime(detail.calculated_at)} />
                <DataLine label="内容哈希" value={<code className="text-[11px]">{detail.summary.resource.content_hash}</code>} />
              </Card>
            </div>
          )}
        </DrawerFrame>
      ) : null}
    </Dialog.Root>
  );
}

export function SkinDetailDrawer({
  client,
  resourceId,
  onClose,
}: {
  client: OsuClient;
  resourceId: string | null;
  onClose: () => void;
}) {
  const query = useLocalSkinDetail(client, resourceId);
  const detail = query.data;

  return (
    <Dialog.Root onOpenChange={(open) => !open && onClose()} open={Boolean(resourceId)}>
      {resourceId ? (
        <DrawerFrame
          description={
            detail
              ? `${detail.summary.author} · ${detail.summary.version}`
              : "正在读取 Skin 配置与资源盘点"
          }
          title={detail?.summary.name ?? "Skin 详情"}
        >
          {query.isLoading ? (
            <div className="grid min-h-64 place-items-center">
              <LoaderCircle className="size-6 animate-spin text-cyan-200" />
            </div>
          ) : query.error || !detail ? (
            <ErrorPanel error={query.error} onRetry={() => query.refetch()} />
          ) : (
            <div className="space-y-5">
              {detail.notice ? (
                <div className="rounded-2xl border border-amber-300/15 bg-amber-300/[0.07] p-4 text-sm leading-6 text-amber-100">
                  {detail.notice}
                </div>
              ) : null}
              <div className="grid grid-cols-2 gap-5">
                <Card className="p-5">
                  <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-white">
                    <Palette className="size-4 text-pink-200" />
                    配置概览
                  </div>
                  <DataLine label="完整度" value={detail.summary.completeness === "complete" ? "完整" : "部分"} />
                  <DataLine label="Section" value={detail.summary.section_count} />
                  <DataLine label="Mania 配置" value={detail.summary.has_mania_config ? "有" : "无"} />
                  <DataLine label="修改时间" value={dateTime(detail.summary.modified_at)} />
                </Card>
                <Card className="p-5">
                  <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-white">
                    <FileArchive className="size-4 text-cyan-200" />
                    资源盘点
                  </div>
                  <DataLine label="递归文件数" value={detail.inventory ? fullNumber(detail.inventory.file_count) : "不可确定"} />
                  <DataLine label="总大小" value={detail.inventory ? formatBytes(detail.inventory.total_bytes) : "不可确定"} />
                  <DataLine label="扩展名种类" value={detail.inventory ? Object.keys(detail.inventory.by_extension).length : "不可确定"} />
                  <DataLine label="资源归属" value={detail.inventory ? "stable 文件夹" : "Realm 未读取"} />
                </Card>
              </div>

              {detail.inventory ? (
                <Card className="p-5">
                  <h3 className="text-sm font-semibold text-white">扩展名分布</h3>
                  <div className="mt-4 flex flex-wrap gap-2">
                    {Object.entries(detail.inventory.by_extension)
                      .sort((left, right) => right[1] - left[1])
                      .map(([extension, count]) => (
                        <Badge key={extension}>
                          .{extension} · {count}
                        </Badge>
                      ))}
                  </div>
                </Card>
              ) : null}

              <div className="space-y-3">
                {detail.sections.map((section, sectionIndex) => (
                  <Card className="overflow-hidden" key={`${section.name}-${sectionIndex}`}>
                    <div className="flex items-center gap-2 border-b border-white/[0.06] px-5 py-3.5">
                      <ChevronRight className="size-3.5 text-cyan-200" />
                      <h3 className="text-sm font-semibold text-white">[{section.name}]</h3>
                      <span className="ml-auto text-xs text-slate-600">{section.entries.length} 项</span>
                    </div>
                    <div className="divide-y divide-white/[0.045] px-5">
                      {section.entries.map((entry, entryIndex) => (
                        <div
                          className="grid grid-cols-[minmax(130px,.45fr)_1fr] gap-5 py-2.5 text-xs"
                          key={`${entry.key}-${entryIndex}`}
                        >
                          <span className="font-mono text-slate-500">{entry.key}</span>
                          <span className="flex min-w-0 items-center gap-2 break-all text-slate-300">
                            {entry.color ? (
                              <span
                                className="size-4 shrink-0 rounded border border-white/20"
                                style={{
                                  backgroundColor: `rgb(${entry.color.slice(0, 3).join(",")})`,
                                }}
                              />
                            ) : null}
                            {entry.value}
                          </span>
                        </div>
                      ))}
                    </div>
                  </Card>
                ))}
              </div>
            </div>
          )}
        </DrawerFrame>
      ) : null}
    </Dialog.Root>
  );
}
