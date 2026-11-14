import { useState, type ReactNode } from "react";
import { ArrowUpRight, Heart, Layers3, RefreshCw, Search, Star, Trophy, Users, X } from "lucide-react";
import { Button } from "../../shared/components/ui";
import { RulesetIcon } from "../../shared/components/RulesetIcon";
import type { BeatmapHubPack, RomaiPoolSummary } from "../../shared/types/osu";
import { useBeatmapHubRecommendations, useHubSearch } from "./api";
import { CollectorBrowse } from "./CollectorBrowse";
import { RomaiBrowse } from "./RomaiBrowse";
import { collectorSortOptions, hubError, parseHubInput, type CollectorSort } from "./model";

/** 四联封面拼贴（RomAI 图池卡片同款视觉主体）：每段等宽铺满，底部渐变过渡到卡片底色。 */
export function HubCollage({ ids, className = "" }: { ids: number[]; className?: string }) {
  const covers = ids.filter((id) => Number.isSafeInteger(id) && id > 0).slice(0, 4);
  return <div className={`hub-collage ${className}`}>
    {covers.length ? covers.map((id) => <HubCollageSegment key={id} setId={id} />) : <Layers3 aria-hidden="true" className="hub-cover-placeholder" />}
    <span aria-hidden="true" className="hub-collage-scrim" />
  </div>;
}

function HubCollageSegment({ setId }: { setId: number }) {
  const [failed, setFailed] = useState(false);
  if (failed) return <span aria-hidden="true" className="hub-collage-segment hub-collage-fallback" />;
  return <img
    alt=""
    aria-hidden="true"
    className="hub-collage-segment"
    loading="lazy"
    src={`https://assets.ppy.sh/beatmaps/${setId}/covers/cover.jpg`}
    onError={() => setFailed(true)}
  />;
}

function PackCard({ pack, onOpen }: { pack: BeatmapHubPack; onOpen: (id: string) => void }) {
  return <button className="hub-pack-card" onClick={() => onOpen(pack.id)} type="button" aria-label={`查看曲包 ${pack.title}`}>
    <HubCollage ids={pack.beatmapset_ids} />
    <div className="hub-pack-copy">
      <div className="hub-card-eyebrow"><span>{pack.beatmapset_ids.length} 个谱面集</span><ArrowUpRight size={16} /></div>
      <h3 title={pack.title}>{pack.title}</h3>
      <p className="hub-card-subtitle">{pack.owner.display_name}</p>
      <p className="hub-card-description">{pack.description || "打开曲包，发现下一首想玩的谱面。"}</p>
      <div className="hub-card-stats"><span>难度 {pack.stars_min != null && pack.stars_max != null ? `${pack.stars_min.toFixed(1)}–${pack.stars_max.toFixed(1)}★` : "未提供"}</span><span><Star size={13} />评分 {pack.rating.average?.toFixed(1) ?? "暂无"}<small>({pack.rating.count})</small></span></div>
    </div>
  </button>;
}

/** 与在线谱面页 OnlineBeatmapFilters 的 FilterRow/TextOption 同款控件，保持两个页面的筛选视觉一致。 */
export function FilterRow({ label, children }: { label: string; children: ReactNode }) {
  return <div className="grid min-h-10 items-center gap-1 py-1 sm:grid-cols-[clamp(4.75rem,calc(3.25rem+2vw),6.25rem)_minmax(0,1fr)] sm:gap-2.5">
    <div className="whitespace-nowrap text-[clamp(11px,calc(8px+0.3vw),14px)] font-semibold leading-5 text-slate-500">{label}</div>
    <div aria-label={`${label}筛选`} className="flex min-w-0 flex-wrap items-center gap-1.5" role="group">{children}</div>
  </div>;
}
export function TextOption({ active, children, onClick }: { active: boolean; children: ReactNode; onClick: () => void }) {
  return <button
    aria-pressed={active}
    className={`shrink-0 whitespace-nowrap rounded-md px-1.5 py-0.5 text-left transition-colors ${active ? "bg-[var(--theme-primary-muted)] font-semibold text-[var(--theme-primary-light)]" : "font-normal text-slate-500 hover:bg-[var(--surface-interactive)] hover:text-slate-200"}`}
    onClick={onClick}
    type="button"
  >
    <span className="inline-flex items-center gap-1 text-[clamp(11px,calc(8px+0.3vw),14px)] leading-[1.25]">{children}</span>
  </button>;
}

type HubSource = "community" | "collector" | "romai";

/** 来源切换分段控件：三个来源常驻可见（比下拉更显眼），当前来源高亮为主色。 */
function SourceSwitch({ value, onChange }: { value: HubSource; onChange: (source: HubSource) => void }) {
  const options: { key: HubSource; label: string; icon: ReactNode; hint: string }[] = [
    { key: "community", label: "社区曲包", icon: <Users size={16} />, hint: "OPP 社区分享" },
    { key: "collector", label: "osu!Collector", icon: <Heart size={16} />, hint: "社区收藏集合" },
    { key: "romai", label: "比赛图池", icon: <Trophy size={16} />, hint: "RomAI 图池库" },
  ];
  return <div aria-label="曲包来源" className="hub-source-switch" role="tablist">
    {options.map((option) => <button
      aria-label={option.label}
      aria-selected={value === option.key}
      className="hub-source-segment"
      key={option.key}
      onClick={() => onChange(option.key)}
      role="tab"
      type="button"
    >
      {option.icon}
      <span className="hub-source-segment-copy"><strong>{option.label}</strong><small>{option.hint}</small></span>
    </button>)}
  </div>;
}

export function HubBrowser({ onOpen, onOpenCollector, onOpenRomai, fallbackQuery, onClearFallback }: { onOpen: (id: string, fallback?: string) => void; onOpenCollector: (id: number) => void; onOpenRomai: (pool: RomaiPoolSummary) => void; fallbackQuery: string | null; onClearFallback: () => void }) {
  // 默认展示 osu!Collector 收藏集合；社区曲包与分享码回退期间强制切换社区来源。
  const [sourceChoice, setSourceChoice] = useState<HubSource>("collector");
  const [text, setText] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [collectorSort, setCollectorSort] = useState<CollectorSort>("relevance");
  const [validation, setValidation] = useState<string | null>(null);
  // 分享码打开失败的“改为关键词搜索”回退只作用于社区结果：回退期间强制展示社区页签。
  const source: HubSource = fallbackQuery ? "community" : sourceChoice;
  const activeQuery = fallbackQuery ?? submitted;
  const inputText = fallbackQuery ?? text;
  const recommendations = useBeatmapHubRecommendations();
  const search = useHubSearch(activeQuery);
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
    <form className="hub-search" data-page-guide-hub-search="true" onSubmit={(event) => { event.preventDefault(); submit(); }}>
      <Search size={20} aria-hidden="true" />
      <input aria-label="搜索曲包或输入分享码" placeholder="搜索标题、作者，或粘贴 BPH 分享码" value={inputText} onChange={(event) => { setText(event.target.value); setValidation(null); if (fallbackQuery) { setSubmitted(fallbackQuery); onClearFallback(); } if (!event.target.value.trim()) { setSubmitted(""); onClearFallback(); } }} />
      {inputText ? <Button aria-label="清空搜索" size="icon" variant="ghost" type="button" onClick={() => { setText(""); setSubmitted(""); setValidation(null); onClearFallback(); }}><X size={16} /></Button> : null}
      <Button type="submit" variant="primary">搜索 / 打开</Button>
    </form>
    {validation ? <p className="hub-error" role="alert">{validation}</p> : null}
    <SourceSwitch onChange={setSourceChoice} value={source} />
    {source === "collector" ? <div className="hub-filter-rows">
      <FilterRow label="排序">
        {collectorSortOptions.map((option) => <TextOption active={collectorSort === option.key} key={option.key} onClick={() => setCollectorSort(option.key)}>
          {"mode" in option ? <RulesetIcon className="size-3.5" mode={option.mode} /> : null}{option.label}
        </TextOption>)}
      </FilterRow>
    </div> : null}
    {source === "romai"
      ? <RomaiBrowse onOpen={onOpenRomai} search={activeQuery} />
      : source === "collector"
        ? <CollectorBrowse onOpen={onOpenCollector} query={{ search: activeQuery, sort: collectorSort }} />
        : <>
          <div className="hub-section-heading"><div><h2>{activeQuery ? `“${activeQuery}”的搜索结果` : "社区推荐"}</h2><p>{activeQuery ? `${packs.length} 个结果` : "来自社区的练习灵感与精选清单"}</p></div><Button aria-label={activeQuery ? "刷新搜索" : "刷新推荐"} size="icon" loading={result.isFetching} onClick={() => void result.refetch()}><RefreshCw size={16} /></Button></div>
          {result.isPending ? <div className="hub-grid" aria-label="正在加载曲包" aria-busy="true">{Array.from({ length: 6 }, (_, index) => <div className="hub-skeleton" key={index} />)}</div> : result.error ? <div className="hub-empty" role="alert"><h3>曲包暂时无法加载</h3><p>{hubError(result.error)}</p><Button onClick={() => void result.refetch()}>重新加载</Button></div> : !packs.length ? <div className="hub-empty"><Layers3 size={32} /><h3>{activeQuery ? "没有找到匹配的曲包" : "社区还没有公开曲包"}</h3><p>{activeQuery ? "试试其他关键词，也可以直接输入分享码。" : "你可以输入分享码，或发布自己的第一个曲包。"}</p></div> : <div className="hub-grid" data-page-guide-hub-results="true">{packs.map((pack) => <PackCard key={pack.id} onOpen={onOpen} pack={pack} />)}</div>}
        </>}
  </section>;
}
