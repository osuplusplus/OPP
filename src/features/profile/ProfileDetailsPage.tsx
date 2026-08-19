import type { MouseEvent } from "react";
import DOMPurify from "dompurify";
import * as Tabs from "@radix-ui/react-tabs";
import {
  Award,
  BookOpenText,
  Braces,
  CalendarDays,
  ExternalLink,
  Gamepad2,
  Globe2,
  Heart,
  History,
  Map,
  Medal,
  MessageCircle,
  Shield,
  Sparkles,
  Star,
  Users,
  type LucideIcon,
} from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import {
  Badge,
  Card,
  DataLine,
  EmptyState,
  SectionTitle,
  Skeleton,
} from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import {
  dateOnly,
  duration,
  fullNumber,
  percent,
  rulesetLabels,
} from "../../shared/lib/format";
import type { OwnProfile, Ruleset, UserStatistics } from "../../shared/types/osu";
import { useOwnProfile } from "./api";
import { selectedStatistics } from "./statistics";

const tabs = [
  ["identity", "身份与社交", Users],
  ["stats", "游玩统计", Gamepad2],
  ["community", "社区与谱面", Map],
  ["about", "关于我", BookOpenText],
  ["raw", "原始数据", Braces],
] as const;

function ProfileDetailsSkeleton() {
  return (
    <div>
      <Skeleton className="h-12 w-full" />
      <div className="mt-5 grid grid-cols-2 gap-5">
        <Skeleton className="h-96" />
        <Skeleton className="h-96" />
      </div>
    </div>
  );
}

function InfoCard({
  title,
  icon: Icon,
  children,
}: {
  title: string;
  icon: typeof Users;
  children: React.ReactNode;
}) {
  return (
    <Card className="p-5">
      <div className="mb-3 flex items-center gap-3">
        <span className="grid size-9 place-items-center rounded-xl bg-white/[0.05] text-cyan-200">
          <Icon className="size-4" />
        </span>
        <h3 className="text-sm font-semibold text-white">{title}</h3>
      </div>
      {children}
    </Card>
  );
}

function linkValue(value?: string | null) {
  if (!value) return "—";
  return (
    <button
      className="inline-flex items-center gap-1 text-cyan-200 hover:text-cyan-100"
      onClick={() => desktopApi.openExternal(value.startsWith("http") ? value : `https://${value}`)}
      type="button"
    >
      {value}
      <ExternalLink className="size-3" />
    </button>
  );
}

function IdentityTab({ profile }: { profile: OwnProfile }) {
  return (
    <div className="grid grid-cols-2 gap-5">
      <InfoCard icon={Users} title="账号身份">
        <DataLine label="用户名" value={profile.username} />
        <DataLine label="用户 ID" value={profile.id} />
        <DataLine
          label="国家或地区"
          value={profile.country?.name ?? profile.country_code}
        />
        <DataLine label="加入日期" value={dateOnly(profile.join_date)} />
        <DataLine label="所在地" value={profile.location ?? "—"} />
        <DataLine
          label="账号状态"
          value={
            <div className="flex justify-end gap-2">
              <Badge tone={profile.is_active ? "success" : "neutral"}>
                {profile.is_active ? "活跃" : "不活跃"}
              </Badge>
              {profile.is_restricted ? <Badge tone="warning">受限</Badge> : null}
            </div>
          }
        />
      </InfoCard>
      <InfoCard icon={Globe2} title="社交资料">
        <DataLine label="Discord" value={profile.discord ?? "—"} />
        <DataLine label="Twitter / X" value={profile.twitter ?? "—"} />
        <DataLine label="个人网站" value={linkValue(profile.website)} />
        <DataLine label="职业" value={profile.occupation ?? "—"} />
        <DataLine label="兴趣" value={profile.interests ?? "—"} />
        <DataLine label="自定义称号" value={profile.title ?? "—"} />
      </InfoCard>
      <InfoCard icon={History} title="历史用户名">
        {profile.previous_usernames?.length ? (
          <div className="flex flex-wrap gap-2 pt-2">
            {profile.previous_usernames.map((username) => (
              <Badge key={username}>{username}</Badge>
            ))}
          </div>
        ) : (
          <p className="pt-2 text-sm text-slate-500">没有公开的历史用户名</p>
        )}
      </InfoCard>
      <InfoCard icon={Shield} title="账号记录">
        {profile.account_history?.length ? (
          <div className="space-y-3 pt-2">
            {profile.account_history.map((entry, index) => (
              <div
                className="rounded-xl border border-white/[0.06] bg-white/[0.025] p-3"
                key={String(entry.id ?? index)}
              >
                <div className="flex items-center justify-between">
                  <Badge tone={entry.permanent ? "warning" : "neutral"}>
                    {String(entry.type ?? "记录")}
                  </Badge>
                  <span className="text-[11px] text-slate-600">
                    {dateOnly(String(entry.timestamp ?? ""))}
                  </span>
                </div>
                {entry.description ? (
                  <p className="mt-2 text-xs leading-5 text-slate-400">
                    {String(entry.description)}
                  </p>
                ) : null}
              </div>
            ))}
          </div>
        ) : (
          <p className="pt-2 text-sm text-slate-500">账号记录为空</p>
        )}
      </InfoCard>
    </div>
  );
}

function ModeStats({
  mode,
  stats,
}: {
  mode: Ruleset;
  stats?: UserStatistics;
}) {
  return (
    <Card className="p-5">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold uppercase text-white">{rulesetLabels[mode]}</h3>
        <Badge tone={stats?.is_ranked ? "success" : "neutral"}>
          {stats?.is_ranked ? "已上榜" : "未上榜"}
        </Badge>
      </div>
      <div className="mt-4 grid grid-cols-2 gap-x-5">
        <DataLine label="PP" value={stats?.pp?.toFixed(2) ?? "—"} />
        <DataLine label="全球排名" value={`#${fullNumber(stats?.global_rank)}`} />
        <DataLine label="准确率" value={percent(stats?.hit_accuracy)} />
        <DataLine label="地区排名" value={`#${fullNumber(stats?.country_rank)}`} />
        <DataLine label="游玩次数" value={fullNumber(stats?.play_count)} />
        <DataLine label="游玩时长" value={duration(stats?.play_time)} />
        <DataLine label="最大连击" value={`${fullNumber(stats?.maximum_combo)}x`} />
      </div>
    </Card>
  );
}

function StatsTab({
  profile,
  ruleset,
}: {
  profile: OwnProfile;
  ruleset: Ruleset;
}) {
  const scoreMetrics: Array<{
    label: string;
    value?: number;
    icon: LucideIcon;
  }> = [
    { label: "最佳成绩", value: profile.scores_best_count, icon: Star },
    { label: "第一名成绩", value: profile.scores_first_count, icon: Medal },
    { label: "近期成绩", value: profile.scores_recent_count, icon: History },
  ];
  return (
    <div>
      <ModeStats
        mode={ruleset}
        stats={selectedStatistics(profile, ruleset)}
      />
      <Card className="mt-5 p-5">
        <SectionTitle
          eyebrow="Score inventory"
          title="成绩记录量"
        />
        <div className="mt-4 grid grid-cols-3 gap-4">
          {scoreMetrics.map(({ label, value, icon: Icon }) => (
            <div className="rounded-xl border border-white/[0.06] bg-white/[0.025] p-4" key={label}>
              <Icon className="size-4 text-pink-200" />
              <p className="mt-3 font-mono text-xl font-semibold text-white">
                {fullNumber(value)}
              </p>
              <p className="mt-1 text-xs text-slate-500">{label}</p>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

function CommunityTab({ profile }: { profile: OwnProfile }) {
  const metrics: Array<{
    label: string;
    value?: number;
    icon: LucideIcon;
  }> = [
    { label: "关注者", value: profile.follower_count, icon: Users },
    { label: "谱面关注者", value: profile.mapping_follower_count, icon: Heart },
    { label: "论坛发帖", value: profile.post_count, icon: MessageCircle },
    { label: "游玩谱面", value: profile.beatmap_playcounts_count, icon: Gamepad2 },
    { label: "收藏谱面集", value: profile.favourite_beatmapset_count, icon: Star },
    { label: "Ranked 谱面集", value: profile.ranked_beatmapset_count, icon: Sparkles },
    { label: "Loved 谱面集", value: profile.loved_beatmapset_count, icon: Heart },
    { label: "Pending 谱面集", value: profile.pending_beatmapset_count, icon: CalendarDays },
    { label: "Graveyard 谱面集", value: profile.graveyard_beatmapset_count, icon: History },
    { label: "Guest 谱面集", value: profile.guest_beatmapset_count, icon: Users },
    { label: "Nominated 谱面集", value: profile.nominated_beatmapset_count, icon: Award },
    { label: "Kudosu 总计", value: profile.kudosu?.total, icon: Medal },
  ];
  return (
    <div className="grid grid-cols-4 gap-4">
      {metrics.map(({ label, value, icon: Icon }) => (
        <Card className="p-4" key={label}>
          <div className="flex items-center justify-between">
            <Icon className="size-4 text-cyan-200" />
            <span className="font-mono text-xl font-semibold text-white">
              {fullNumber(value)}
            </span>
          </div>
          <p className="mt-4 text-xs text-slate-500">{label}</p>
        </Card>
      ))}
    </div>
  );
}

export function AboutProfile({ profile }: { profile: OwnProfile }) {
  const sanitized = DOMPurify.sanitize(profile.page?.html ?? "", {
    FORBID_TAGS: ["script", "iframe", "object", "embed", "style", "form"],
    FORBID_ATTR: ["style"],
    ALLOW_UNKNOWN_PROTOCOLS: false,
  });

  const openLink = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target as Element;
    const anchor = target.closest("a");
    if (!anchor) return;
    event.preventDefault();
    const href = anchor.getAttribute("href");
    if (href?.startsWith("https://") || href?.startsWith("http://")) {
      desktopApi.openExternal(href);
    }
  };

  if (!sanitized) {
    return (
      <EmptyState
        icon={<BookOpenText className="size-5" />}
        title="没有公开的自我介绍"
        description="当你在 osu! 个人资料中添加内容后，这里会以经过安全清洗的形式展示。"
      />
    );
  }
  return (
    <Card className="p-7">
      <div
        className="profile-html"
        dangerouslySetInnerHTML={{ __html: sanitized }}
        onClick={openLink}
      />
    </Card>
  );
}

function RawTab({ profile }: { profile: OwnProfile }) {
  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between border-b border-white/[0.06] px-5 py-4">
        <div>
          <p className="text-sm font-semibold text-white">UserExtended 原始响应</p>
          <p className="mt-1 text-xs text-slate-500">
            只读视图，确保 API 中不常见的可选字段仍然可检查。
          </p>
        </div>
        <Badge tone="cyan">API v2</Badge>
      </div>
      <pre className="max-h-[620px] overflow-auto p-5 font-mono text-[11px] leading-6 text-slate-400">
        {JSON.stringify(profile, null, 2)}
      </pre>
    </Card>
  );
}

export function ProfileDetailsPage() {
  const { ruleset } = useMode();
  const profileQuery = useOwnProfile(ruleset);
  const profile = profileQuery.data?.data;

  if (profileQuery.isLoading) {
    return (
      <>
        <PageHeader
          description="按主题浏览 UserExtended 返回的完整资料。"
          eyebrow="Profile archive"
          title="详细档案"
        />
        <ProfileDetailsSkeleton />
      </>
    );
  }
  if (profileQuery.error || !profile) {
    return (
      <>
        <PageHeader
          description="按主题浏览 UserExtended 返回的完整资料。"
          eyebrow="Profile archive"
          title="详细档案"
        />
        <ErrorPanel error={profileQuery.error} onRetry={() => profileQuery.refetch()} />
      </>
    );
  }

  return (
    <>
      <PageHeader
        description={`身份与社区资料保持完整，游玩统计集中展示全局所选的 ${rulesetLabels[ruleset]}。`}
        eyebrow="Profile archive"
        title="详细档案"
      />
      <Tabs.Root defaultValue="identity">
        <Tabs.List
          aria-label="档案分类"
          className="mb-5 flex overflow-hidden rounded-2xl border border-white/[0.12] bg-white/[0.045] p-1"
        >
          {tabs.map(([value, label, Icon]) => (
            <Tabs.Trigger
              className="relative inline-flex flex-1 items-center justify-center gap-2 border-b-2 border-transparent px-3 py-2.5 text-xs font-semibold text-slate-500 outline-none transition hover:bg-white/[0.05] hover:text-slate-200 focus-visible:ring-2 focus-visible:ring-cyan-300/40 data-[state=active]:border-[var(--theme-primary)] data-[state=active]:bg-[var(--theme-primary-muted)] data-[state=active]:text-[var(--theme-primary)]"
              key={value}
              value={value}
            >
              <Icon className="size-4" />
              {label}
            </Tabs.Trigger>
          ))}
        </Tabs.List>
        <Tabs.Content value="identity">
          <IdentityTab profile={profile} />
        </Tabs.Content>
        <Tabs.Content value="stats">
          <StatsTab profile={profile} ruleset={ruleset} />
        </Tabs.Content>
        <Tabs.Content value="community">
          <CommunityTab profile={profile} />
        </Tabs.Content>
        <Tabs.Content value="about">
          <AboutProfile profile={profile} />
        </Tabs.Content>
        <Tabs.Content value="raw">
          <RawTab profile={profile} />
        </Tabs.Content>
      </Tabs.Root>
    </>
  );
}
