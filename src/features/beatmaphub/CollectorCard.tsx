import { useState } from "react";
import { Heart, Layers3 } from "lucide-react";
import { RulesetIcon } from "../../shared/components/RulesetIcon";
import type { OsuCollectorCollection, Ruleset } from "../../shared/types/osu";
import { collectorAvatar, collectorDate, starBinColor } from "./model";

/** 难度分布直方图：1-10★ 分桶，桶高为该星数难度占集合峰值的比例（对齐 osu!Collector 官网卡片）。 */
function DifficultySpread({ spread, total }: { spread: number[]; total: number }) {
  const max = Math.max(...spread, 1);
  return <div className="hub-spread" role="img" aria-label={`难度分布直方图，共 ${total.toLocaleString()} 张难度`}>
    <div className="hub-spread-bars">
      {spread.map((count, index) => <span
        className="hub-spread-bar"
        key={index}
        style={{ backgroundColor: starBinColor(index + 1), height: `${count > 0 ? Math.max((count / max) * 100, 6) : 2}%` }}
      />)}
    </div>
    <div aria-hidden="true" className="hub-spread-axis">{spread.map((_, index) => <span key={index}>{index + 1}</span>)}</div>
  </div>;
}

/** osu!Collector 集合卡片：无封面图，以难度分布 + 模式/收藏数据代替视觉主体。 */
export function CollectorCard({ collection, onOpen }: { collection: OsuCollectorCollection; onOpen: (id: number) => void }) {
  const date = collectorDate(collection.last_modified ?? collection.date_uploaded);
  const avatar = collectorAvatar(collection.uploader_id);
  const [avatarFailed, setAvatarFailed] = useState(false);
  return <button aria-label={`查看收藏 ${collection.name}`} className="hub-pack-card hub-collector-card" onClick={() => onOpen(collection.id)} type="button">
    {collection.difficulty_spread
      ? <DifficultySpread spread={collection.difficulty_spread} total={collection.beatmap_count} />
      : <div className="hub-spread hub-spread-empty"><Layers3 aria-hidden="true" size={20} /><span>暂无难度分布</span></div>}
    <div className="hub-pack-copy">
      <div className="hub-card-eyebrow">
        <span className="hub-modes">
          {collection.modes.length
            ? collection.modes.map((mode) => <span className="hub-mode-count" key={mode.mode} title={`${mode.mode} 难度 ${mode.count.toLocaleString()} 张`}><RulesetIcon className="size-3.5" mode={mode.mode as Ruleset} />{mode.count.toLocaleString()}</span>)
            : <span>{collection.beatmap_count.toLocaleString()} 张难度</span>}
        </span>
        <span className="hub-favourites" title={`收藏 ${collection.favourites.toLocaleString()} 次`}><Heart aria-hidden="true" size={13} />{collection.favourites.toLocaleString()}</span>
      </div>
      <h3 title={collection.name}>{collection.name}</h3>
      <p className="hub-card-description">{collection.description || "打开收藏集合，查看难度样本与本地已有统计。"}</p>
      <div className="hub-card-footer">
        <span className="hub-uploader">
          {avatar && !avatarFailed ? <img alt="" loading="lazy" onError={() => setAvatarFailed(true)} src={avatar} /> : null}
          {collection.uploader}
        </span>
        {date ? <span className="hub-muted">更新于 {date}</span> : null}
      </div>
    </div>
  </button>;
}
