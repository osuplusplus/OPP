import { Layers3, RefreshCw } from "lucide-react";
import { Button } from "../../shared/components/ui";
import { useCollectorCollections } from "./api";
import { CollectorCard } from "./CollectorCard";
import { hubError, type CollectorSort } from "./model";

/** osu!Collector 浏览区：统一搜索词 + 排序驱动搜索端点，网络不可达时给出明确提示。 */
export function CollectorBrowse({ query, onOpen }: { query: { search: string; sort: CollectorSort }; onOpen: (id: number) => void }) {
  const result = useCollectorCollections(query);
  const collections = result.data?.pages.flatMap((page) => page.collections) ?? [];
  const total = result.data?.pages[0]?.total ?? null;
  return <div aria-label="osu!Collector 搜索结果">
    <div className="hub-section-heading">
      <div>
        <h2>{query.search ? `“${query.search}”的搜索结果` : "osu!Collector 收藏集合"}</h2>
        <p>{total != null ? `共 ${total.toLocaleString()} 个集合` : "社区收藏集合直连浏览，导入后收藏夹会标注来源"}</p>
      </div>
      <Button aria-label="刷新 osu!Collector 列表" loading={result.isFetching && !result.isFetchingNextPage} onClick={() => void result.refetch()} size="icon"><RefreshCw size={16} /></Button>
    </div>
    {result.isPending
      ? <div aria-busy="true" aria-label="正在加载收藏集合" className="hub-grid">{Array.from({ length: 8 }, (_, index) => <div className="hub-skeleton" key={index} />)}</div>
      : result.error
        ? <div className="hub-empty" role="alert"><h3>osu!Collector 暂时无法连接</h3><p>{hubError(result.error)}</p><Button onClick={() => void result.refetch()}>重新加载</Button></div>
        : !collections.length
          ? <div className="hub-empty"><Layers3 size={32} /><h3>{query.search ? "没有匹配的收藏集合" : "还没有可浏览的收藏集合"}</h3><p>{query.search ? "换个关键词，或切换到社区曲包试试。" : "osu!Collector 列表为空，稍后再来看看。"}</p></div>
          : <>
            <div className="hub-grid" data-page-guide-hub-results="true">{collections.map((collection) => <CollectorCard collection={collection} key={collection.id} onOpen={onOpen} />)}</div>
            {result.hasNextPage ? <div className="hub-collector-more"><Button disabled={result.isFetchingNextPage} loading={result.isFetchingNextPage} onClick={() => void result.fetchNextPage()}>加载更多</Button></div> : null}
          </>}
  </div>;
}
