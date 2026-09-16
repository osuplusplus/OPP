import { useState } from "react";
import { LoaderCircle, Pause, Play } from "lucide-react";
import type { OsuClient } from "../../shared/types/osu";
import { musicApi, musicError, musicTime, useMusicState } from "../music-player/api";

export function LocalAudioPreview({ client, resourceId }: { client: OsuClient; resourceId: string; volume: number }) {
  const state = useMusicState();
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const selected = state.resource_id === resourceId;
  const playing = selected && state.playing;
  const toggle = async () => {
    setLoading(true); setError(null);
    try {
      if (selected) await musicApi.control({ action: "toggle" });
      else await musicApi.setQueue({ client, resource_id: resourceId, append: true, preview: true });
    } catch (error) { setError(musicError(error)); } finally { setLoading(false); }
  };
  return <div className="local-stage-audio">
    <button className="local-stage-button" type="button" aria-label={playing ? "暂停试听" : "试听本地音频"} title="使用本地播放器试听" disabled={loading} onClick={() => void toggle()}>{loading ? <LoaderCircle className="animate-spin" /> : playing ? <Pause /> : <Play />}</button>
    {selected && state.duration > 0 && <div className="local-stage-audio-progress"><input aria-label="试听进度" type="range" min={0} max={state.duration} step={.5} value={Math.min(state.position,state.duration)} onChange={(event) => void musicApi.control({ action: "seek", seconds: Number(event.target.value) }).catch((e) => setError(musicError(e)))} /><span>{musicTime(state.position)} / {musicTime(state.duration)}</span></div>}
    {error && <span className="local-stage-error" role="alert">{error}</span>}
  </div>;
}
