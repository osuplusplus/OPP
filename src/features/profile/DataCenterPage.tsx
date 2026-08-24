import { BarChart3, CircleUserRound, History, Medal, Pin, UserRound } from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";
import { Avatar } from "../../shared/components/Avatar";
import { Badge, Card, Skeleton } from "../../shared/components/ui";
import { useMode } from "../../app/ModeContext";
import { useOwnProfile } from "./api";
import { rulesetLabels } from "../../shared/lib/format";

const sections = [
  ["overview", "概览", CircleUserRound], ["scores", "最佳成绩", BarChart3],
  ["recent", "近期成绩", History], ["pinned", "Pinned", Pin],
  ["medals", "玩家奖牌", Medal], ["profile", "详细档案", UserRound],
] as const;

export function DataCenterPage() {
  const { ruleset } = useMode();
  const profileQuery = useOwnProfile(ruleset);
  const profile = profileQuery.data?.data;
  return <div className="space-y-4">
    <Card className="opp-profile-rail theme-profile-hero relative min-h-44 overflow-hidden p-0">
      {profile?.cover_url ? <img alt="" className="absolute inset-0 size-full object-cover" src={profile.cover_url} /> : null}
      <div className="theme-profile-overlay absolute inset-0 bg-[linear-gradient(90deg,rgba(8,11,19,.94)_0%,rgba(8,11,19,.72)_48%,rgba(8,11,19,.34)_100%)]" />
      <div className="absolute inset-x-0 bottom-0 h-20 bg-gradient-to-t from-black/45 to-transparent" />
      <div className="relative flex min-h-44 items-end gap-4 px-5 py-5">
        {profileQuery.isLoading ? <Skeleton className="size-20 rounded-xl" /> : profile ? <Avatar profile={profile} className="size-20 rounded-xl border-2 border-white/20 shadow-xl" /> : <span className="size-20 rounded-xl border-2 border-white/20 bg-white/10" />}
        <div className="min-w-0 flex-1 pb-1"><div className="flex flex-wrap items-center gap-2"><h1 className="truncate text-2xl font-semibold tracking-tight text-white">{profile?.username ?? "玩家信息"}</h1>{profile?.country_code ? <Badge tone="cyan">{profile.country_code}</Badge> : null}{profile?.is_supporter ? <Badge tone="pink">Supporter</Badge> : null}</div><p className="mt-1 text-xs text-slate-300">{profile ? `${rulesetLabels[ruleset]} · osu! 官方玩家资料` : "正在加载玩家资料…"}</p></div>
        <span className="mb-1 hidden text-[10px] font-semibold uppercase tracking-[0.18em] text-white/55 sm:block">Player profile</span>
      </div>
    </Card>
    <nav aria-label="玩家信息页面" className="sticky top-[96px] z-20 flex overflow-x-auto border-b border-white/[0.1] bg-[var(--surface)]/95 backdrop-blur-md">
      {sections.map(([id, label, Icon]) => <NavLink className="inline-flex shrink-0 items-center gap-2 border-b-2 border-transparent px-4 py-2.5 text-xs font-semibold text-slate-500 transition hover:text-white [&.active]:border-[var(--theme-primary)] [&.active]:text-[var(--theme-primary)]" end to={`/data/${id}`} key={id}><Icon className="size-3.5" />{label}</NavLink>)}
    </nav>
    <main><Outlet /></main>
  </div>;
}
