import { useState } from "react";
import { MonitorCog, RotateCcw } from "lucide-react";
import { ErrorPanel } from "../../../shared/components/ErrorPanel";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";

export function DisplayGammaCard() {
  const [gamma, setGamma] = useState(1);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const apply = async (value: number) => { setApplying(true); setError(null); try { await desktopApi.setDisplayGamma(value); } catch (caught) { setError(caught); } finally { setApplying(false); } };
  const resetGamma = () => { setGamma(1); void apply(1); };
  return <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><MonitorCog className="size-5" /></div><SectionTitle title="显示器伽马" description="直接使用 Windows 显示接口调整主显示器的伽马。1.00 为默认值，数值越高会提亮中间调。" /></div><div className="mt-6 flex items-center gap-4"><input aria-label="显示器伽马" className="h-2 w-full cursor-pointer accent-[var(--theme-primary)]" type="range" min="0.5" max="2.5" step="0.01" value={gamma} onChange={(event) => setGamma(Number(event.target.value))} onKeyUp={() => void apply(gamma)} onPointerUp={() => void apply(gamma)} /><output className="w-12 shrink-0 text-right text-sm font-semibold tabular-nums text-white">{gamma.toFixed(2)}</output><Button disabled={applying || gamma === 1} loading={applying} onClick={resetGamma} size="sm"><RotateCcw className="size-3.5" />复原</Button></div><p className="mt-3 text-xs leading-5 text-slate-400">松开滑块后应用，避免显示驱动积压请求；关闭或重启显示驱动后，Windows 可能会恢复系统默认伽马。</p>{error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}</Card>;
}
