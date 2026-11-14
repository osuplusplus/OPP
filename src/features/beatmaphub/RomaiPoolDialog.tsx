import { useMemo } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { FolderInput } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { RomaiPoolSummary, TournamentPoolEntry } from "../../shared/types/osu";
import { collectionsQueryKey, collectionEntriesKey } from "../collections/api";
import { useRomaiPoolDetail } from "./api";
import { HubCollage } from "./HubBrowser";
import { ROMAI_MOD_COLORS, beatmapCoverUrl, hubError, romaiModChips, romaiSlotLabel, romaiTierMeta } from "./model";

/** 图位分组顺序与组内标签；TB 恒为最后一组。 */
const SLOT_GROUPS = ["NM", "HD", "HR", "DT", "FM", "TB"] as const;

/** 单个图位行：左侧大号槽位标识（NM1/HD2…，与本地收藏夹管理的图位样式同源），
 * 右侧谱面资料与解析状态。 */
function PoolSlotRow({ entry }: { entry: TournamentPoolEntry }) {
  const label = romaiSlotLabel(entry.selection_type, entry.position);
  const color = ROMAI_MOD_COLORS[entry.selection_type] ?? "var(--theme-primary-light)";
  const map = entry.beatmap;
  const stars = map?.metadata?.difficulty_rating;
  return <li className="hub-slot-row">
    <span className="hub-slot-chip" style={{ color, borderColor: `${color}66`, background: `${color}17` }}>{label}</span>
    <div className="hub-slot-map">
      {map ? <>
        <img alt="" loading="lazy" src={beatmapCoverUrl(map.beatmapset_id)} />
        <div>
          <strong title={map.title}>{map.title}</strong>
          <p title={`${map.artist} · ${map.creator}`}>{map.artist} · {map.creator}</p>
          <small>[{map.difficulty_name}] · #{entry.beatmap_id}</small>
        </div>
      </> : <div>
        <strong>{entry.resolution_error ? "谱面资料解析失败" : `Beatmap #${entry.beatmap_id}`}</strong>
        <p>{entry.resolution_error ?? "正在解析谱面资料…"}</p>
      </div>}
    </div>
    <span className="hub-slot-status">{stars != null ? `${stars.toFixed(2)}★` : entry.resolution_error ? "解析失败" : map ? "—" : "…"}</span>
  </li>;
}

/** RomAI 图池详情：横幅 + 模组分组图位列表 + “同步为收藏夹”（带 NM1/HD2… 图位标识）。 */
export function RomaiPoolDialog({ pool, open, onClose }: { pool: RomaiPoolSummary | null; open: boolean; onClose: () => void }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const detail = useRomaiPoolDetail(open ? pool?.id ?? null : null);
  const tier = romaiTierMeta(pool?.tier ?? "candidate");
  const sync = useMutation({
    mutationFn: () => desktopApi.syncTournamentPoolCollection({ provider: "romai", pool_id: pool!.id }),
    onSuccess: async (result) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: collectionsQueryKey }),
        queryClient.invalidateQueries({ queryKey: collectionEntriesKey(result.folder_id) }),
      ]);
    },
  });
  const data = detail.data;
  const groups = useMemo(() => {
    const byGroup = new Map<string, TournamentPoolEntry[]>();
    for (const entry of data?.entries ?? []) {
      const list = byGroup.get(entry.selection_type) ?? [];
      list.push(entry);
      byGroup.set(entry.selection_type, list);
    }
    return SLOT_GROUPS.filter((group) => byGroup.has(group)).map((group) => ({ group, entries: byGroup.get(group)! }));
  }, [data]);
  const unresolved = (data?.entries ?? []).filter((entry) => !entry.beatmap).length;
  const footer = <div className="hub-import-footer">
    {sync.error ? <div className="hub-error" role="alert"><p>{hubError(sync.error)}</p></div> : null}
    {sync.isSuccess ? <div className="hub-feedback" role="status"><p>已同步为收藏夹（{sync.data.entry_count} 个图位），图位标识与缺失谱面可在收藏夹页管理。</p><Button size="sm" onClick={() => { onClose(); navigate(`/collections?${new URLSearchParams({ folder: sync.data.folder_id })}`); }}>前往收藏夹</Button></div> : null}
    <div className="hub-import-action">
      <p className="hub-muted">数据来自 romai.dev 公开图池库；同步会创建（或替换）同名收藏夹并保留图位标识。</p>
      <Button disabled={detail.isPending || sync.isPending} loading={sync.isPending} variant="primary" onClick={() => void sync.mutate()}><FolderInput size={16} />{sync.isSuccess ? "再次同步" : "同步为收藏夹"}</Button>
    </div>
  </div>;
  return <AppDialog open={open} onOpenChange={(value) => { if (!value && !sync.isPending) onClose(); }} overlayProps={{ className: "hub-overlay" }} title={pool?.name ?? "RomAI 比赛图池"} description={`RomAI · ${tier.label} · ${pool?.elo ?? "—"} ELO`} size="lg" contentClassName="hub-dialog hub-detail" bodyClassName="hub-detail-body" closeLabel="关闭图池详情" footer={footer}>
    {pool ? <HubCollage className="hub-detail-cover" ids={pool.covers} /> : null}
    <div className="hub-detail-content">
      <div className="hub-romai-headline">
        <span className="hub-elo-badge" style={{ borderColor: `${tier.color}66`, color: tier.color }}><i aria-hidden="true" className="hub-elo-dot" style={{ backgroundColor: tier.color }} />{pool?.elo} ELO</span>
        <span className="hub-card-subtitle"><span style={{ color: tier.color }}>{tier.label}</span> · {pool?.map_count ?? data?.entries.length ?? 0} 张谱面</span>
        <div className="hub-mod-chips">
          {pool ? romaiModChips(pool.counts).map((chip) => <span className="hub-mod-chip" key={chip.label} style={{ color: ROMAI_MOD_COLORS[chip.label], borderColor: `${ROMAI_MOD_COLORS[chip.label]}55`, background: `${ROMAI_MOD_COLORS[chip.label]}14` }}>{chip.count != null ? `${chip.count} ${chip.label}` : "TB"}</span>) : null}
        </div>
      </div>
      {detail.isPending ? <p role="status">正在读取图池并解析谱面资料…</p>
        : detail.error ? <div className="hub-error" role="alert"><p>{hubError(detail.error)}</p><Button size="sm" onClick={() => void detail.refetch()}>重试</Button></div>
          : data ? <>
            {unresolved ? <p className="hub-muted">{unresolved} 个图位的谱面资料暂时不可用，同步时将作为占位条目写入。</p> : null}
            <ol className="hub-slot-list">
              {groups.map(({ group, entries }) => <li key={group}>
                <div className="hub-slot-group" style={{ color: ROMAI_MOD_COLORS[group] }}>{group}<span>{entries.length}</span></div>
                <ul>{entries.map((entry) => <PoolSlotRow entry={entry} key={`${entry.selection_type}${entry.position}`} />)}</ul>
              </li>)}
            </ol>
          </> : null}
    </div>
  </AppDialog>;
}
