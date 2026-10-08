import { useMemo, useState } from "react";
import { Activity, ChevronLeft, ChevronRight, MoreHorizontal, RefreshCw, Trash2 } from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { Avatar } from "../../shared/components/Avatar";
import { AppDialog } from "../../shared/components/AppDialog";
import { PageHeader } from "../../shared/components/PageHeader";
import { Button, EmptyState, Skeleton } from "../../shared/components/ui";
import { dateTime, errorMessage, rulesetLabels } from "../../shared/lib/format";
import type { Ruleset } from "../../shared/types/osu";
import { useCareerActions, useCareerCalendar, useCareerDay, useCareerStatus, useOwnProfile } from "./api";
import { buildPlayActivity, careerToday, changePeriodView, currentPeriod, fillCareerDays, periodRange, recordedDay, shiftPeriod, type CareerPeriod } from "./careerModel";
import { CareerSummary } from "./CareerSummary";
import { CareerTrend } from "./CareerTrend";
import { CareerHeatmap } from "./CareerHeatmap";
import { CareerDayPanel } from "./CareerDayPanel";
import "./career.css";

export function CareerPage() {
  const { ruleset } = useMode();
  const [period, setPeriod] = useState(() => currentPeriod(careerToday()));
  return <CareerWorkspace key={ruleset} ruleset={ruleset} period={period} onPeriodChange={setPeriod} />;
}

function CareerWorkspace({ ruleset, period, onPeriodChange }: { ruleset: Ruleset; period: CareerPeriod; onPeriodChange: (period: CareerPeriod) => void }) {
  const today = careerToday();
  const [selectedDate, setSelectedDate] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [captureNotice, setCaptureNotice] = useState<{ message: string; failed: boolean } | null>(null);
  const profileQuery = useOwnProfile(ruleset);
  const statusQuery = useCareerStatus();
  const { capture, clear } = useCareerActions(ruleset);
  const range = periodRange(period);
  const calendarQuery = useCareerCalendar(ruleset, range.queryStart, range.end);
  const allDays = useMemo(() => fillCareerDays(range.queryStart, range.end, calendarQuery.data?.days ?? []), [calendarQuery.data?.days, range.queryStart, range.end]);
  const days = useMemo(() => allDays.filter((day) => day.date >= range.start && day.date <= today), [allDays, range.start, today]);
  const activities = useMemo(() => buildPlayActivity(allDays, today).filter((item) => item.day.date >= range.start), [allDays, range.start, today]);
  const selected = selectedDate && activities.some((item) => item.day.date === selectedDate && item.state !== "future") ? selectedDate : [...days].reverse().find(recordedDay)?.date ?? null;
  const dayQuery = useCareerDay(ruleset, selected);
  const selectedActivity = activities.find((item) => item.day.date === selected);
  const nextPeriod = shiftPeriod(period, 1);
  const atCurrentPeriod = periodRange(period).start <= today && range.end >= today;
  const changePeriod = (next: CareerPeriod) => { onPeriodChange(next); setSelectedDate(null); };
  const update = async () => {
    setCaptureNotice(null);
    try {
      const result = await capture.mutateAsync();
      setCaptureNotice({ message: result.message || (result.status === "captured" ? "记录已更新" : "部分数据未采集成功"), failed: result.status !== "captured" });
    } catch (error) { setCaptureNotice({ message: errorMessage(error), failed: true }); }
  };
  const clearHistory = async () => {
    try { await clear.mutateAsync(); setSelectedDate(null); setCaptureNotice(null); setConfirmClear(false); }
    catch { /* The dialog keeps the error and retry action visible. */ }
  };
  const profile = profileQuery.data?.data;
  if (profileQuery.isLoading) return <div className="space-y-5"><Skeleton className="h-28" /><Skeleton className="h-72" /></div>;
  if (!profile) return <EmptyState icon={<Activity className="size-5" />} title="暂时无法读取生涯数据" description={profileQuery.error ? errorMessage(profileQuery.error) : "请先登录 osu!，然后重试。"} action={<Button onClick={() => void profileQuery.refetch()}>重试</Button>} />;

  return <div className="career-page">
    <PageHeader title="生涯动态" actions={<div className="career-page-actions"><Button disabled={!statusQuery.data?.configured || clear.isPending} loading={capture.isPending} onClick={() => void update()} size="sm"><RefreshCw className="size-4" />更新记录</Button><details className="career-more"><summary aria-label="更多"><MoreHorizontal className="size-5" /></summary><div><Button disabled={capture.isPending || clear.isPending} onClick={() => setConfirmClear(true)} size="sm" variant="ghost"><Trash2 className="size-4" />清空历史</Button></div></details></div>} />
    <div className="career-toolbar">
      <div className="career-identity"><Avatar profile={profile} className="size-9 rounded-lg" /><div><p><strong>{profile.username}</strong><span>{rulesetLabels[ruleset]}</span></p><p className="career-muted">{capture.isPending ? "正在更新记录…" : statusQuery.data?.latest_date ? `最近记录 ${statusQuery.data.latest_date}` : "尚未保存记录"}</p></div></div>
      <div className="career-period-controls"><div aria-label="时间视图" className="career-segments">{(["month", "year"] as const).map((view) => <button aria-pressed={period.view === view} key={view} onClick={() => changePeriod(changePeriodView(period, view, today))} type="button">{view === "month" ? "月" : "年"}</button>)}</div><div className="career-period-navigation"><Button aria-label={period.view === "month" ? "上个月" : "上一年"} onClick={() => changePeriod(shiftPeriod(period, -1))} size="icon" variant="ghost"><ChevronLeft className="size-4" /></Button><span aria-live="polite">{period.year} 年{period.view === "month" ? ` ${period.month + 1} 月` : ""}</span><Button aria-label={period.view === "month" ? "下个月" : "下一年"} disabled={periodRange(nextPeriod).start > today} onClick={() => changePeriod(nextPeriod)} size="icon" variant="ghost"><ChevronRight className="size-4" /></Button></div><Button disabled={atCurrentPeriod} onClick={() => changePeriod({ ...currentPeriod(today), view: period.view })} size="sm" variant="ghost">回到今天</Button></div>
    </div>
    {captureNotice ? <div className={`career-notice ${captureNotice.failed ? "career-warning" : ""}`} role={captureNotice.failed ? "alert" : "status"}><span>{captureNotice.message}</span>{captureNotice.failed ? <Button loading={capture.isPending} onClick={() => void update()} size="sm">重试更新</Button> : null}</div> : null}
    {statusQuery.error ? <div className="career-notice career-warning" role="alert"><span>记录状态加载失败：{errorMessage(statusQuery.error)}</span><Button onClick={() => void statusQuery.refetch()} size="sm">重试状态</Button></div> : statusQuery.data && !statusQuery.data.configured ? <p className="career-notice career-warning">请先在设置的“本地数据”中配置数据库，才能保存生涯记录。</p> : statusQuery.data?.last_error && !captureNotice ? <p className="career-notice career-warning" role="status">最近采集遇到问题：{statusQuery.data.last_error}</p> : null}
    {calendarQuery.isLoading ? <><Skeleton className="h-32" /><Skeleton className="h-72" /><Skeleton className="h-40" /></> : calendarQuery.error ? <div className="career-panel career-inline-empty" role="alert"><strong>生涯记录加载失败</strong><p>{errorMessage(calendarQuery.error)}</p><Button onClick={() => void calendarQuery.refetch()}>重试生涯记录</Button></div> : <>
      <CareerSummary days={days} today={today} />
      <CareerTrend days={days} selected={selected} onSelect={setSelectedDate} />
      <CareerHeatmap activities={activities} period={period} selected={selected} onSelect={setSelectedDate} />
      <CareerDayPanel key={selected} date={selected} detail={dayQuery.data} activity={selectedActivity} loading={dayQuery.isLoading} error={dayQuery.error} onRetry={() => void dayQuery.refetch()} />
      <p className="career-page-foot">历史记录来自本地保存的快照{selectedActivity?.day.captured_at ? ` · 所选记录 ${dateTime(selectedActivity.day.captured_at)}` : ""}</p>
    </>}
    <AppDialog open={confirmClear} onOpenChange={(open) => { if (clear.isPending) return; setConfirmClear(open); if (!open) clear.reset(); }} title="清空全部生涯历史？" description="会删除所有模式的本地生涯记录，此操作不可恢复。" size="sm" closeDisabled={clear.isPending} footer={<><Button disabled={clear.isPending} onClick={() => { setConfirmClear(false); clear.reset(); }}>取消</Button><Button disabled={capture.isPending} loading={clear.isPending} onClick={() => void clearHistory()} variant="danger">清空全部历史</Button></>}>
      <p className="career-muted">确认后，后续采集会从新的记录开始。</p>{clear.error ? <p className="career-warning mt-3" role="alert">清空失败：{errorMessage(clear.error)}。可以重试。</p> : null}
    </AppDialog>
  </div>;
}
