import { useEffect, useRef, useState } from "react";
import {
  CircleUserRound,
  ExternalLink,
  Film,
  Heart,
  Image,
  LayoutDashboard,
  Map,
  Music2,
  PackageOpen,
  Palette,
  Play,
  Radio,
  ScanSearch,
  WandSparkles,
  Settings,
  Wrench,
} from "lucide-react";
import { NavLink, useLocation } from "react-router-dom";
import type { OwnProfile, OsuClient } from "../shared/types/osu";
import { cn } from "../shared/lib/cn";
import { Avatar } from "../shared/components/Avatar";
import { Skeleton } from "../shared/components/ui";
import { desktopApi } from "../shared/lib/tauri";
import { useMode } from "./ModeContext";

interface NavItemProps {
  to: string;
  label: string;
  icon: typeof LayoutDashboard;
  emphasis?: "beatmaps" | "similar" | "trainer";
  onboarding?: string;
}

function NavItem({ to, label, icon: Icon, emphasis, onboarding }: NavItemProps) {
  const location = useLocation();
  // Only the data center has nested navigation.  Using prefix matching for
  // every item made “截图与回放” remain selected on /local/media/render.
  const active = location.pathname === to || (to === "/data" && location.pathname.startsWith("/data/"));
  const linkRef = useRef<HTMLAnchorElement>(null);

  useEffect(() => {
    if (!active) return;
    const frame = window.requestAnimationFrame(() => linkRef.current?.scrollIntoView({ block: "nearest" }));
    return () => window.cancelAnimationFrame(frame);
  }, [active]);

  return (
    <NavLink
      className={cn(
        "opp-nav-item group flex min-h-10 items-center gap-3 px-3 text-[13px] font-medium text-slate-400 outline-none hover:bg-[var(--surface-interactive)] hover:text-slate-100 focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)]",
        emphasis && "min-h-11",
        active && "text-white",
      )}
      end={to !== "/data"}
      data-onboarding={onboarding}
      ref={linkRef}
      to={to}
    >
      {emphasis ? (
        <span className={cn("grid size-7 shrink-0 place-items-center", emphasis === "beatmaps" ? "text-cyan-200" : emphasis === "similar" ? "text-violet-200" : "text-pink-200")}>
          <Icon className="size-4" />
        </span>
      ) : <Icon className="size-4 shrink-0" />}
      <span className={cn("min-w-0 flex-1 truncate", emphasis && "font-semibold")}>{label}</span>
    </NavLink>
  );
}

function NavGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <section className="mt-5 first:mt-0">
      <p className="mb-1.5 px-2.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-slate-500">{label}</p>
      <div className="space-y-0.5">{children}</div>
    </section>
  );
}

export function Sidebar({ profile, loading }: { profile?: OwnProfile; loading: boolean }) {
  const { ruleset } = useMode();
  const [starting, setStarting] = useState(false);
  const [startMenuOpen, setStartMenuOpen] = useState(false);
  const startGame = async (targetClient: OsuClient) => {
    setStarting(true); setStartMenuOpen(false);
    try { await desktopApi.startGameSession(ruleset, targetClient); } finally { setStarting(false); }
  };

  return (
    <aside className="fixed bottom-0 left-0 top-11 z-40 flex w-[var(--sidebar-width)] flex-col overflow-hidden border-r border-[var(--line-subtle)] bg-[var(--surface-sidebar)] px-3 pb-3 pt-4">
      <div className="mb-4 border-b border-[var(--line-subtle)] px-2 pb-4"><div className="flex items-center gap-3"><img alt="OPP" className="opp-brand-mark size-10" src="/03.png" /><p className="text-sm font-semibold tracking-wide text-white">OSU! Plus Plus</p></div></div>
      <nav aria-label="主导航" className="min-h-0 flex-1 overflow-y-auto pr-1">
        <NavGroup label="核心功能">
          <div data-onboarding="online-and-collections">
            <NavItem emphasis="beatmaps" icon={Music2} label="在线谱面" to="/online/beatmaps" />
            <NavItem emphasis="beatmaps" icon={Heart} label="谱面收藏夹" to="/collections" />
            <NavItem emphasis="beatmaps" icon={PackageOpen} label="BeatmapHub" to="/beatmaphub" />
          </div>
          <NavItem emphasis="similar" icon={ScanSearch} label="相似谱面" onboarding="similar-beatmaps" to="/online/similar" />
          <NavItem emphasis="trainer" icon={WandSparkles} label="谱面练习生成器" onboarding="trainer" to="/trainer" />
        </NavGroup>
        <NavGroup label="资料与资源">
          <NavItem icon={CircleUserRound} label="玩家信息" onboarding="data-center" to="/data" />
          <div data-onboarding="local-resources">
            <NavItem icon={Map} label="本地谱面" to="/local/maps" />
            <NavItem icon={Palette} label="本地皮肤" to="/local/skins" />
            <NavItem icon={Image} label="截图与回放" to="/local/media" />
          </div>
        </NavGroup>
        <NavGroup label="创作与直播">
          <NavItem icon={Film} label="回放渲染" onboarding="replay-render" to="/local/media/render" />
          <NavItem icon={Radio} label="tosu 直播集成" onboarding="tosu" to="/tosu" />
          <NavItem icon={Wrench} label="工具集合" onboarding="tools" to="/tools" />
        </NavGroup>
      </nav>
      <div className="relative shrink-0 border-t border-[var(--line-subtle)] pt-3">
        <button aria-expanded={startMenuOpen} aria-label="选择客户端并启动游戏" className="flex min-h-11 w-full items-center justify-center gap-2 rounded-lg bg-[var(--theme-primary)] px-4 py-2.5 text-sm font-semibold text-[var(--on-primary)] shadow-[inset_0_1px_0_rgba(255,255,255,0.16),0_1px_2px_rgba(0,0,0,0.3)] transition-colors hover:bg-[var(--theme-primary-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-sidebar)] disabled:opacity-50" data-onboarding="start-game" disabled={starting} onClick={() => setStartMenuOpen((open) => !open)} type="button"><Play className={`size-4 ${starting ? "animate-pulse" : ""}`} />启动 osu!</button>
        {startMenuOpen ? <div className="absolute bottom-14 left-0 z-50 w-full rounded-lg border border-white/10 bg-[var(--surface-panel-strong)] p-2 shadow-2xl"><button className="min-h-10 w-full rounded-md px-2 py-2 text-left text-xs text-slate-300 transition-colors hover:bg-white/[0.06] hover:text-white" onClick={() => void startGame("stable")} type="button">osu! Stable</button><button className="mt-0.5 min-h-10 w-full rounded-md px-2 py-2 text-left text-xs text-slate-300 transition-colors hover:bg-white/[0.06] hover:text-white" onClick={() => void startGame("lazer")} type="button">osu! Lazer</button></div> : null}
      </div>
      <div className="mt-2 shrink-0"><NavItem icon={Settings} label="设置" onboarding="settings" to="/settings" />{loading ? <div className="mt-2 flex items-center gap-3 px-2 py-1"><Skeleton className="size-8 rounded-lg" /><Skeleton className="h-3 w-20" /></div> : profile ? <div className="mt-2 flex items-center gap-3 border-t border-[var(--line-subtle)] px-2 pt-3"><Avatar className="size-8 rounded-lg border border-white/10" profile={profile} /><div className="min-w-0 flex-1"><p className="truncate text-xs font-semibold text-white">{profile.username}</p></div><button aria-label="在浏览器中打开个人主页" className="grid size-8 shrink-0 place-items-center rounded-md text-slate-500 transition-colors hover:bg-white/[0.06] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)]" onClick={() => void desktopApi.openExternal(`https://osu.ppy.sh/users/${profile.id}`)} title="在浏览器中打开个人主页" type="button"><ExternalLink className="size-3.5" /></button></div> : null}</div>
    </aside>
  );
}
