import { Check } from "lucide-react";
import * as Switch from "@radix-ui/react-switch";
import { Card, InfoTip, SectionTitle } from "../../../shared/components/ui";
import type { AppSettings, ThemeColor } from "../../../shared/types/osu";

const colors: Array<[ThemeColor, string, string]> = [
  ["cyan", "青色", "#67e8f9"],
  ["blue", "蓝色", "#60a5fa"],
  ["violet", "紫罗兰", "#a78bfa"],
  ["pink", "粉色", "#f472b6"],
  ["orange", "橙色", "#fb923c"],
  ["green", "绿色", "#4ade80"],
];

export function AppearancePanel({
  className,
  settings,
  save,
}: {
  className?: string;
  settings: AppSettings;
  save: (next: AppSettings) => Promise<void>;
}) {
  const lightTheme = settings.theme_mode === "light";
  return (
    <Card className={className}>
      <SectionTitle title="主题" />
      <div className="mt-5 flex items-center justify-between rounded-xl border border-white/[.1] bg-white/[.035] p-4">
        <div className="flex items-center gap-2">
          <p className="font-semibold text-slate-100">浅色主题</p>
          <InfoTip text="开启后使用浅色界面；关闭时使用默认深色界面。" />
        </div>
        <Switch.Root
          checked={lightTheme}
          className="relative h-6 w-11 shrink-0 rounded-full bg-slate-500 data-[state=checked]:bg-[var(--theme-primary)]"
          onCheckedChange={(value) => void save({ ...settings, theme_mode: value ? "light" : "dark" })}
        >
          <Switch.Thumb className="block size-5 translate-x-0.5 rounded-full bg-white transition-transform data-[state=checked]:translate-x-5" />
        </Switch.Root>
      </div>
      <div className="mt-6">
        <p className="mb-3 text-base font-bold text-slate-100">主题色</p>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          {colors.map(([value, label, color]) => {
            const selected = settings.theme_primary === value;
            return (
              <button
                aria-pressed={selected}
                className={`flex items-center gap-2 rounded-xl border px-3 py-2.5 text-sm font-semibold transition ${selected ? "border-[var(--theme-primary)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary-light)] shadow-[0_0_0_2px_var(--theme-primary-soft)]" : "border-white/10 text-slate-200 hover:bg-white/[.06]"}`}
                key={value}
                onClick={() => void save({ ...settings, theme_primary: value, theme_secondary: value })}
                type="button"
              >
                <span className="size-4 rounded-full" style={{ background: color }} />
                {label}
                {selected ? <Check className="ml-auto size-4" /> : null}
              </button>
            );
          })}
        </div>
      </div>
    </Card>
  );
}
