import { useState } from "react";
import * as Switch from "@radix-ui/react-switch";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";
import type { AppSettings, CommandError, OsuClient } from "../../../shared/types/osu";
import { useLocalSources } from "../../local-analysis/api";
import { LocalDatabaseCard } from "../../local-database/DatabaseSetup";
import { LocalScanAction } from "../LocalScanAction";

export function LocalDataPanel({ settings, save, busy, onConfigure }: {
  settings: AppSettings; save: (settings: AppSettings) => Promise<void>; busy: boolean; onConfigure: () => void;
}) {
  const sources = useLocalSources();
  const [error, setError] = useState<string | null>(null);
  const update = async (patch: Partial<AppSettings>) => {
    setError(null);
    try { await save({ ...settings, ...patch }); }
    catch (caught) { setError((caught as CommandError).message ?? "无法保存本地数据设置"); }
  };
  return <div className="space-y-5">
    <LocalDatabaseCard />
    <Card className="p-6">
      <SectionTitle title="本地索引与扫描" description="读取 Stable 和 lazer 的谱面与皮肤信息，扫描结果自动增量写入索引。" />
      <p className="mt-3 text-sm text-slate-400">游戏文件变化会自动触发增量扫描，也可以手动更新。导入旧索引与扫描游戏文件是两个独立操作。</p>
      <div className="mt-4 grid gap-4 md:grid-cols-2">
        {(["stable", "lazer"] as OsuClient[]).map((client) => {
          const source = sources.data?.find((item) => item.client === client);
          const name = client === "stable" ? "osu! Stable" : "osu!lazer";
          return <section key={client} aria-label={`${name} 本地索引`} className="rounded-xl border border-white/10 p-4">
            <h3 className="text-sm font-semibold text-slate-100">{name}</h3>
            <p className="mt-2 break-all text-xs text-slate-400">{sources.isPending ? "正在读取游戏目录…" : source?.data_root ?? source?.configured_path ?? "尚未找到游戏目录"}</p>
            {source && !source.valid ? <p className="mt-2 text-xs text-amber-300">{source.validation_errors.join("；")}</p> : null}
            <div className="mt-3"><LocalScanAction client={client} onConfigure={onConfigure} allowRescan /></div>
          </section>;
        })}
      </div>
      {sources.error ? <p role="alert" className="mt-3 text-sm text-rose-300">{sources.error.message ?? "无法读取游戏目录"}</p> : null}
      <Button className="mt-4" size="sm" onClick={onConfigure}>配置游戏目录</Button>
    </Card>
    <Card className="p-6">
      <SectionTitle title="在线谱面的本地已有标记" description="按最近扫描的谱面 ID 匹配具体难度；文件版本可能不同，未扫描或不可访问时显示未知。" />
      <div className="mt-4 flex items-center justify-between gap-4 rounded-xl border border-white/10 p-4">
        <span className="text-sm font-semibold text-slate-100">显示本地已有谱面</span>
        <Switch.Root aria-label="显示本地已有谱面" disabled={busy} checked={settings.show_local_beatmap_presence !== false} onCheckedChange={(value) => void update({ show_local_beatmap_presence: value })} className="relative h-6 w-11 shrink-0 rounded-full bg-slate-500 data-[state=checked]:bg-[var(--theme-primary)] disabled:opacity-45">
          <Switch.Thumb className="block size-5 translate-x-0.5 rounded-full bg-white transition-transform data-[state=checked]:translate-x-5" />
        </Switch.Root>
      </div>
      <label className="mt-4 block text-sm text-slate-300">匹配客户端
        <select aria-label="本地谱面匹配客户端" className="opp-input mt-2 w-full" disabled={busy} value={settings.local_beatmap_presence_scope ?? "all"} onChange={(event) => void update({ local_beatmap_presence_scope: event.target.value as "all" | OsuClient })}>
          <option value="all">Stable 与 lazer</option><option value="stable">仅 Stable</option><option value="lazer">仅 lazer</option>
        </select>
      </label>
      {error ? <p role="alert" className="mt-3 text-sm text-rose-300">{error}</p> : null}
    </Card>
  </div>;
}
