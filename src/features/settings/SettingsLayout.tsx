import { useNavigate } from "react-router-dom";
import {
  Bug,
  User,
  Palette,
  Download,
  FolderOpen,
  Film,
  Wrench,
  Sparkles,
  Info,
} from "lucide-react";
import { Button } from "../../shared/components/ui";
import { cn } from "../../shared/lib/cn";

const categories = [
  { id: "logs", label: "日志与诊断", icon: Bug },
  { id: "account", label: "账户", icon: User },
  { id: "appearance", label: "外观", icon: Palette },
  { id: "online", label: "在线谱面", icon: Download },
  { id: "directories", label: "游戏目录", icon: FolderOpen },
  { id: "replay", label: "回放渲染", icon: Film },
  { id: "tools", label: "工具与缓存", icon: Wrench },
  { id: "similarity", label: "相似谱面", icon: Sparkles },
  { id: "about", label: "关于", icon: Info },
] as const;

export type SettingsCategory = typeof categories[number]["id"];

interface SettingsLayoutProps {
  children: React.ReactNode;
  activeCategory: SettingsCategory;
  onCategoryChange: (category: SettingsCategory) => void;
}

export function SettingsLayout({ children, activeCategory, onCategoryChange }: SettingsLayoutProps) {
  const navigate = useNavigate();

  return (
    <div className="fixed inset-0 z-[70] overflow-hidden bg-black/65 backdrop-blur-sm">
      <div className="flex h-full items-center justify-center p-4">
        <div className="flex h-full max-h-[90vh] w-full max-w-6xl overflow-hidden rounded-3xl border border-white/10 bg-[var(--surface)] shadow-2xl">
          {/* 左侧导航栏 */}
          <aside className="flex w-56 shrink-0 flex-col border-r border-white/[0.08] bg-black/20">
            <div className="border-b border-white/[0.06] p-6">
              <p className="text-[10px] font-bold uppercase tracking-[.2em] text-[var(--theme-primary)]">
                Application
              </p>
              <h1 className="mt-1 text-xl font-bold text-white">设置</h1>
            </div>

            <nav className="flex-1 overflow-y-auto p-3">
              <div className="space-y-1">
                {categories.map((category) => {
                  const Icon = category.icon;
                  const isActive = activeCategory === category.id;

                  return (
                    <button
                      key={category.id}
                      onClick={() => onCategoryChange(category.id)}
                      className={cn(
                        "flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors",
                        isActive
                          ? "bg-[var(--theme-primary)]/15 text-[var(--theme-primary)] shadow-sm"
                          : "text-slate-300 hover:bg-white/[0.05] hover:text-white"
                      )}
                      type="button"
                    >
                      <Icon className="size-4 shrink-0" />
                      <span className="truncate">{category.label}</span>
                    </button>
                  );
                })}
              </div>
            </nav>

            <div className="border-t border-white/[0.06] p-3">
              <Button
                onClick={() => navigate(-1)}
                size="sm"
                variant="ghost"
                className="w-full"
              >
                关闭设置
              </Button>
            </div>
          </aside>

          {/* 右侧内容区域 */}
          <main className="flex-1 overflow-y-auto">
            <div className="p-6">
              {children}
            </div>
          </main>
        </div>
      </div>
    </div>
  );
}
