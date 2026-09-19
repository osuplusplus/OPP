import { useCallback, useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ExternalLink, FolderOpen, Play, RefreshCw, Save, Square } from "lucide-react";
import { Badge, Button, Card, DataLine, SectionTitle } from "../../shared/components/ui";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { desktopApi } from "../../shared/lib/tauri";
import type { OtdStatus } from "../../shared/types/osu";
import { settingsQueryKey, useSettings } from "../settings/api";

export function OtdPanel() {
  const [status, setStatus] = useState<OtdStatus | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const settings = useSettings();
  const queryClient = useQueryClient();
  const refresh = useCallback(async () => { try { setStatus(await desktopApi.getOtdStatus()); setError(null); } catch (caught) { setError(caught); } }, []);
  useEffect(() => { void refresh(); }, [refresh]);
  const run = async (name: string, action: () => Promise<unknown>) => { setBusy(name); setError(null); try { await action(); await refresh(); } catch (caught) { setError(caught); } finally { setBusy(null); } };
  const choose = async () => { const path = await desktopApi.chooseOtdExecutable(status?.executable_path); if (path) await run("choose", () => desktopApi.setOtdExecutable(path)); };
  const backup = async () => { const directory = await desktopApi.chooseLocalDirectory(null); if (directory) await run("backup", () => desktopApi.backupOtdConfig(directory)); };
  const toggleLaunch = async (checked: boolean) => { if (!settings.data) return; setBusy("toggle"); try { const saved = await desktopApi.updateSettings({ ...settings.data, launch_otd_with_game: checked }); queryClient.setQueryData(settingsQueryKey, saved); } catch (caught) { setError(caught); } finally { setBusy(null); } };
  return <Card className="p-6"><div className="flex items-start justify-between gap-4"><SectionTitle title="OpenTabletDriver" description="检测数位板驱动状态，启动或备份 OTD 配置。" /><Badge tone={status?.daemon_running ? "success" : status?.installed ? "warning" : "neutral"}>{status?.daemon_running ? "运行中" : status?.installed ? "未运行" : "未配置"}</Badge></div><div className="mt-4 space-y-2"><DataLine label="可执行文件" value={<span className="truncate font-mono text-xs">{status?.executable_path ?? "未选择"}</span>} /><DataLine label="设备" value={status?.tablet_name ?? "未检测到"} /><DataLine label="当前配置" value={status?.config_path ?? "未检测到"} /><DataLine label="输出模式" value={status?.config_summary?.output_mode ?? "未知"} /><DataLine label="活动区域" value={status?.config_summary?.area ?? "未知"} /></div><label className="mt-4 flex items-center gap-2 text-sm text-slate-300"><input checked={settings.data?.launch_otd_with_game ?? false} disabled={busy === "toggle"} onChange={(event) => void toggleLaunch(event.target.checked)} type="checkbox" />启动 osu! 前自动启动 OTD</label><div className="mt-5 flex flex-wrap gap-2"><Button loading={busy === "choose"} onClick={() => void choose()} size="sm" variant="secondary"><FolderOpen className="size-4" />选择 OTD</Button><Button disabled={!status?.installed || status.daemon_running} loading={busy === "start"} onClick={() => void run("start", desktopApi.startOtd)} size="sm"><Play className="size-4" />启动</Button><Button disabled={!status?.owned_by_opp} loading={busy === "stop"} onClick={() => void run("stop", desktopApi.stopOtd)} size="sm" variant="danger"><Square className="size-4" />停止</Button><Button disabled={busy !== null} onClick={() => void refresh()} size="sm" variant="ghost"><RefreshCw className="size-4" />刷新</Button><Button disabled={!status?.config_path} loading={busy === "backup"} onClick={() => void backup()} size="sm" variant="secondary"><Save className="size-4" />备份配置</Button><Button disabled={!status?.executable_path} onClick={() => void desktopApi.openOtd()} size="sm" variant="ghost"><ExternalLink className="size-4" />打开 OTD</Button></div>{error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}</Card>;
}
