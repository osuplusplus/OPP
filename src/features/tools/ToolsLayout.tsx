import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { Cpu, FileCog, ImageIcon, Radio, Wrench } from "lucide-react";
import { Button } from "../../shared/components/ui";
import { cn } from "../../shared/lib/cn";

const categories = [
  { id: "game", label: "游戏与设备", icon: Cpu, path: "/tools/game" },
  { id: "beatmaps", label: "谱面与预览", icon: ImageIcon, path: "/tools/beatmaps" },
  { id: "system", label: "系统与文件", icon: FileCog, path: "/tools/system" },
  { id: "live", label: "媒体与直播", icon: Radio, path: "/tools/live" },
] as const;

export function ToolsLayout() {
  const navigate = useNavigate();
  const location = useLocation();
  const activeCategory = categories.find((category) => location.pathname.startsWith(category.path))?.id ?? "game";
  return (
    <div className="fixed inset-0 z-[70] overflow-hidden bg-black/65 backdrop-blur-sm">
      <div className="flex h-full items-center justify-center p-4">
        <div className="flex h-full max-h-[90vh] w-full max-w-6xl overflow-hidden rounded-3xl border border-white/10 bg-[var(--surface)] shadow-2xl">
          <aside className="flex w-56 shrink-0 flex-col border-r border-white/[0.08] bg-black/20">
            <div className="border-b border-white/[0.06] p-6">
              <p className="text-[10px] font-bold uppercase tracking-[.2em] text-[var(--theme-primary)]">Application</p>
              <h1 className="mt-1 text-xl font-bold text-white">工具集合</h1>
            </div>
            <nav className="flex-1 overflow-y-auto p-3">
              <div className="space-y-1">
                {categories.map((category) => {
                  const Icon = category.icon;
                  return <button className={cn("flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors", activeCategory === category.id ? "bg-[var(--theme-primary)]/15 text-[var(--theme-primary)] shadow-sm" : "text-slate-300 hover:bg-white/[0.05] hover:text-white")} key={category.id} onClick={() => navigate(category.path)} type="button"><Icon className="size-4 shrink-0" /><span className="truncate">{category.label}</span></button>;
                })}
              </div>
            </nav>
            <div className="border-t border-white/[0.06] p-3"><Button className="w-full" onClick={() => navigate("/online/beatmaps", { replace: true })} size="sm" variant="ghost"><Wrench className="size-3.5" />关闭工具</Button></div>
          </aside>
          <main className="min-w-0 flex-1 overflow-y-auto"><div className="p-6"><Outlet /></div></main>
        </div>
      </div>
    </div>
  );
}
