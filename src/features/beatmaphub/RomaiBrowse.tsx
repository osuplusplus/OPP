import { useEffect, useMemo, useRef, useState } from "react";
import { Layers3, RefreshCw, Trophy } from "lucide-react";
import { Button } from "../../shared/components/ui";
import type { RomaiPoolSummary, RomaiTier } from "../../shared/types/osu";
import { useRomaiPools } from "./api";
import { FilterRow, HubCollage, TextOption } from "./HubBrowser";
import {
  ROMAI_MOD_COLORS,
  ROMAI_TIERS,
  hubError,
  romaiFilterPools,
  romaiModChips,
  romaiSortOptions,
  romaiTierCounts,
  romaiTierMeta,
  type RomaiSort,
} from "./model";

/** 首屏与每批增量渲染的卡片数：1336 个图池全量渲染会拖垮首帧，首批 48 张、滚动触底扩容。 */
const RENDER_BATCH = 48;

/** RomAI 图池卡片：四联封面拼贴 + ELO 段位徽章 + 模组计数色签（对齐 romai.dev 图池页）。 */
export function RomaiPoolCard({ pool, onOpen }: { pool: RomaiPoolSummary; onOpen: (pool: RomaiPoolSummary) => void }) {
  const tier = romaiTierMeta(pool.tier);
  return <button aria-label={`查看图池 ${pool.name}`} className="hub-pack-card hub-romai-card" onClick={() => onOpen(pool)} type="button">
    <HubCollage ids={pool.covers} />
    <span className="hub-elo-badge" style={{ borderColor: `${tier.color}66`, color: tier.color }}>
      <i aria-hidden="true" className="hub-elo-dot" style={{ backgroundColor: tier.color }} />
      {pool.elo} ELO
    </span>
    <div className="hub-pack-copy">
      <h3 title={pool.name}>{pool.name}</h3>
      <p className="hub-card-subtitle"><span style={{ color: tier.color }}>{tier.label}</span> · {pool.map_count} 张谱面</p>
      <div className="hub-mod-chips">
        {romaiModChips(pool.counts).map((chip) => <span
          className="hub-mod-chip"
          key={chip.label}
          style={{ color: ROMAI_MOD_COLORS[chip.label], borderColor: `${ROMAI_MOD_COLORS[chip.label]}55`, background: `${ROMAI_MOD_COLORS[chip.label]}14` }}
        >{chip.count != null ? `${chip.count} ${chip.label}` : "TB"}</span>)}
      </div>
    </div>
  </button>;
}

/** RomAI 图池浏览区：段位筛选 + 排序 + 关键词（沿用顶部统一搜索框）全在客户端完成；
 * 网格增量渲染（首批 48 张、滚动触底自动扩容），避免一次性渲染上千张卡片。 */
export function RomaiBrowse({ search, onOpen }: { search: string; onOpen: (pool: RomaiPoolSummary) => void }) {
  const result = useRomaiPools(true);
  const [tier, setTier] = useState<RomaiTier | null>(null);
  const [sort, setSort] = useState<RomaiSort>("elo-desc");
  const [limit, setLimit] = useState(RENDER_BATCH);
  const sentinel = useRef<HTMLDivElement>(null);
  const pools = useMemo(() => result.data?.pools ?? [], [result.data]);
  const tierCounts = useMemo(() => romaiTierCounts(pools), [pools]);
  const visible = useMemo(() => romaiFilterPools(pools, { search, tier, sort }), [pools, search, tier, sort]);
  const shown = visible.slice(0, limit);
  // 筛选/排序/搜索变化时回到首批（渲染期重置，避免 effect 级联渲染）。
  const filterKey = `${search}|${tier}|${sort}`;
  const [prevFilterKey, setPrevFilterKey] = useState(filterKey);
  if (prevFilterKey !== filterKey) {
    setPrevFilterKey(filterKey);
    setLimit(RENDER_BATCH);
  }
  // 触底哨兵：进入视口前 600px 就预扩容一批，滚动过程无感加载。
  useEffect(() => {
    const node = sentinel.current;
    if (!node || shown.length >= visible.length) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        setLimit((current) => Math.min(visible.length, current + RENDER_BATCH));
      }
    }, { rootMargin: "600px" });
    observer.observe(node);
    return () => observer.disconnect();
  }, [shown.length, visible.length]);
  return <div aria-label="RomAI 比赛图池">
    <div className="hub-filter-rows">
      <FilterRow label="段位">
        <TextOption active={tier === null} onClick={() => setTier(null)}>全部</TextOption>
        {ROMAI_TIERS.map((item) => <TextOption active={tier === item.id} key={item.id} onClick={() => setTier(tier === item.id ? null : item.id)}>
          <i aria-hidden="true" className="hub-elo-dot" style={{ backgroundColor: item.color }} />{item.label}<small className="hub-option-count">{tierCounts.get(item.id) ?? 0}</small>
        </TextOption>)}
      </FilterRow>
      <FilterRow label="排序">
        {romaiSortOptions.map((option) => <TextOption active={sort === option.key} key={option.key} onClick={() => setSort(option.key)}>{option.label}</TextOption>)}
      </FilterRow>
    </div>
    <div className="hub-section-heading">
      <div>
        <h2>{search ? `“${search}”的图池结果` : "RomAI 比赛图池"}</h2>
        <p>{result.data ? `${visible.length.toLocaleString()} / ${pools.length.toLocaleString()} 个图池${result.data.from_cache ? " · 本地缓存" : ""}` : "来自 romai.dev 的公开赛图池库，按段位与模组结构浏览"}</p>
      </div>
      <Button aria-label="刷新 RomAI 图池" loading={result.isFetching} onClick={() => void result.refetch()} size="icon"><RefreshCw size={16} /></Button>
    </div>
    {result.isPending
      ? <div aria-busy="true" aria-label="正在加载图池" className="hub-grid">{Array.from({ length: 8 }, (_, index) => <div className="hub-skeleton" key={index} />)}</div>
      : result.error
        ? <div className="hub-empty" role="alert"><Trophy size={32} /><h3>RomAI 图池暂时无法加载</h3><p>{hubError(result.error)}</p><Button onClick={() => void result.refetch()}>重新加载</Button></div>
        : !visible.length
          ? <div className="hub-empty"><Layers3 size={32} /><h3>{search || tier ? "没有匹配的图池" : "还没有可浏览的图池"}</h3><p>{search || tier ? "换个关键词，或把段位筛选放宽到全部。" : "RomAI 图池索引为空，稍后再来看看。"}</p></div>
          : <>
            <div className="hub-grid" data-page-guide-hub-results="true">{shown.map((pool) => <RomaiPoolCard key={pool.id} onOpen={onOpen} pool={pool} />)}</div>
            {shown.length < visible.length ? <div className="hub-romai-more" ref={sentinel}>
              <span className="hub-muted">已显示 {shown.length.toLocaleString()} / {visible.length.toLocaleString()} · 滚动自动加载</span>
              <Button onClick={() => setLimit((current) => Math.min(visible.length, current + RENDER_BATCH))}>加载更多</Button>
            </div> : null}
          </>}
  </div>;
}
