import { Trash2 } from "lucide-react";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import type { AppSettings } from "../../../shared/types/osu";

export function ToolsCachePanel({
  className,
  settings,
  save,
  busy,
}: {
  className?: string;
  settings: AppSettings;
  save: (next: AppSettings) => Promise<void>;
  busy: boolean;
}) {
  return (
    <Card className={className}>
      <SectionTitle title="工具与缓存" />
      <div className="mt-5 space-y-3">
        <div className="flex flex-wrap gap-3">
          <Button disabled={busy} onClick={() => void desktopApi.clearProfileCache()}>
            <Trash2 className="size-4" />清除缓存
          </Button>
        </div>
        <label className="block rounded-xl border border-white/[.1] bg-white/[.035] p-4">
          <span className="block text-sm font-semibold text-slate-100">本地缩略图缓存上限</span>
          <span className="mt-1 block text-xs leading-5 text-slate-500">超过上限时会自动清理最早的谱面背景缩略图；索引和你的谱面文件不会被删除。</span>
          <div className="mt-3 flex items-center gap-2">
            <input
              aria-label="本地缩略图缓存上限（MB）"
              className="opp-input w-32"
              disabled={busy}
              max={10240}
              min={64}
              onChange={(event) => void save({ ...settings, cache_limit_mb: Math.max(64, Math.min(10240, Number(event.target.value) || 64)) })}
              step={64}
              type="number"
              value={settings.cache_limit_mb}
            />
            <span className="text-sm text-slate-400">MB</span>
          </div>
        </label>
      </div>
    </Card>
  );
}
