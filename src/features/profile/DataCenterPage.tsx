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
  return <div className="space-y-5">
    <Card className="relative overflow-hidden border-white/[0.1] p-0">
      <div className="relative h-36 overflow-hidden bg-gradient-to-r from-cyan-400/20 via-slate-900 to-pink-400/15">
        {profile?.cover_url ? <img alt="" className="absolute inset-0 size-full object-cover opacity-50" src={profile.cover_url} /> : null}
        <div className="absolute inset-0 bg-gradient-to-t from-[var(--surface-panel)] via-transparent to-black/10" />
      </div>
      <div className="relative -mt-10 flex items-end gap-4 px-6 pb-5">
        {profileQuery.isLoading ? <Skeleton className="size-20 rounded-full" /> : profile ? <Avatar profile={profile} className="size-20 rounded-full border-4 border-[var(--surface-panel)]" /> : <span className="size-20 rounded-full border-4 border-[var(--surface-panel)] bg-white/10" />}
        <div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><h1 className="text-2xl font-bold text-white">{profile?.username ?? "数据中心"}</h1>{profile?.country_code ? <Badge tone="cyan">{profile.country_code}</Badge> : null}</div><p className="mt-1 text-xs text-slate-400">{profile ? `${rulesetLabels[ruleset]} · osu! 个人资料` : "加载个人资料…"}</p></div>
        {profile?.is_supporter ? <Badge tone="pink">Supporter</Badge> : null}
      </div>
    </Card>
    <nav aria-label="数据中心页面" className="sticky top-[96px] z-20 flex overflow-x-auto border-b border-white/[0.1] bg-[var(--surface)]/95 backdrop-blur-md">
      {sections.map(([id, label, Icon]) => <NavLink className="inline-flex shrink-0 items-center gap-2 border-b-2 border-transparent px-4 py-3 text-xs font-semibold text-slate-500 transition hover:text-white [&.active]:border-[var(--theme-primary)] [&.active]:text-[var(--theme-primary)]" end to={`/data/${id}`} key={id}><Icon className="size-3.5" />{label}</NavLink>)}
    </nav>
    <main><Outlet /></main>
  </div>;
}
