import { useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Copy, ExternalLink, RefreshCw, Search, Trophy } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import { COMMUNITY_GROUP_NUMBER } from "../../shared/constants/community";
import { communityApi, useTournament, useTournaments } from "./api";
import { CommunityFailure, DetailField, Feed, FilterSelect, Poster, TournamentCard } from "./components";
import { activityTime, copyCommunityText, queryFromParams, rulesetLabels, statusLabels } from "./model";
import "./community.css";

export function TournamentsPage() {
  const [params, setParams] = useSearchParams(); const query = queryFromParams(params);
  const [text, setText] = useState(query.q ?? ""); const [notice, setNotice] = useState("");
  const [linkError, setLinkError] = useState("");
  const [selection, setSelection] = useState<string | null>(null); const trigger = useRef<HTMLButtonElement | null>(null);
  const list = useTournaments(query); const detail = useTournament(selection);
  const items = [...new Map((list.data?.pages.flatMap((page) => page.items) ?? []).map((item) => [item.id, item])).values()];
  const change = (key: string, value: string) => setParams((previous) => { const next = new URLSearchParams(previous); if (value) next.set(key, value); else next.delete(key); return next; }, { replace: true });
  return <div className="community-page">
    <header className="community-page-header"><div><span className="community-eyebrow">OPP COMMUNITY</span><h1>比赛公告</h1><p>发现下一场比赛，找到属于你的舞台。</p></div><Trophy size={38} aria-hidden="true" /></header>
    <div className="community-contact"><div><strong>想让更多玩家看到你的比赛？</strong><p>发布比赛公告请加入 QQ 群 <span className="community-group-number">{COMMUNITY_GROUP_NUMBER}</span> 联系管理员</p></div><Button size="sm" onClick={() => void copyCommunityText(COMMUNITY_GROUP_NUMBER).then(setNotice)}><Copy size={14} />复制群号</Button></div>
    {notice ? <p role="status" className="community-notice">{notice}</p> : null}
    <div className="community-toolbar"><form className="community-search" onSubmit={(event) => { event.preventDefault(); change("q", text.trim()); }}><Search size={18} /><input aria-label="搜索比赛" placeholder="搜索比赛、主办方或介绍" maxLength={120} value={text} onChange={(event) => setText(event.target.value)} /><Button type="submit" size="sm">搜索</Button></form>
      <FilterSelect label="游戏模式" value={query.ruleset} options={rulesetLabels} onChange={(value) => change("ruleset", value)} />
      <FilterSelect label="赛事状态" value={query.status === "all" ? "" : query.status ?? "active"} all="全部状态" options={{ active: "未结束", registering: "报名中", upcoming: "即将开始", ongoing: "进行中", ended: "已结束" }} onChange={(value) => change("status", value || "all")} />
      <Button size="icon" aria-label="刷新比赛" loading={list.isFetching && !list.isFetchingNextPage} onClick={() => void list.refetch()}><RefreshCw size={17} /></Button>
    </div>
    <Feed loading={list.isPending} error={list.error} empty={!items.length} retry={() => void list.refetch()} more={list.hasNextPage} loadingMore={list.isFetchingNextPage} loadMore={() => void list.fetchNextPage()}>{items.map((item) => <TournamentCard key={item.id} item={item} onOpen={(id, button) => { trigger.current = button; setLinkError(""); setSelection(id); }} />)}</Feed>
    <AppDialog open={Boolean(selection)} onOpenChange={(open) => { if (!open) setSelection(null); }} title={detail.data?.title ?? "比赛详情"} description="比赛信息与报名方式" size="lg" onCloseAutoFocus={(event) => { event.preventDefault(); trigger.current?.focus({ preventScroll: true }); }} footer={detail.data?.registration_url ? <Button onClick={() => void communityApi.openExternal(detail.data!.registration_url!).catch(() => setLinkError("无法打开报名链接，请重试。"))}><ExternalLink size={16} />打开报名链接</Button> : undefined}>
      {linkError ? <p role="alert" className="community-error">{linkError}</p> : null}
      {detail.isPending ? <p role="status">正在加载比赛详情…</p> : detail.error ? <CommunityFailure error={detail.error} retry={() => void detail.refetch()} /> : detail.data ? <div className="community-detail"><Poster key={detail.data.poster_url} url={detail.data.poster_url} title={detail.data.title} detail />
        <div className="community-tags"><span>{rulesetLabels[detail.data.ruleset]}</span><span className={`community-status status-${detail.data.status}`}>{statusLabels[detail.data.status]}</span></div>
        <DetailField label="主办方">{detail.data.organizer}</DetailField><DetailField label="报名时间">{activityTime(detail.data.registration_starts_at)} — {activityTime(detail.data.registration_ends_at)}</DetailField>
        <DetailField label="比赛时间">{activityTime(detail.data.starts_at)} — {activityTime(detail.data.ends_at)}</DetailField>
        {detail.data.requirements ? <DetailField label="参赛要求"><p className="community-prose">{detail.data.requirements}</p></DetailField> : null}
        <DetailField label="比赛介绍"><p className="community-prose">{detail.data.description}</p></DetailField>
      </div> : null}
    </AppDialog>
  </div>;
}
