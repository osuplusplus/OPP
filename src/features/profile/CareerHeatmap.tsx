import { useRef, useState, type KeyboardEvent } from "react";
import { fullNumber } from "../../shared/lib/format";
import { activityDescription, weekday, type CareerPeriod, type PlayActivity } from "./careerModel";

const weekdays = ["日", "一", "二", "三", "四", "五", "六"];

export function CareerHeatmap({ activities, period, selected, onSelect }: {
  activities: PlayActivity[]; period: CareerPeriod; selected: string | null; onSelect: (date: string) => void;
}) {
  const [hovered, setHovered] = useState<string | null>(null);
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  const offset = activities.length ? weekday(activities[0].day.date) : 0;
  const cells = [...Array.from({ length: offset }, () => null), ...activities];
  const current = activities.find((item) => item.day.date === hovered) ?? activities.find((item) => item.day.date === selected);
  const focusDate = selected ?? [...activities].reverse().find((item) => item.state !== "future")?.day.date;
  const recorded = activities.filter((item) => item.state === "recorded" || item.state === "unknown");
  const activeDays = activities.filter((item) => item.count != null && item.count > 0 && !item.spanning).length;
  const keyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const steps: Record<string, number> = period.view === "year" ? { ArrowUp: -1, ArrowDown: 1, ArrowLeft: -7, ArrowRight: 7 } : { ArrowUp: -7, ArrowDown: 7, ArrowLeft: -1, ArrowRight: 1 };
    const target = event.key === "Home" ? 0 : event.key === "End" ? activities.length - 1 : event.key in steps ? index + steps[event.key] : null;
    if (target == null) return;
    event.preventDefault();
    let next = Math.max(0, Math.min(target, activities.length - 1));
    while (next > 0 && activities[next].state === "future") next -= 1;
    buttons.current.get(activities[next].day.date)?.focus();
  };
  return <section aria-label="游玩记录" className="career-panel career-activity">
    <div className="career-section-header"><div><h2>游玩记录</h2><p className="career-muted">以新增游玩次数着色，点击日期回看当天</p></div><span className="career-muted">{recorded.length} 天有记录</span></div>
    <div className={`career-heatmap-layout career-heatmap-${period.view}`}>
      <div className="career-heatmap-scroll">
        {period.view === "year" ? <div className="career-heatmap-months" style={{ gridTemplateColumns: `repeat(${Math.ceil(cells.length / 7)}, minmax(0, 1fr))` }}>{activities.filter((item) => item.day.date.endsWith("-01")).map((item) => <span key={item.day.date} style={{ gridColumn: Math.floor((offset + activities.indexOf(item)) / 7) + 1 }}>{Number(item.day.date.slice(5, 7))}月</span>)}</div> : null}
        <div className="career-heatmap-body">
          <div className="career-weekdays">{weekdays.map((label) => <span key={label}>{label}</span>)}</div>
          <div aria-label={`${period.year}年${period.view === "month" ? `${period.month + 1}月` : ""}游玩热力图`} className="career-heatmap-grid" style={period.view === "year" ? { gridTemplateColumns: `repeat(${Math.ceil(cells.length / 7)}, minmax(0, 1fr))` } : undefined}>
            {cells.map((item, cellIndex) => {
              if (!item) return <span aria-hidden="true" key={`padding-${cellIndex}`} />;
              const index = cellIndex - offset;
              const description = activityDescription(item);
              return <button aria-label={`${item.day.date} · ${description}`} aria-pressed={item.day.date === selected} className={`career-heat-cell career-heat-${item.state} career-heat-level-${item.level}${item.spanning ? " career-heat-spanning" : ""}${item.day.status === "partial" ? " career-heat-partial" : ""}`} data-date={item.day.date} disabled={item.state === "future"} key={item.day.date}
                onClick={() => onSelect(item.day.date)} onFocus={() => setHovered(item.day.date)} onBlur={() => setHovered(null)} onMouseEnter={() => setHovered(item.day.date)} onMouseLeave={() => setHovered(null)} onKeyDown={(event) => keyDown(event, index)} ref={(element) => { if (element) buttons.current.set(item.day.date, element); else buttons.current.delete(item.day.date); }} tabIndex={item.day.date === focusDate ? 0 : -1} title={`${item.day.date} · ${description}`} type="button">
                {period.view === "month" ? <span>{Number(item.day.date.slice(-2))}</span> : item.state === "failed" ? <span>!</span> : item.state === "unknown" ? <span>?</span> : null}
                {period.view === "month" && (item.state === "failed" || item.state === "unknown") ? <small aria-hidden="true" className="career-heat-mark">{item.state === "failed" ? "!" : "?"}</small> : null}
              </button>;
            })}
          </div>
        </div>
      </div>
      {period.view === "month" ? <div className="career-activity-caption"><p className="career-label">本月游玩足迹</p><p><strong>{fullNumber(activeDays)}</strong> 天确认有游玩新增</p><p className="career-muted">深色代表更多新增游玩。斜纹代表跨日累计，不会将次数分摊到空白日期。</p><p className="career-muted">虚线为未采集，问号为次数无法确定；角标代表部分采集。</p></div> : null}
    </div>
    <div aria-live="polite" className="career-heatmap-readout">{current ? <><strong>{current.day.date}</strong><span>{activityDescription(current)}</span></> : <span>悬浮或用方向键选择日期，查看新增次数</span>}</div>
    <div className="career-heatmap-legend"><div><span>少</span>{[0, 1, 2, 3, 4].map((level) => <span aria-label={["0 次", "1–19 次", "20–49 次", "50–99 次", "100 次以上"][level]} className={`career-legend-cell career-heat-recorded career-heat-level-${level}`} key={level} title={["0 次", "1–19 次", "20–49 次", "50–99 次", "100 次以上"][level]} />)}<span>多</span></div><span>虚线 未采集 · ? 未知 · ! 失败 · 斜纹 跨日累计 · 角标 部分采集</span></div>
  </section>;
}
