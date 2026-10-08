import { Award, ExternalLink, FileImage, Film } from "lucide-react";
import { Button, Skeleton } from "../../shared/components/ui";
import { notification } from "../../shared/components/notifications";
import { desktopApi } from "../../shared/lib/tauri";
import { dateTime, errorMessage, scoreMods } from "../../shared/lib/format";
import type { CareerDayDetail, CareerStats } from "../../shared/types/osu";
import { activityDescription, careerStatLabels, describeDelta, formatCareerStat, type PlayActivity } from "./careerModel";

function Stat({ stat, stats, previous }: { stat: [keyof CareerStats, string]; stats: CareerStats; previous: CareerStats | null }) {
  const [key, label] = stat;
  const delta = describeDelta(key, stats[key], previous?.[key]);
  return <div className="career-day-stat"><dt>{label}</dt><dd>{formatCareerStat(key, stats[key])}</dd><p className={`career-delta career-delta-${delta.tone}`}>{delta.text}</p></div>;
}

export function CareerDayPanel({ date, detail, activity, loading, error, onRetry }: {
  date: string | null; detail?: CareerDayDetail; activity?: PlayActivity; loading: boolean; error: unknown; onRetry: () => void;
}) {
  const scores = detail?.score_diffs.filter((item) => item.kind === "added" || item.kind === "removed") ?? [];
  const events = (detail?.medal_events.length ?? 0) + (detail?.media_events.length ?? 0) + scores.length;
  return <section aria-label="当天变化" className="career-panel career-day-panel">
    <div className="career-section-header"><div><h2>{date ?? "选择日期"}<span className="career-muted career-day-heading">当天变化</span></h2><p className="career-muted">{detail?.captured_at ? `记录于 ${dateTime(detail.captured_at)} · 变化相较上次记录` : "从游玩记录或趋势图选择日期"}</p></div>{activity ? <span className="career-day-status">{activityDescription(activity)}</span> : null}</div>
    {loading ? <Skeleton className="mt-5 h-36" /> : error ? <div className="career-inline-empty" role="alert"><strong>当天详情加载失败</strong><p>{errorMessage(error)}</p><Button onClick={onRetry} size="sm">重试当天详情</Button></div>
      : !date ? <div className="career-inline-empty"><strong>还没有历史记录</strong><p>更新记录后，即可回看统计和成绩变化。</p></div>
      : !detail?.stats && !events ? <div className="career-inline-empty"><strong>{detail?.status === "unavailable" || detail?.status === "partial" ? "这一天采集失败" : "这一天没有保存记录"}</strong><p>{detail?.error || "未保存的日期无法还原当天的成绩和游玩次数。"}</p></div>
      : <>
        {detail?.status === "partial" || detail?.error ? <p className="career-warning" role="status">部分数据未采集成功{detail.error ? `：${detail.error}` : "，以下仅展示已保存的内容。"}</p> : null}
        {detail?.stats ? <dl className="career-day-stats">{careerStatLabels.slice(0, 4).map((stat) => <Stat key={stat[0]} stat={stat} stats={detail.stats!} previous={detail.previous_stats} />)}</dl> : null}
        <div className="career-events">
          <h3>BP 变化<span className="career-muted">{scores.length} 条</span></h3>
          {scores.length ? scores.map((item) => {
            const score = item.score;
            const title = score.beatmapset?.title ?? score.beatmap?.version ?? `成绩 #${score.id ?? "—"}`;
            const position = item.kind === "added" ? item.after_position : item.before_position;
            return <div className="career-score-event" key={`${item.kind}-${item.key}`}>
              <span className={`career-event-kind career-delta-${item.kind === "added" ? "positive" : "negative"}`}>{item.kind === "added" ? "新增" : "掉出"}<small>{position == null ? "—" : `#${position}`}</small></span>
              <div className="career-event-body"><p>{title}</p><span>{[score.beatmap?.version, scoreMods(score).join(" ")].filter(Boolean).join(" · ") || "谱面信息暂无"}</span></div>
              <strong>{formatCareerStat("pp", score.pp)}<small> PP</small></strong>
              {score.beatmap?.id ? <Button aria-label={`打开谱面 ${title}`} onClick={() => {
                void desktopApi.openExternal(`https://osu.ppy.sh/beatmaps/${score.beatmap!.id}`).catch((openError) => notification.error("打开谱面失败", errorMessage(openError)));
              }} size="icon" variant="ghost"><ExternalLink className="size-4" /></Button> : null}
            </div>;
          }) : <p className="career-muted career-no-events">{events ? "没有 BP 新增或掉出" : "这一天有记录，没有 BP、奖章或本地媒体变化。"}</p>}
          {detail?.medal_events.length ? <div className="career-secondary-events"><h3><Award className="size-4" />获得奖章</h3>{detail.medal_events.map((item, index) => <p key={`${item.name}-${index}`}>{item.name}</p>)}</div> : null}
          {detail?.media_events.length ? <details className="career-extra-stats"><summary>本地媒体<span>{detail.media_events.length} 条</span></summary><div className="career-secondary-events">{detail.media_events.map((item, index) => <p key={`${item.name}-${index}`}>{item.payload?.kind === "replay" ? <Film className="size-4" /> : <FileImage className="size-4" />}<span>{item.name}</span><small>{item.client ?? "本地"}</small></p>)}</div></details> : null}
        </div>
        {detail?.stats ? <details className="career-extra-stats"><summary>其他统计<span>地区排名、总分、时长等</span></summary><dl className="career-extra-stat-grid">{careerStatLabels.slice(4).map((stat) => <Stat key={stat[0]} stat={stat} stats={detail.stats!} previous={detail.previous_stats} />)}</dl></details> : null}
      </>}
  </section>;
}
