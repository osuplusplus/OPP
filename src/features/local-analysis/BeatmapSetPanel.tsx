import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { LoaderCircle, Shuffle } from "lucide-react";
import type { LocalBeatmapSetSummary, OsuClient, Ruleset } from "../../shared/types/osu";
import { errorMessage } from "../../shared/lib/format";
import { useSettings } from "../settings/api";
import { LocalSetSearch } from "./LocalSetSearch";
import { StageBackground } from "./StageBackground";
import { useStageArtwork } from "./useStageArtwork";
import { StageControls } from "./StageControls";
import { createStageQuery, initialDifficulty } from "./stageModel";
import { fetchCompleteLocalSet, pickLocalSet } from "./api";
import "./localStage.css";

type Selection = { set: LocalBeatmapSetSummary; matched: Set<string>; difficultyId: string; focus: number };

export function BeatmapSetPanel({ client, ruleset, onOpen, libraryControl, libraryRevision }: {
  client: OsuClient; ruleset: Ruleset; onOpen: (resourceId: string) => void; libraryControl?: ReactNode; libraryRevision?: string;
}) {
  const queryClient = useQueryClient();
  const [query, setQuery] = useState(() => createStageQuery(client, ruleset));
  const [selection, setSelection] = useState<Selection | null>(null);
  const [busy, setBusy] = useState(true);
  const [notice, setNotice] = useState<string | null>(null);
  const request = useRef(0);
  const previousRevision = useRef(libraryRevision);
  const settings = useSettings();
  const artwork = useStageArtwork(client, selection?.set.background_resource_id ?? null);

  const commit = useCallback(async (candidate: LocalBeatmapSetSummary, token: number, focus: boolean) => {
    const set = await fetchCompleteLocalSet(queryClient, client, candidate.set_key, ruleset);
    if (request.current !== token) return;
    const matched = new Set(candidate.difficulties.map((difficulty) => difficulty.resource.resource_id));
    const difficulty = initialDifficulty(set, matched);
    if (!difficulty) throw new Error("该谱面集在当前模式下没有可用难度");
    setSelection({ set, matched, difficultyId: difficulty.resource.resource_id, focus: focus ? token : 0 });
  }, [client, ruleset, queryClient]);

  useEffect(() => {
    const token = ++request.current;
    void pickLocalSet(createStageQuery(client, ruleset)).then(async (candidate) => {
      if (request.current !== token) return;
      if (candidate) await commit(candidate, token, false);
      else setNotice("当前模式下没有本地谱面。请扫描谱库或切换游戏模式。");
    }).catch((error) => { if (request.current === token) setNotice(errorMessage(error)); })
      .finally(() => { if (request.current === token) setBusy(false); });
    return () => { request.current += 1; };
  }, [client, ruleset, commit]);

  const selectedKey = selection?.set.set_key;
  useEffect(() => {
    if (previousRevision.current === libraryRevision) return;
    previousRevision.current = libraryRevision;
    if (!selectedKey) return;
    const token = request.current;
    let active = true;
    void fetchCompleteLocalSet(queryClient, client, selectedKey, ruleset).then((set) => {
      if (!active || token !== request.current) return;
      setSelection((current) => {
        if (!current || current.set.set_key !== selectedKey) return current;
        const difficulty = set.difficulties.find((item) => item.resource.resource_id === current.difficultyId) ?? initialDifficulty(set, current.matched);
        return difficulty ? { ...current, set, difficultyId: difficulty.resource.resource_id, focus: 0 } : null;
      });
    }).catch((error) => {
      if (!active || token !== request.current) return;
      setNotice(errorMessage(error));
      if ((error as { code?: string })?.code === "LOCAL_RESOURCE_NOT_FOUND") setSelection(null);
    });
    return () => { active = false; };
  }, [client, libraryRevision, queryClient, ruleset, selectedKey]);

  const choose = async (candidate: LocalBeatmapSetSummary) => {
    const token = ++request.current;
    setBusy(true); setNotice(null);
    try { await commit(candidate, token, true); }
    catch (error) { if (token === request.current) setNotice(errorMessage(error)); }
    finally { if (token === request.current) setBusy(false); }
  };
  const random = async () => {
    const token = ++request.current;
    setBusy(true); setNotice(null);
    try {
      const candidate = await pickLocalSet(query, selection?.set.set_key ?? null);
      if (token !== request.current) return;
      if (!candidate) setNotice("没有符合当前搜索和筛选条件的谱面。");
      else if (candidate.set_key === selection?.set.set_key) setNotice("当前条件下只有这一个谱面集。");
      else await commit(candidate, token, true);
    } catch (error) { if (token === request.current) setNotice(errorMessage(error)); }
    finally { if (token === request.current) setBusy(false); }
  };
  const difficulty = selection?.set.difficulties.find((item) => item.resource.resource_id === selection.difficultyId);
  return <section className="local-stage" style={artwork.style} aria-label="本地谱面工作区" aria-busy={busy}>
    <StageBackground source={artwork.source} reduceMotion={settings.data?.reduce_motion} />
    <header className="local-stage-toolbar">
      <div className="local-stage-label"><span>LOCAL</span><strong>BEATMAPS</strong></div>
      <LocalSetSearch query={query} onQuery={setQuery} onChoose={(candidate) => void choose(candidate)} />
      <button type="button" className="local-stage-button local-stage-random" disabled={busy} onClick={() => void random()} title="从当前搜索与筛选结果随机选择"><Shuffle />随机一首</button>
      {libraryControl}
    </header>
    <div className="local-stage-space" />
    <div className="local-stage-content">
      {notice ? <p className="local-stage-notice" role="status">{notice}</p> : null}
      {selection && difficulty ? <>
        <div className="local-stage-song" key={`song:${selection.set.set_key}`}>
          <div className="local-stage-song-meta"><span>{selection.set.beatmap_set_id ? "已提交" : "本地谱面"}</span><span>{selection.set.beatmap_set_id ? `SET / ${selection.set.beatmap_set_id}` : "LOCAL / COLLECTION"}</span>{selection.set.grouping_inferred ? <span>推断分组</span> : null}</div>
          <p>{selection.set.artist_unicode || selection.set.artist}</p>
          <h1>{selection.set.title_unicode || selection.set.title}</h1>
          <div className="local-stage-mapper">谱师 <strong>{selection.set.creators.join(" / ") || "未知"}</strong>{busy ? <span><LoaderCircle className="animate-spin" />正在准备谱面…</span> : null}</div>
        </div>
        <StageControls key={`controls:${selection.set.set_key}`} client={client} set={selection.set} difficulty={difficulty} focusSelection={selection.focus} volume={settings.data?.preview_volume ?? 65} onOpen={onOpen} onDifficulty={(difficultyId) => setSelection({ ...selection, difficultyId })} />
      </> : <div className="local-stage-welcome"><span>YOUR LOCAL COLLECTION</span><h1>{busy ? "正在寻找下一首" : "从一首谱面开始"}</h1><p>{busy ? "正在准备本地谱面与背景…" : "搜索标题、艺术家或谱师，按 Enter 展开谱面。"}</p>{busy ? <LoaderCircle className="animate-spin" /> : null}</div>}
    </div>
  </section>;
}
