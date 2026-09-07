import * as Switch from "@radix-ui/react-switch";
import { Gamepad2 } from "lucide-react";
import { Card, SectionTitle, InfoTip } from "../../../shared/components/ui";
import { useMode } from "../../../app/ModeContext";
import type { AppSettings, ThemeColor, Ruleset } from "../../../shared/types/osu";

const colors: Array<[ThemeColor, string, string]> = [
  ["cyan", "青色", "#67e8f9"],
  ["blue", "蓝色", "#60a5fa"],
  ["violet", "紫罗兰", "#a78bfa"],
  ["pink", "粉色", "#f472b6"],
  ["orange", "橙色", "#fb923c"],
  ["green", "绿色", "#4ade80"],
];

const modes: Array<[Ruleset, string]> = [
  ["osu", "osu!"],
  ["taiko", "Taiko"],
  ["fruits", "Catch"],
  ["mania", "Mania"],
];

interface AppearancePanelProps {
  settings: AppSettings;
  save: (settings: AppSettings) => Promise<void>;
}

export function AppearancePanel({ settings, save }: AppearancePanelProps) {
  const { ruleset, setRuleset } = useMode();
  const lightTheme = settings.theme_mode === "light";

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-2xl font-bold text-white">外观</h2>
        <p className="mt-1 text-sm text-slate-400">
          自定义应用主题色彩和默认游戏模式。
        </p>
      </div>

      <Card className="p-6">
        <SectionTitle title="主题" />

        <div className="mt-5 flex items-center justify-between rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
          <div className="flex items-center gap-2">
            <p className="font-semibold text-slate-100">浅色主题</p>
            <InfoTip text="开启后使用浅色界面；关闭时使用默认深色界面。" />
          </div>

          <Switch.Root
            checked={lightTheme}
            onCheckedChange={(checked) =>
              void save({ ...settings, theme_mode: checked ? "light" : "dark" })
            }
            className="peer relative h-7 w-12 cursor-pointer rounded-full bg-white/20 shadow-inner outline-none transition-colors data-[state=checked]:bg-[var(--theme-primary)] focus-visible:ring-2 focus-visible:ring-[var(--theme-primary-soft)]"
          >
            <Switch.Thumb className="block size-5 translate-x-1 rounded-full bg-white shadow-lg transition-transform will-change-transform data-[state=checked]:translate-x-6" />
          </Switch.Root>
        </div>

        <div className="mt-5 space-y-3">
          <p className="text-sm font-semibold text-slate-200">主色调</p>
          <div className="grid grid-cols-3 gap-2">
            {colors.map(([value, label, hex]) => (
              <button
                key={value}
                onClick={() => void save({ ...settings, theme_primary: value })}
                className="group relative flex items-center gap-3 rounded-lg border border-white/[0.08] bg-white/[0.02] p-3 transition-all hover:border-white/20 hover:bg-white/[0.06] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary-soft)]"
                type="button"
              >
                <span
                  className="size-6 shrink-0 rounded-full shadow-sm ring-2 ring-white/30"
                  style={{ backgroundColor: hex }}
                />
                <span className="text-sm text-slate-200 group-hover:text-white">
                  {label}
                </span>
                {settings.theme_primary === value ? (
                  <span className="ml-auto size-5 rounded-full bg-[var(--theme-primary)] text-[10px] font-bold leading-5 text-white">
                    ✓
                  </span>
                ) : null}
              </button>
            ))}
          </div>
        </div>
      </Card>

      <Card className="p-6">
        <SectionTitle
          title="默认游戏模式"
          description="选择应用打开时优先使用的 osu! 游戏模式。"
        />

        <div className="mt-4 grid grid-cols-2 gap-2">
          {modes.map(([value, label]) => (
            <button
              key={value}
              onClick={() => setRuleset(value)}
              className="group relative flex items-center gap-3 rounded-lg border border-white/[0.08] bg-white/[0.02] p-3 transition-all hover:border-white/20 hover:bg-white/[0.06] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary-soft)]"
              type="button"
            >
              <Gamepad2 className="size-5 text-slate-400 group-hover:text-slate-300" />
              <span className="text-sm text-slate-200 group-hover:text-white">
                {label}
              </span>
              {ruleset === value ? (
                <span className="ml-auto size-5 rounded-full bg-[var(--theme-primary)] text-[10px] font-bold leading-5 text-white">
                  ✓
                </span>
              ) : null}
            </button>
          ))}
        </div>
      </Card>
    </div>
  );
}
