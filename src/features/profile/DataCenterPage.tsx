import { BarChart3, Database, History, Medal, Pin, UserRound } from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";
import { Avatar } from "../../shared/components/Avatar";
import { Badge, Card, Skeleton } from "../../shared/components/ui";
import { useMode } from "../../app/ModeContext";
import { useOwnProfile } from "./api";
import { rulesetLabels } from "../../shared/lib/format";

const sections = [
  ["overview", "概览", Database], ["scores", "最佳成绩", BarChart3],
  ["recent", "近期成绩", History], ["pinned", "Pinned", Pin],
  ["medals", "玩家奖牌", Medal], ["profile", "详细档案", UserRound],
] as const;

export function DataCenterPage() {
  const { ruleset } = useMode();
  const profileQuery = useOwnProfile(ruleset);
  const profile = profileQuery.data?.data;
  return <div className="space-y-4">
    <Card className="opp-profile-rail relative overflow-hidden p-0">
      {profile?.cover_url ? <img alt="" className="absolute inset-0 size-full object-cover opacity-[0.16]" src={profile.cover_url} /> : null}
      <div className="absolute inset-0 bg-gradient-to-r from-[var(--surface)]/95 via-[var(--surface)]/80 to-transparent" />
      <div className="relative flex min-h-[76px] items-center gap-3 px-4 py-3">
        {profileQuery.isLoading ? <Skeleton className="size-14 rounded-full" /> : profile ? <Avatar profile={profile} className="size-14 rounded-full border-2 border-[var(--surface-panel)]" /> : <span className="size-14 rounded-full border-2 border-[var(--surface-panel)] bg-white/10" />}
        <div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><h1 className="text-lg font-semibold text-white">{profile?.username ?? "数据中心"}</h1>{profile?.country_code ? <Badge tone="cyan">{profile.country_code}</Badge> : null}</div><p className="mt-0.5 text-xs text-slate-400">{profile ? `${rulesetLabels[ruleset]} · osu! 个人资料` : "加载个人资料…"}</p></div>
        {profile?.is_supporter ? <Badge tone="pink">Supporter</Badge> : null}
      </div>
    </Card>
    <nav aria-label="数据中心页面" className="sticky top-[96px] z-20 flex overflow-x-auto border-b border-white/[0.1] bg-[var(--surface)]/95 backdrop-blur-md">
      {sections.map(([id, label, Icon]) => <NavLink className="inline-flex shrink-0 items-center gap-2 border-b-2 border-transparent px-4 py-2.5 text-xs font-semibold text-slate-500 transition hover:text-white [&.active]:border-[var(--theme-primary)] [&.active]:text-[var(--theme-primary)]" end to={`/data/${id}`} key={id}><Icon className="size-3.5" />{label}</NavLink>)}
    </nav>
    <main><Outlet /></main>
  </div>;
}
