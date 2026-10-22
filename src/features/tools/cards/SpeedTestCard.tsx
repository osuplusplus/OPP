import { useEffect, useRef, useState } from "react";
import { Keyboard, Square } from "lucide-react";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";

export function SpeedTestCard() {
  return <><SpeedTestContent /></>;
}

function SpeedTestContent() {
  const [active, setActive] = useState(false);
  const [armed, setArmed] = useState(false);
  const [binding, setBinding] = useState(false);
  const [testKeys, setTestKeys] = useState<string[]>(["KeyZ", "KeyX"]);
  const [presses, setPresses] = useState(0);
  const [duration, setDuration] = useState(10);
  const [remaining, setRemaining] = useState(10);
  const [result, setResult] = useState<number | null>(null);
  const [terminated, setTerminated] = useState(false);
  const [liveKps, setLiveKps] = useState(0);
  const [peakKps, setPeakKps] = useState(0);
  const [buckets, setBuckets] = useState<number[]>([]);
  const startedAt = useRef(0);
  const pressTimesRef = useRef<number[]>([]);

  useEffect(() => {
    if (!active) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!event.repeat && testKeys.includes(event.code)) {
        event.preventDefault();
        pressTimesRef.current.push(performance.now());
        setPresses(pressTimesRef.current.length);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    const interval = window.setInterval(() => {
      const now = performance.now();
      const elapsed = (now - startedAt.current) / 1000;
      const next = Math.max(0, duration - elapsed);
      setRemaining(next);
      const times = pressTimesRef.current;
      // 实时 KPS：最近 1 秒滚动窗口内的按键数。
      const rolling = times.filter((time) => now - time <= 1000).length;
      setLiveKps(rolling);
      setPeakKps((current) => Math.max(current, rolling));
      // 每秒一档的按键直方图，随测试实时更新。
      const seconds = Math.max(1, Math.ceil(Math.min(elapsed + 0.05, duration)));
      const counts = Array.from({ length: seconds }, () => 0);
      for (const time of times) {
        counts[Math.min(seconds - 1, Math.floor((time - startedAt.current) / 1000))] += 1;
      }
      setBuckets(counts);
      if (next === 0) {
        window.clearInterval(interval);
        setActive(false);
        setResult(times.length / duration);
      }
    }, 50);
    return () => { window.removeEventListener("keydown", onKeyDown); window.clearInterval(interval); };
  }, [active, duration, testKeys]);

  useEffect(() => {
    if (!binding) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") { setBinding(false); return; }
      if (event.repeat || ["Control", "Alt", "Shift", "Meta"].includes(event.key)) return;
      event.preventDefault();
      setTestKeys((current) => current.includes(event.code) ? current.filter((key) => key !== event.code) : [...current, event.code]);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [binding]);

  const resetTest = () => {
    pressTimesRef.current = [];
    setPresses(0);
    setRemaining(duration);
    setResult(null);
    setTerminated(false);
    setLiveKps(0);
    setPeakKps(0);
    setBuckets([]);
  };
  const armTest = () => {
    resetTest();
    setArmed(true);
  };
  const terminate = () => { setTerminated(true); setActive(false); };

  useEffect(() => {
    if (!armed) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat || !testKeys.includes(event.code)) return;
      event.preventDefault();
      const firstPressAt = performance.now();
      startedAt.current = firstPressAt;
      pressTimesRef.current = [firstPressAt];
      setPresses(1);
      setArmed(false);
      setActive(true);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [armed, testKeys]);

  const bpm = active ? liveKps * 15 : result == null ? null : result * 15;
  const chartMax = Math.max(1, ...buckets);
  // 折线图：x 轴按测试时长铺满，每秒一档的按键数归一化到 0-100。
  const linePoints = buckets.map((count, index) => `${(index / Math.max(1, duration - 1)) * 100},${100 - (count / chartMax) * 100}`).join(" ");
  const lineEndX = ((buckets.length - 1) / Math.max(1, duration - 1)) * 100;
  const status = active ? "测试中" : armed ? "等待测试按键" : terminated ? "已终止（不计成绩）" : result == null ? "准备开始" : "本次成绩";

  return <Card className="p-6">
    <div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><Keyboard className="size-5" /></div><SectionTitle title="手速测试" description="仅统计绑定的测试按键；实时 KPS 取最近 1 秒滚动窗口，BPM 按四分之一拍换算。" /></div>
    <div className="mt-5 flex flex-wrap items-center gap-3"><span className="text-xs text-slate-500">测试时长</span>{[5, 10, 15, 30].map((value) => <button className={`rounded-lg border px-3 py-1.5 text-xs font-semibold ${duration === value ? "border-[var(--theme-primary)] bg-[var(--theme-primary-muted)] text-white" : "border-white/[0.08] text-slate-400"}`} disabled={active || armed} key={value} onClick={() => { setDuration(value); setRemaining(value); setResult(null); setTerminated(false); }} type="button">{value}s</button>)}</div>
    <div className="mt-3 flex flex-wrap items-center gap-2"><Button disabled={active || armed} onClick={() => setBinding((current) => !current)} size="sm" variant="secondary">{binding ? "完成绑定" : "绑定测试按键"}</Button><span className="font-mono text-xs text-cyan-200">{testKeys.length ? testKeys.join(" + ").replace(/Key/g, "") : "未绑定"}</span></div>
    {binding ? <p className="mt-2 text-xs text-amber-200">按一个键以添加或移除；按 Esc 结束绑定。</p> : null}
    <div className="mt-4 rounded-2xl border border-white/[0.08] bg-black/[0.12] p-5">
      <div className="flex items-end justify-between gap-5">
        <div>
          <p className="text-xs uppercase tracking-[0.16em] text-slate-500">{status}</p>
          <p className="mt-1 text-3xl font-semibold tabular-nums text-white">{active || terminated ? `${liveKps.toFixed(1)} KPS` : result == null ? "—" : `${result.toFixed(1)} KPS`}</p>
          {bpm != null ? <p className="mt-1 text-sm font-semibold text-pink-200">≈ {bpm.toFixed(0)} BPM（四分之一拍）</p> : null}
        </div>
        <div className="text-right">
          <p className="text-xs text-slate-500">剩余时间</p>
          <p className="mt-1 text-xl font-semibold tabular-nums text-[var(--theme-primary-light)]">{remaining.toFixed(1)}s</p>
          <p className="mt-1 text-xs text-slate-500">峰值 {peakKps.toFixed(1)} KPS · 已按 {presses} 次</p>
        </div>
      </div>
      <div className="mt-4 h-2 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-[width]" style={{ width: `${((duration - remaining) / duration) * 100}%` }} /></div>
      {buckets.length ? <div className="mt-4">
        <p className="text-xs text-slate-500">每秒按键数</p>
        <svg className="mt-2 h-24 w-full" preserveAspectRatio="none" viewBox="0 0 100 100">
          {buckets.length >= 2 ? <>
            <polygon fill="var(--theme-primary)" fillOpacity="0.12" points={`0,100 ${linePoints} ${lineEndX},100`} />
            <polyline fill="none" points={linePoints} stroke="var(--theme-primary)" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" vectorEffect="non-scaling-stroke" />
          </> : null}
        </svg>
      </div> : null}
      <div className="mt-5 flex flex-wrap gap-3">
        {active
          ? <Button onClick={terminate} variant="danger"><Square className="size-4" />终止测试</Button>
          : armed
            ? <><Button disabled variant="primary">按 {testKeys.join(" / ").replace(/Key/g, "")} 开始计时</Button><Button onClick={() => setArmed(false)} variant="ghost">取消等待</Button></>
            : <Button disabled={!testKeys.length || binding} onClick={armTest} variant="primary">{result == null && !terminated ? `开始 ${duration} 秒测试` : "再测一次"}</Button>}
      </div>
    </div>
  </Card>;
}
