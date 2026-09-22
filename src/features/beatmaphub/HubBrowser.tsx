import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ArrowUpRight, Layers3, RefreshCw, Search, Star, X } from "lucide-react";
import { Button } from "../../shared/components/ui";
import type { BeatmapHubPack } from "../../shared/types/osu";
import { beatmapHubRecommendationsKey, hubApi, useBeatmapHubRecommendations, useHubSearch } from "./api";
import { hubError, packCover, parseHubInput } from "./model";

export function HubCover({ ids, className = "" }: { ids: number[]; className?: string }) {
  const url = packCover(ids);
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  return <div className={`hub-cover ${className}`}>
    <Layers3 aria-hidden="true" className="hub-cover-placeholder" />
    {url && failedUrl !== url ? <img alt="" loading="lazy" src={url} onError={() => setFailedUrl(url)} /> : null}
  </div>;
}
function PackCard({ pack, onOpen }: { pack: BeatmapHubPack; onOpen: (id: string) => void }) {
  return <button className="hub-pack-card" onClick={() => onOpen(pack.id)} type="button" aria-label={`查看曲包 ${pack.title}`}>
    <HubCover ids={pack.beatmapset_ids} />
    <div className="hub-pack-copy">
      <div className="hub-card-eyebrow"><span>{pack.beatmapset_ids.length} 个谱面集</span><ArrowUpRight size={16} /></div>
      <h3 title={pack.title}>{pack.title}</h3>
      <p className="hub-muted">{pack.owner.display_name}</p>
      <p className="hub-card-description">{pack.description || "打开曲包，发现下一首想玩的谱面。"}</p>
      <div className="hub-card-stats"><span>难度 {pack.stars_min != null && pack.stars_max != null ? `${pack.stars_min.toFixed(1)}–${pack.stars_max.toFixed(1)}★` : "未提供"}</span><span><Star size={13} />评分 {pack.rating.average?.toFixed(1) ?? "暂无"}<small>({pack.rating.count})</small></span></div>
    </div>
  </button>;
}
export function HubBrowser({ onOpen, fallbackQuery, onClearFallback }: { onOpen: (id: string, fallback?: string) => void; fallbackQuery: string | null; onClearFallback: () => void }) {
  const client = useQueryClient();
  const [text, setText] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [validation, setValidation] = useState<string | null>(null);
  const activeQuery = fallbackQuery ?? submitted;
  const inputText = fallbackQuery ?? text;
  const recommendations = useBeatmapHubRecommendations();
  const search = useHubSearch(activeQuery);
  const refresh = useMutation({ mutationFn: hubApi.refreshRecommendations, onSuccess: (packs) => client.setQueryData(beatmapHubRecommendationsKey, packs) });
  const result = activeQuery ? search : recommendations;
  const packs = (result.data ?? []).filter((pack) => !pack.is_private);
  const submit = () => {
    setValidation(null); onClearFallback();
    const input = parseHubInput(inputText);
    if (input.kind === "invalid") setValidation("请输入有效的 6 位分享码，例如 BPH-7K3N9A。");
    else if (input.kind === "code") onOpen(input.id, input.bare ? inputText.trim() : undefined);
    else { setSubmitted(input.query); if (submitted === input.query && input.query) void search.refetch(); }
  };
  return <section aria-label="发现曲包">
    <div className="hub-intro"><span className="hub-eyebrow">COMMUNITY COLLECTIONS</span><h2>下一段旅程，从这里开始。</h2><p>发现玩家分享的曲包，把喜欢的谱面带进你的收藏夹。</p></div>
    <form className="hub-search" data-page-guide-hub-search="true" onSubmit={(event) => { event.preventDefault(); submit(); }}>
      <Search size={20} aria-hidden="true" />
      <input aria-label="搜索曲包或输入分享码" placeholder="搜索标题、作者，或粘贴 BPH 分享码" value={inputText} onChange={(event) => { setText(event.target.value); setValidation(null); if (fallbackQuery) { setSubmitted(fallbackQuery); onClearFallback(); } if (!event.target.value.trim()) { setSubmitted(""); onClearFallback(); } }} />
      {inputText ? <Button aria-label="清空搜索" size="icon" variant="ghost" type="button" onClick={() => { setText(""); setSubmitted(""); setValidation(null); onClearFallback(); }}><X size={16} /></Button> : null}
      <Button type="submit" variant="primary">搜索 / 打开</Button>
    </form>
    {validation ? <p className="hub-error" role="alert">{validation}</p> : null}
    <div className="hub-section-heading"><div><h2>{activeQuery ? `“${activeQuery}”的搜索结果` : "社区推荐"}</h2><p>{activeQuery ? `${packs.length} 个结果` : "来自社区的练习灵感与精选清单"}</p></div><Button aria-label={activeQuery ? "刷新搜索" : "刷新推荐"} size="icon" loading={result.isFetching || refresh.isPending} onClick={() => { if (activeQuery) void search.refetch(); else refresh.mutate(); }}><RefreshCw size={16} /></Button></div>
    {refresh.error ? <p className="hub-error" role="alert">刷新失败：{hubError(refresh.error)}</p> : null}
    {result.isPending ? <div className="hub-grid" aria-label="正在加载曲包" aria-busy="true">{Array.from({ length: 6 }, (_, index) => <div className="hub-skeleton" key={index} />)}</div> : result.error ? <div className="hub-empty" role="alert"><h3>曲包暂时无法加载</h3><p>{hubError(result.error)}</p><Button onClick={() => void result.refetch()}>重新加载</Button></div> : !packs.length ? <div className="hub-empty"><Layers3 size={32} /><h3>{activeQuery ? "没有找到匹配的曲包" : "社区还没有公开曲包"}</h3><p>{activeQuery ? "试试其他关键词，也可以直接输入分享码。" : "你可以输入分享码，或发布自己的第一个曲包。"}</p></div> : <div className="hub-grid" data-page-guide-hub-results="true">{packs.map((pack) => <PackCard key={pack.id} pack={pack} onOpen={onOpen} />)}</div>}
  </section>;
}
