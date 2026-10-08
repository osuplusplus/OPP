import { Children, useEffect, useRef, useState, type ReactNode } from "react";
import { CalendarDays, Clock3, Trophy, Users } from "lucide-react";
import { Button } from "../../shared/components/ui";
import type { CommunityLobby, CommunityTournament } from "../../shared/types/osu";
import { activityLabels, activityTime, communityError, platformLabels, rulesetLabels, statusLabels } from "./model";
export function Poster({ url, title, detail = false }: { url: string | null; title: string; detail?: boolean }) {
  const [failed, setFailed] = useState(false);
  return <div className={`community-poster${detail ? " community-poster-detail" : ""}`}>
    {url && !failed ? <img alt={`${title}赛事海报`} src={url} loading="lazy" referrerPolicy="no-referrer" onError={() => setFailed(true)} /> : <div className="community-poster-placeholder"><Trophy size={36} aria-hidden="true" /><span>COMMUNITY TOURNAMENT</span></div>}
  </div>;
}
export function TournamentCard({ item, onOpen }: { item: CommunityTournament; onOpen: (id: string, button: HTMLButtonElement) => void }) {
  return <button type="button" className="community-card community-tournament-card" aria-label={`查看比赛 ${item.title}`} onClick={(event) => onOpen(item.id, event.currentTarget)}>
    <Poster key={item.poster_url} url={item.poster_url} title={item.title} />
    <div className="community-card-body"><div className="community-card-meta"><span className={`community-status status-${item.status}`}>{statusLabels[item.status]}</span><span>{rulesetLabels[item.ruleset]}</span></div>
      <h3>{item.title}</h3><p className="community-card-description">{item.summary}</p>
      <div className="community-card-time"><CalendarDays size={14} /><span>{item.registration_ends_at ? `报名至 ${activityTime(item.registration_ends_at)}` : "报名时间待公布"}</span></div>
      <div className="community-card-time"><Clock3 size={14} /><span>{item.starts_at ? `${activityTime(item.starts_at)} 开始` : "比赛时间待公布"}</span></div><p className="community-card-author">{item.organizer}</p>
    </div>
  </button>;
}
export function LobbyCard({ item, onOpen }: { item: CommunityLobby; onOpen: (id: string, button: HTMLButtonElement) => void }) {
  return <button type="button" className="community-card community-lobby-card" aria-label={`查看约玩 ${item.title}`} onClick={(event) => onOpen(item.id, event.currentTarget)}>
    <div className="community-card-body"><div className="community-card-meta"><span className="community-activity">{activityLabels[item.activity_type]}</span><span className={`community-status status-${item.status}`}>{statusLabels[item.status]}</span></div>
      <h3>{item.title}</h3><div className="community-tags"><span>{rulesetLabels[item.ruleset]}</span>{item.platform ? <span>{platformLabels[item.platform]}</span> : null}</div>
      <p className="community-card-description">{item.description}</p>
      <div className="community-time-range"><Clock3 size={15} /><div><span>{activityTime(item.starts_at)}</span><span>至 {activityTime(item.ends_at)}</span></div></div>
      <div className="community-card-person"><Users size={16} /><strong>{item.osu_username}</strong><span>#{item.osu_user_id}</span></div>
    </div>
  </button>;
}
export function CommunityFailure({ error, retry }: { error: unknown; retry: () => void }) {
  return <div className="community-empty" role="alert"><h3>暂时无法加载</h3><p>{communityError(error)}</p><Button onClick={retry}>重新加载</Button></div>;
}
function Masonry({ children }: { children: ReactNode }) {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!root.current || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const wrapper = entry.target.parentElement;
        if (wrapper) wrapper.style.gridRowEnd = `span ${Math.max(1, Math.ceil((entry.contentRect.height + 20) / 28))}`;
      }
    });
    const observeChildren = () => {
      observer.disconnect();
      root.current?.querySelectorAll(".community-card").forEach((card) => observer.observe(card));
    };
    observeChildren();
    const mutations = new MutationObserver(observeChildren);
    mutations.observe(root.current, { childList: true });
    return () => { observer.disconnect(); mutations.disconnect(); };
  }, []);
  return <div className="community-masonry" ref={root}>{Children.map(children, (child) => <div className="community-masonry-item">{child}</div>)}</div>;
}
export function Feed({ loading, error, empty, children, retry, more, loadingMore, loadMore }: { loading: boolean; error: unknown; empty: boolean; children: ReactNode; retry: () => void; more: boolean; loadingMore: boolean; loadMore: () => void }) {
  if (loading) return <div className="community-masonry" aria-label="正在加载活动" aria-busy="true">{Array.from({ length: 6 }, (_, index) => <div key={index} className="community-skeleton" />)}</div>;
  if (error) return <CommunityFailure error={error} retry={retry} />;
  if (empty) return <div className="community-empty"><Users size={34} /><h3>暂时没有符合条件的信息</h3><p>试试其他筛选条件，或稍后刷新。</p></div>;
  return <><Masonry>{children}</Masonry>{more ? <div className="community-load-more"><Button loading={loadingMore} onClick={loadMore}>加载更多</Button></div> : null}</>;
}
export function FilterSelect({ label, value, options, onChange, all = "全部" }: { label: string; value?: string; options: Record<string, string>; onChange: (value: string) => void; all?: string }) {
  return <label className="community-filter"><span>{label}</span><select className="opp-input" aria-label={label} value={value ?? ""} onChange={(event) => onChange(event.target.value)}><option value="">{all}</option>{Object.entries(options).map(([key, text]) => <option key={key} value={key}>{text}</option>)}</select></label>;
}
export function DetailField({ label, children }: { label: string; children: ReactNode }) {
  return <div className="community-detail-field"><span>{label}</span><div>{children}</div></div>;
}
