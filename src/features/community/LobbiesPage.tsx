import { useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, RefreshCw, Search, Users } from "lucide-react";
import { Button } from "../../shared/components/ui";
import { PageGuideButton } from "../../shared/components/PageHeader";
import { useCommunityIdentity, useCommunityProfile } from "../../shared/lib/communityIdentity";
import { desktopApi } from "../../shared/lib/tauri";
import { useMode } from "../../app/ModeContext";
import type { CommunityLobby } from "../../shared/types/osu";
import { communityKey, useLobbies, useServerClock } from "./api";
import { Feed, FilterSelect, LobbyCard } from "./components";
import { activityLabels, lobbyState, queryFromParams, platformLabels, rulesetLabels } from "./model";
import { CommunityIdentityDialog } from "./CommunityIdentityDialog";
import { LobbyPublishDialog } from "./LobbyPublishDialog";
import { LobbyDetailDialog } from "./LobbyDetailDialog";
import "./community.css";

/**
 * 大厅列表 + 服务器时钟:每秒 tick 只在本组件内消化(推算 status、
 * 过滤),页面其余部分不随之重渲染;卡片接收原始 item 与稳定 status
 * 字符串,由 React Compiler 记忆化,只有真正跨状态的大厅才会重渲染。
 */
function LobbyFeedClocked({ pages, serverTime, receivedAt, statusFilter, onOpen, feed }: {
  pages: { items: CommunityLobby[]; server_time?: string }[] | undefined;
  serverTime: string | undefined;
  receivedAt: number;
  statusFilter: string;
  onOpen: (id: string, button: HTMLButtonElement) => void;
  feed: Omit<React.ComponentProps<typeof Feed>, "empty" | "children">;
}) {
  const clock = useServerClock(serverTime, receivedAt);
  const items = useMemo(
    () => [...new Map((pages?.flatMap((page) => page.items) ?? []).map((item) => [item.id, item])).values()],
    [pages],
  );
  const activeOnly = statusFilter === "active";
  const visible = useMemo(
    () => items
      .map((item) => ({ item, status: lobbyState(item, clock) }))
      .filter(({ status }) => activeOnly
        ? status !== "ended" && status !== "closed"
        : statusFilter === "all" || status === statusFilter),
    [items, clock, activeOnly, statusFilter],
  );
  return <Feed {...feed} empty={!visible.length}>{visible.map(({ item, status }) => <LobbyCard item={item} key={item.id} onOpen={onOpen} status={status} />)}</Feed>;
}

export function LobbiesPage() {
  const [params, setParams] = useSearchParams(); const query = queryFromParams(params); const mine = params.get("view") === "mine";
  const client = useQueryClient(); const { ruleset } = useMode(); const auth = useCommunityIdentity(); const connected = Boolean(auth.data?.connected);
  const profile = useCommunityProfile(connected);
  const osu = useQuery({ queryKey: ["auth-status"], queryFn: desktopApi.getAuthStatus, staleTime: 15_000 });
  const [text, setText] = useState(query.q ?? ""); const [selection, setSelection] = useState<string | null>(null);
  const [publisher, setPublisher] = useState<{ edit: CommunityLobby | null } | null>(null);
  const [identityOpen, setIdentityOpen] = useState(false); const [pendingMine, setPendingMine] = useState(false);
  const trigger = useRef<HTMLButtonElement | null>(null); const publishTrigger = useRef<HTMLDivElement | null>(null);
  const identity = `${auth.data?.device_id ?? "guest"}:${connected}`;
  const list = useLobbies({ ...query, status: query.status ?? (mine ? "all" : "active") }, mine, mine ? identity : "public", !mine || connected);
  const pages = list.data?.pages;
  const change = (key: string, value: string) => setParams((previous) => { const next = new URLSearchParams(previous); if (value) next.set(key, value); else next.delete(key); return next; }, { replace: true });
  const changeView = (value: boolean) => setParams((previous) => { const next = new URLSearchParams(previous); next.delete("status"); if (value) next.set("view", "mine"); else next.delete("view"); return next; }, { replace: true });
  const refresh = () => client.invalidateQueries({ queryKey: communityKey });
  const openMine = () => { if (!connected) { setPendingMine(true); setIdentityOpen(true); } else changeView(true); };
  const restoreFocus = (event: Event) => { event.preventDefault(); const target = trigger.current?.isConnected ? trigger.current : publishTrigger.current?.querySelector("button"); target?.focus({ preventScroll: true }); };
  return <div className="community-page">
    <header className="community-page-header"><div data-page-guide-title="true"><span className="community-eyebrow">PLAY TOGETHER</span><div className="flex items-center gap-2"><h1>约玩大厅</h1><PageGuideButton title="约玩大厅" /></div><p>约一场 MP，找个排位搭子，或一起练练图。</p></div><div className="community-header-actions"><Button onClick={openMine}><Users size={16} />我的发布</Button><div ref={publishTrigger}><Button variant="primary" onClick={() => setPublisher({ edit: null })}><Plus size={17} />发布约玩</Button></div></div></header>
    <div className="community-view-tabs" aria-label="约玩范围"><Button variant={!mine ? "primary" : "ghost"} onClick={() => changeView(false)}>公开大厅</Button><Button variant={mine ? "primary" : "ghost"} onClick={openMine}>我的发布</Button><span>通过 osu! 联系发布者，自由约好一起玩。</span></div>
    <div className="community-toolbar"><form className="community-search" onSubmit={(event) => { event.preventDefault(); change("q", text.trim()); }}><Search size={18} /><input aria-label="搜索约玩" placeholder="搜索活动、玩家或 osu! ID" value={text} maxLength={120} onChange={(event) => setText(event.target.value)} /><Button type="submit" size="sm">搜索</Button></form>
      <FilterSelect label="游戏模式" value={query.ruleset} options={rulesetLabels} onChange={(value) => change("ruleset", value)} />
      <FilterSelect label="活动类型" value={query.activity_type} options={activityLabels} onChange={(value) => change("activity_type", value)} />
      <FilterSelect label="平台" value={query.platform} options={platformLabels} onChange={(value) => change("platform", value)} />
      {mine ? <FilterSelect label="活动状态" value={query.status === "all" ? "" : query.status} options={{ active: "未结束", upcoming: "即将开始", ongoing: "进行中", ended: "已结束", closed: "提前结束" }} onChange={(value) => change("status", value || "all")} /> : null}
      <Button size="icon" aria-label="刷新约玩" disabled={mine && !connected} loading={list.isFetching && !list.isFetchingNextPage} onClick={() => void list.refetch()}><RefreshCw size={17} /></Button>
    </div>
    {mine && !connected ? <div className="community-empty"><h3>连接身份后查看你的发布</h3><p>公开大厅始终可以直接浏览。</p><Button onClick={openMine}>连接社区身份</Button></div> : <LobbyFeedClocked feed={{ loading: list.isPending, error: list.error, retry: () => void list.refetch(), more: list.hasNextPage, loadingMore: list.isFetchingNextPage, loadMore: () => void list.fetchNextPage() }} onOpen={(id, button) => { trigger.current = button; setSelection(id); }} pages={pages} receivedAt={list.dataUpdatedAt} serverTime={pages?.[pages.length - 1]?.server_time} statusFilter={query.status ?? (mine ? "all" : "active")} />}
    {selection ? <LobbyDetailDialog key={selection} id={selection} ownerId={profile.data?.user.id} onClose={() => setSelection(null)} restoreFocus={restoreFocus} onEdit={(edit) => setPublisher({ edit })} onChanged={refresh} /> : null}
    {publisher ? <LobbyPublishDialog key={publisher.edit?.id ?? "new"} edit={publisher.edit} connected={connected} identity={osu.data ?? {}} mode={ruleset} onConnect={() => setIdentityOpen(true)} onClose={() => setPublisher(null)} onSaved={(item) => { setPublisher(null); void refresh(); setSelection(item.id); }} /> : null}
    {identityOpen ? <CommunityIdentityDialog onClose={() => { setIdentityOpen(false); setPendingMine(false); }} onConnected={() => { setIdentityOpen(false); if (pendingMine) { changeView(true); setPendingMine(false); } }} /> : null}
  </div>;
}
