import { fullNumber } from "../../shared/lib/format";
import type { CareerCalendarDay } from "../../shared/types/osu";
import { describeDelta, formatCareerStat, periodSummary } from "./careerModel";

export function CareerSummary({ days, today }: { days: CareerCalendarDay[]; today: string }) {
  const summary = periodSummary(days, today);
  const comparable = summary.count > 1;
  const comparison = comparable ? `${summary.first!.date} → ${summary.last!.date}` : "保存更多记录后即可比较成长";
  return <section aria-label="期间总览" className="career-summary">
    <div className="career-summary-stats">
      {(["pp", "global_rank"] as const).map((key) => {
        const delta = describeDelta(key, summary.last?.stats?.[key], comparable ? summary.first?.stats?.[key] : null);
        return <div className="career-summary-stat" key={key}>
          <p className="career-label">{key === "pp" ? "PERFORMANCE · PP" : "全球排名"}</p>
          <p className={`career-summary-value ${key === "pp" ? "career-primary" : ""}`}>{formatCareerStat(key, summary.last?.stats?.[key])}</p>
          <p className={`career-delta career-delta-${delta.tone}`}>{delta.text}</p>
        </div>;
      })}
      <div className="career-summary-stat"><p className="career-label">新增游玩</p><p className="career-summary-value">{fullNumber(summary.plays)}<small>次</small></p><p className="career-muted">{summary.plays == null ? "暂无可靠对比" : "真实记录间累计新增"}</p></div>
      <div className="career-summary-stat"><p className="career-label">BP 新增记录</p><p className="career-summary-value">{summary.count || summary.addedScores ? fullNumber(summary.addedScores) : "—"}<small>条</small></p><p className="career-muted">Top 200 新增记录</p></div>
    </div>
    <div className="career-summary-foot"><span>{comparison}</span><span>{summary.count} 天有统计记录</span></div>
  </section>;
}
