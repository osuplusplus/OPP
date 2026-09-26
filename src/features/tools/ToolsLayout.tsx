import { Suspense, useState, type ReactNode } from "react";
import { useOutlet, useLocation, useNavigate } from "react-router-dom";
import { Cpu, FileCog, ImageIcon, Radio } from "lucide-react";
import { cn } from "../../shared/lib/cn";
import { RouteDialog } from "../../shared/components/RouteDialog";

const categories = [
  { id: "game", label: "游戏与设备", icon: Cpu, path: "/tools/game" },
  { id: "beatmaps", label: "谱面与预览", icon: ImageIcon, path: "/tools/beatmaps" },
  { id: "system", label: "系统与文件", icon: FileCog, path: "/tools/system" },
  { id: "live", label: "媒体与直播", icon: Radio, path: "/tools/live" },
] as const;

export function ToolsLayout() {
  const navigate = useNavigate();
  const location = useLocation();
  const outlet = useOutlet();
  const [visited, setVisited] = useState<Record<string, ReactNode>>({});
  const activeCategory = categories.find((category) => location.pathname.startsWith(category.path))?.id ?? "game";
  // Keep visited tools mounted so drafts and results survive category changes.
  const categoryPath = categories.find((category) => category.id === activeCategory)!.path;
  if (location.pathname === categoryPath && !visited[activeCategory]) {
    setVisited({ ...visited, [activeCategory]: outlet });
  }
  return (
    <RouteDialog closeLabel="关闭工具" title="工具集合">
      <aside className="flex w-56 shrink-0 flex-col border-r border-white/[0.08] bg-black/20">
        <div className="border-b border-white/[0.06] p-6">
          <p className="text-[10px] font-bold uppercase tracking-[.2em] text-[var(--theme-primary)]">Application</p>
          <h1 className="mt-1 text-xl font-bold text-white">工具集合</h1>
        </div>
        <nav className="flex-1 overflow-y-auto p-3">
          <div className="space-y-1">
            {categories.map((category) => {
              const Icon = category.icon;
              return <button className={cn("flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors", activeCategory === category.id ? "bg-[var(--theme-primary)]/15 text-[var(--theme-primary)] shadow-sm" : "text-slate-300 hover:bg-white/[0.05] hover:text-white")} key={category.id} onClick={() => navigate(category.path, { replace: true })} type="button"><Icon className="size-4 shrink-0" /><span className="truncate">{category.label}</span></button>;
            })}
          </div>
        </nav>
      </aside>
      <main className="min-w-0 flex-1 overflow-hidden">
        {Object.entries(visited).map(([category, content]) => <div key={category} hidden={category !== activeCategory} className="h-full overflow-y-auto p-6">
          <Suspense fallback={<p role="status" className="text-sm text-slate-400">正在加载工具…</p>}>{category === activeCategory ? outlet : content}</Suspense>
        </div>)}
        {location.pathname !== categoryPath ? outlet : null}
      </main>
    </RouteDialog>
  );
}
