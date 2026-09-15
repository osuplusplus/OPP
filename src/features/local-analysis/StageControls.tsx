import { useLayoutEffect, useRef, useState } from "react";
import { ExternalLink, FolderSearch, Heart, ImageIcon, ListMusic, Music4, PackageOpen, ScanSearch, WandSparkles } from "lucide-react";
import type { LocalBeatmapSetSummary, LocalBeatmapSummary, OsuClient } from "../../shared/types/osu";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage, fixedNumber } from "../../shared/lib/format";
import { openCollectionDialog } from "../collections/events";
import { similarityRouteForLocalResource } from "../similar-beatmaps/navigation";
import { beatmapPreviewRoute } from "../tools/navigation";
import { LocalAudioPreview } from "./LocalAudioPreview";
import { durationLabel, sortedDifficulties } from "./stageModel";

export function StageControls({ client, set, difficulty, focusSelection, volume, onDifficulty, onOpen }: {
  client: OsuClient; set: LocalBeatmapSetSummary; difficulty: LocalBeatmapSummary; focusSelection: number; volume: number;
  onDifficulty: (id: string) => void; onOpen: (id: string) => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const difficulties = sortedDifficulties(set);
  const id = difficulty.resource.resource_id;
  useLayoutEffect(() => {
    if (focusSelection) root.current?.querySelector<HTMLButtonElement>('[aria-selected="true"]')?.focus({ preventScroll: true });
  }, [focusSelection, set.set_key, id]);
  const run = async (action: () => Promise<unknown>) => {
    setNotice(null);
    try { await action(); } catch (e) { setNotice(errorMessage(e)); }
  };
  const exportSet = async () => {
    setExporting(true);
    await run(async () => {
      const directory = await desktopApi.chooseDirectory("选择 .osz 导出位置");
      if (directory) setNotice(`已导出：${await desktopApi.exportLocalBeatmapSet(client, set.set_key, directory)}`);
    });
    setExporting(false);
  };
  const previewRoute = beatmapPreviewRoute(difficulty.beatmap_id);
  return <div className="local-stage-console" ref={root}>
    <div className="local-stage-difficulties" role="tablist" aria-label="谱面难度">
      {difficulties.map((item, index) => <button key={item.resource.resource_id} type="button" role="tab" aria-selected={item.resource.resource_id === id} tabIndex={item.resource.resource_id === id ? 0 : -1}
        onClick={() => onDifficulty(item.resource.resource_id)} onKeyDown={(event) => {
          let next: number;
          if (event.key === "ArrowRight") next = (index + 1) % difficulties.length;
          else if (event.key === "ArrowLeft") next = (index - 1 + difficulties.length) % difficulties.length;
          else if (event.key === "Home") next = 0;
          else if (event.key === "End") next = difficulties.length - 1;
          else return;
          event.preventDefault(); onDifficulty(difficulties[next].resource.resource_id);
          const button = event.currentTarget.parentElement?.children[next] as HTMLElement | undefined;
          button?.focus({ preventScroll: true }); button?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
        }}>
        <strong title={item.difficulty_name}>{item.difficulty_name}</strong><small>{fixedNumber(item.stars, 2)} ★</small>
      </button>)}
    </div>
    <div className="local-stage-metrics" key={id} aria-label="所选难度参数">
      {[["星级", `${fixedNumber(difficulty.stars, 2)} ★`], ["满分 PP", fixedNumber(difficulty.max_pp, 1)], ["BPM", fixedNumber(difficulty.bpm, 1)], ["时长", durationLabel(difficulty.length_ms / 1000)], ["AR", fixedNumber(difficulty.ar, 1)], ["OD", fixedNumber(difficulty.od, 1)], ["CS", fixedNumber(difficulty.cs, 1)], ["HP", fixedNumber(difficulty.hp, 1)]].map(([label, value]) => <div key={label}><span>{label}</span><strong>{value}</strong></div>)}
    </div>
    <div className="local-stage-actions">
      <div className="local-stage-action-grid">
        <LocalAudioPreview key={`${client}:${id}`} client={client} resourceId={id} volume={volume} />
        <button type="button" className="local-stage-button" onClick={() => onOpen(id)}><ListMusic />完整详情</button>
        <button type="button" className="local-stage-button" onClick={() => { window.location.hash = similarityRouteForLocalResource(client, id, difficulty.ruleset); }}><ScanSearch />查找相似</button>
        <button type="button" className="local-stage-button" onClick={() => openCollectionDialog([{ beatmap_id: difficulty.beatmap_id, beatmapset_id: difficulty.beatmap_set_id, checksum: null, ruleset: difficulty.ruleset, difficulty_name: difficulty.difficulty_name, title: difficulty.title_unicode || difficulty.title, artist: difficulty.artist_unicode || difficulty.artist, creator: difficulty.creator, local_client: client, local_resource_id: id }])}><Heart />加入收藏夹</button>
        <button type="button" className="local-stage-button" onClick={() => { window.location.hash = `/view-trainer?${new URLSearchParams({ client, resource: id })}`; }}><WandSparkles />导入 Trainer</button>
        <button type="button" className="local-stage-button" disabled={!previewRoute} title={previewRoute ? "生成所选难度的谱面预览" : "该谱面没有在线 ID，暂不支持生成预览"} onClick={() => { if (previewRoute) window.location.hash = previewRoute; }}><ImageIcon />生成预览</button>
        {client === "stable" ? <button type="button" className="local-stage-button" disabled={!difficulty.resource.logical_path} onClick={() => { const path = difficulty.resource.logical_path; if (path) void run(() => desktopApi.openLocalResourceInExplorer(client, path)); }}><FolderSearch />打开文件</button> : <button type="button" className="local-stage-button" disabled={exporting} onClick={() => void exportSet()}><PackageOpen />{exporting ? "正在导出…" : "导出 .osz"}</button>}
        <button type="button" className="local-stage-button" disabled={!set.beatmap_set_id} title={set.beatmap_set_id ? "打开谱面集官网" : "该谱面集尚未提交"} onClick={() => void run(() => desktopApi.openExternal(`https://osu.ppy.sh/beatmapsets/${set.beatmap_set_id}`))}><ExternalLink />谱面官网</button>
        <button type="button" className="local-stage-button" onClick={() => void run(() => desktopApi.openNeteaseMusicSearch(set.artist_unicode || set.artist, set.title_unicode || set.title))}><Music4 />网易云搜索</button>
      </div>
    </div>
    {notice ? <p className="local-stage-notice" role="status">{notice}</p> : null}
  </div>;
}
