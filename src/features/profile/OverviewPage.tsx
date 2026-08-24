import { useMemo, useState } from "react";
import {
  Activity,
  Clock3,
  Crosshair,
  Crown,
  RefreshCw,
  ShieldAlert,
  Target,
  TrendingDown,
  TrendingUp,
  Trophy,
  WifiOff,
  type LucideIcon,
} from "lucide-react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import {
  Badge,
  Button,
  Card,
  SectionTitle,
  Skeleton,
} from "../../shared/components/ui";
import {
  compactNumber,
  dateOnly,
  duration,
  fullNumber,
  percent,
} from "../../shared/lib/format";
import type { UserStatistics } from "../../shared/types/osu";
import { useOwnProfile } from "./api";
import { buildRankHistory, calculateRankTrend } from "./rankHistory";
import { selectedStatistics } from "./statistics";

function StatCard({
  icon: Icon,
  label,
  value,
  helper,
  tone = "pink",
}: {
  icon: typeof Trophy;
  label: string;
  value: string;
  helper?: string;
  tone?: "pink" | "cyan" | "purple" | "green";
}) {
  const tones = {
    pink: "text-pink-200",
    cyan: "text-cyan-100",
    purple: "text-violet-200",
    green: "text-emerald-200",
  };
  return (
    <Card className={`opp-metric-cell relative overflow-hidden p-4 ${tones[tone]}`} unstyled>
      <div className="flex items-start justify-between">
        <div>
          <p className="text-xs text-slate-500">{label}</p>
          <p className="metric-value mt-2 text-xl tracking-tight text-white">
            {value}
          </p>
          {helper ? (
            <p className="mt-1.5 text-[11px] text-slate-600">{helper}</p>
          ) : null}
        </div>
        <Icon className="mt-0.5 size-4 opacity-75" />
      </div>
    </Card>
  );
}

function ChartTooltip({
  active,
  payload,
  label,
  suffix = "",
}: {
  active?: boolean;
  payload?: Array<{ value?: number }>;
  label?: string;
  suffix?: string;
}) {
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded-xl border border-white/10 bg-[#0c111d]/95 px-3 py-2 shadow-2xl">
      <p className="text-[10px] text-slate-500">{label}</p>
      <p className="mt-1 font-mono text-xs font-semibold text-white">
        {fullNumber(payload[0]?.value)}
        {suffix}
      </p>
    </div>
  );
}

function OverviewSkeleton() {
  return (
    <div className="space-y-6">
      <div className="opp-metric-strip grid grid-cols-2 xl:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => (
          <Skeleton className="h-32" key={index} />
        ))}
      </div>
      <div className="grid grid-cols-2 gap-5">
        <Skeleton className="h-80" />
        <Skeleton className="h-80" />
      </div>
    </div>
  );
}

export function OverviewPage() {
  const { ruleset } = useMode();
  const profileQuery = useOwnProfile(ruleset);
  const [refreshing, setRefreshing] = useState(false);
  const profile = profileQuery.data?.data;

  const statistics = useMemo<UserStatistics>(
    () => selectedStatistics(profile, ruleset),
    [profile, ruleset],
  );

  const rankHistory = useMemo(
    () => profile?.rank_history?.data ?? [],
    [profile],
  );
  const rankData = useMemo(() => buildRankHistory(rankHistory), [rankHistory]);
  const rankTrend = useMemo(
    () => calculateRankTrend(rankHistory),
    [rankHistory],
  );

  const monthlyData = useMemo(
    () =>
      (profile?.monthly_playcounts ?? []).map((entry) => ({
        label: entry.start_date?.slice(0, 7) ?? "",
        count: entry.count ?? 0,
      })),
    [profile],
  );

  if (profileQuery.isLoading) {
    return (
      <>
        <PageHeader
          description="正在安全读取 osu! API v2 中的个人资料。"
          eyebrow="Online profile"
          title="个人概览"
        />
        <OverviewSkeleton />
      </>
    );
  }
  if (profileQuery.error || !profile) {
    return (
      <>
        <PageHeader
          description="你的主要游玩指标、排名趋势与活跃记录。"
          eyebrow="Online profile"
          title="个人概览"
        />
        <ErrorPanel error={profileQuery.error} onRetry={() => profileQuery.refetch()} />
      </>
    );
  }

  const refresh = async () => {
    setRefreshing(true);
    try {
      await profileQuery.refresh();
    } finally {
      setRefreshing(false);
    }
  };

  const grades = statistics.grade_counts ?? {};
  const gradeItems = [
    ["SSH", grades.ssh ?? 0, "text-slate-100"],
    ["SS", grades.ss ?? 0, "text-yellow-200"],
    ["SH", grades.sh ?? 0, "text-slate-200"],
    ["S", grades.s ?? 0, "text-yellow-300"],
    ["A", grades.a ?? 0, "text-emerald-300"],
  ];
  const maxGrade = Math.max(...gradeItems.map((item) => Number(item[1])), 1);
  const lifetimeMetrics: Array<{
    icon: LucideIcon;
    label: string;
    value: string;
  }> = [
    { icon: Clock3, label: "游玩时长", value: duration(statistics.play_time) },
    { icon: Crosshair, label: "总命中数", value: fullNumber(statistics.total_hits) },
    { icon: Trophy, label: "最大连击", value: `${fullNumber(statistics.maximum_combo)}x` },
    { icon: Activity, label: "总分", value: fullNumber(statistics.total_score) },
  ];

  return (
    <>
      <PageHeader
        actions={
          <Button loading={refreshing} onClick={refresh} size="icon" title="刷新资料">
            <RefreshCw className="size-4" />
          </Button>
        }
        description="集中查看当前模式的表现、排名轨迹与长期活跃度。"
        eyebrow="Online profile"
        title="个人概览"
      />

      {profileQuery.data?.stale ? (
        <div className="mb-5 flex items-center gap-3 rounded-xl border border-amber-300/15 bg-amber-300/[0.07] px-4 py-3 text-xs text-amber-100">
          <WifiOff className="size-4" />
          当前展示的是 {dateOnly(profileQuery.data?.fetched_at)} 缓存数据，网络恢复后可手动刷新。
        </div>
      ) : null}
      {profile.is_restricted ? (
        <div className="mb-5 flex items-center gap-3 rounded-xl border border-rose-300/15 bg-rose-300/[0.07] px-4 py-3 text-xs text-rose-100">
          <ShieldAlert className="size-4" />
          osu! 将此账号标记为受限状态，部分公开数据可能缺失。
        </div>
      ) : null}

      <div className="grid grid-cols-4 gap-4">
        <StatCard
          icon={Trophy}
          label="PP"
          tone="pink"
          value={statistics.pp?.toFixed(2) ?? "—"}
        />
        <StatCard
          helper={`地区排名 #${fullNumber(statistics.country_rank)}`}
          icon={Crown}
          label="全球排名"
          tone="purple"
          value={`#${fullNumber(statistics.global_rank)}`}
        />
        <StatCard
          helper={`${fullNumber(statistics.play_count)} 次游玩`}
          icon={Target}
          label="准确率"
          tone="cyan"
          value={percent(statistics.hit_accuracy)}
        />
        <StatCard
          helper={`${statistics.level?.progress?.toFixed(0) ?? "—"}% 至下一级`}
          icon={Activity}
          label="等级"
          tone="green"
          value={statistics.level?.current?.toFixed(0) ?? "—"}
        />
      </div>

      <div className="mt-5 grid grid-cols-[1.25fr_.75fr] gap-5">
        <Card className="p-6">
          <div className="flex items-start justify-between">
            <SectionTitle
              eyebrow="Rank history"
              title="排名轨迹"
            />
            <div className="flex items-center gap-2">
              {rankTrend.direction !== "flat" ? (
                <Badge tone={rankTrend.direction === "up" ? "success" : "warning"}>
                  {rankTrend.direction === "up" ? (
                    <TrendingUp className="size-3.5" />
                  ) : (
                    <TrendingDown className="size-3.5" />
                  )}
                  {rankTrend.direction === "up" ? "上升" : "下降"}{" "}
                  {fullNumber(rankTrend.amount)} 名
                </Badge>
              ) : null}
              <Badge tone="cyan">{rankData.length} 个采样点</Badge>
            </div>
          </div>
          <div className="mt-5 h-64">
            {rankData.length ? (
              <ResponsiveContainer height="100%" width="100%">
                <LineChart data={rankData}>
                  <defs>
                    <linearGradient id="rankLine" x1="0" x2="1">
                      <stop offset="0%" stopColor="#ff6aa7" />
                      <stop offset="100%" stopColor="#5ce1e6" />
                    </linearGradient>
                  </defs>
                  <CartesianGrid stroke="rgba(255,255,255,.045)" vertical={false} />
                  <XAxis
                    axisLine={false}
                    dataKey="label"
                    minTickGap={44}
                    tick={{ fill: "#596177", fontSize: 10 }}
                    tickLine={false}
                  />
                  <YAxis
                    axisLine={false}
                    domain={["dataMin", "dataMax"]}
                    reversed
                    tick={{ fill: "#596177", fontSize: 10 }}
                    tickFormatter={(value) => `#${compactNumber(value)}`}
                    tickLine={false}
                    width={58}
                  />
                  <Tooltip content={<ChartTooltip />} cursor={{ stroke: "#ffffff22" }} />
                  <Line
                    activeDot={{ fill: "#5ce1e6", r: 4, strokeWidth: 0 }}
                    dataKey="rank"
                    dot={false}
                    stroke="url(#rankLine)"
                    strokeWidth={2.5}
                    type="monotone"
                  />
                </LineChart>
              </ResponsiveContainer>
            ) : (
              <div className="grid h-full place-items-center text-sm text-slate-600">
                当前模式没有排名历史
              </div>
            )}
          </div>
        </Card>

        <Card className="p-6">
          <SectionTitle
            eyebrow="Grade mix"
            title="评价分布"
          />
          <div className="mt-6 space-y-4">
            {gradeItems.map(([label, count, tone]) => (
              <div key={String(label)}>
                <div className="mb-2 flex items-center justify-between">
                  <span className={`font-mono text-xs font-bold ${tone}`}>{label}</span>
                  <span className="font-mono text-xs text-slate-400">
                    {fullNumber(Number(count))}
                  </span>
                </div>
                <div className="h-1.5 overflow-hidden rounded-full bg-white/[0.05]">
                  <div
                          className="h-full rounded-full bg-[var(--theme-primary)]"
                    style={{ width: `${Math.max((Number(count) / maxGrade) * 100, 1.5)}%` }}
                  />
                </div>
              </div>
            ))}
          </div>
        </Card>
      </div>

      <div className="mt-5 grid grid-cols-[1.25fr_.75fr] gap-5">
        <Card className="p-6">
          <SectionTitle
            eyebrow="Monthly activity"
            title="长期活跃度"
          />
          <div className="mt-5 h-60">
            {monthlyData.length ? (
              <ResponsiveContainer height="100%" width="100%">
                <AreaChart data={monthlyData}>
                  <defs>
                    <linearGradient id="activityFill" x1="0" x2="0" y1="0" y2="1">
                      <stop offset="0%" stopColor="#5ce1e6" stopOpacity={0.28} />
                      <stop offset="100%" stopColor="#5ce1e6" stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid stroke="rgba(255,255,255,.045)" vertical={false} />
                  <XAxis
                    axisLine={false}
                    dataKey="label"
                    minTickGap={40}
                    tick={{ fill: "#596177", fontSize: 10 }}
                    tickLine={false}
                  />
                  <YAxis
                    axisLine={false}
                    tick={{ fill: "#596177", fontSize: 10 }}
                    tickLine={false}
                    width={40}
                  />
                  <Tooltip content={<ChartTooltip suffix=" 次" />} />
                  <Area
                    dataKey="count"
                    fill="url(#activityFill)"
                    stroke="#5ce1e6"
                    strokeWidth={2}
                    type="monotone"
                  />
                </AreaChart>
              </ResponsiveContainer>
            ) : (
              <div className="grid h-full place-items-center text-sm text-slate-600">
                暂无月度记录
              </div>
            )}
          </div>
        </Card>

        <Card className="p-6">
          <SectionTitle
            eyebrow="Lifetime"
            title="游玩生涯"
          />
          <div className="mt-6 space-y-4">
            {lifetimeMetrics.map(({ icon: Icon, label, value }) => (
              <div
                className="flex items-center gap-3 rounded-xl border border-white/[0.055] bg-white/[0.025] p-3"
                key={label}
              >
                <span className="grid size-9 place-items-center rounded-lg bg-white/[0.045] text-slate-400">
                  <Icon className="size-4" />
                </span>
                <div>
                  <p className="text-[11px] text-slate-500">{label}</p>
                  <p className="mt-1 font-mono text-sm font-semibold text-white">
                    {value}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </>
  );
}
