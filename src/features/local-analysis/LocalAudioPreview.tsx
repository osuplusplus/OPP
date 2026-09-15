import { useEffect, useRef, useState } from "react";
import { LoaderCircle, Pause, Play } from "lucide-react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { OsuClient } from "../../shared/types/osu";
import { durationLabel } from "./stageModel";

export function LocalAudioPreview({ client, resourceId, volume }: { client: OsuClient; resourceId: string; volume: number }) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const urlRef = useRef<string | null>(null);
  const requestRef = useRef(0);
  const [playing, setPlaying] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const volumeRef = useRef(volume);
  useEffect(() => {
    volumeRef.current = volume;
    if (audioRef.current) audioRef.current.volume = Math.max(0, Math.min(1, volume / 100));
  }, [volume]);
  useEffect(() => () => {
    requestRef.current += 1;
    const audio = audioRef.current;
    if (audio) {
      audio.onloadedmetadata = audio.ontimeupdate = audio.onended = audio.onerror = audio.onplaying = audio.onpause = null;
      audio.pause(); audio.removeAttribute("src"); audio.load();
    }
    if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    audioRef.current = null; urlRef.current = null;
  }, []);

  const toggle = async () => {
    if (loading) return;
    setError(null);
    if (audioRef.current) {
      const audio = audioRef.current;
      if (!audio.paused) audio.pause();
      else { try { await audio.play(); } catch (e) { setError(errorMessage(e)); } }
      return;
    }
    const request = ++requestRef.current;
    setLoading(true);
    try {
      const payload = await desktopApi.getLocalBeatmapAudio(client, resourceId);
      if (request !== requestRef.current) return;
      const bytes = Uint8Array.from(atob(payload.bytes_base64), (character) => character.charCodeAt(0));
      const url = URL.createObjectURL(new Blob([bytes], { type: payload.mime_type }));
      const audio = new Audio();
      audioRef.current = audio; urlRef.current = url;
      audio.volume = Math.max(0, Math.min(1, volumeRef.current / 100));
      audio.onloadedmetadata = () => {
        if (request !== requestRef.current) return;
        const length = Number.isFinite(audio.duration) ? audio.duration : 0;
        setDuration(length);
        audio.currentTime = payload.preview_time_ms >= 0 && payload.preview_time_ms / 1000 < length ? payload.preview_time_ms / 1000 : 0;
        setPosition(audio.currentTime);
      };
      audio.ontimeupdate = () => setPosition(audio.currentTime);
      audio.onplaying = () => { setPlaying(true); setLoading(false); };
      audio.onpause = () => setPlaying(false);
      audio.onended = () => setPlaying(false);
      audio.onerror = () => { setError("无法播放该音频，请检查文件或音频编码"); setLoading(false); setPlaying(false); };
      audio.src = url;
      await audio.play();
    } catch (e) {
      if (request === requestRef.current) { setError(errorMessage(e)); setPlaying(false); }
    } finally { if (request === requestRef.current) setLoading(false); }
  };
  return <div className="local-stage-audio">
    <button type="button" className="local-stage-button is-primary" title={loading ? "正在读取音频" : playing ? "暂停" : "播放"} disabled={loading} onClick={() => void toggle()} aria-label={playing ? "暂停试听" : "试听本地音频"}>
      {loading ? <LoaderCircle className="animate-spin" /> : playing ? <Pause /> : <Play />}
    </button>
    {duration > 0 ? <div className="local-stage-audio-progress"><input aria-label="试听进度" type="range" min={0} max={duration} step={0.1} value={Math.min(position, duration)} onChange={(event) => { const value = Number(event.target.value); if (audioRef.current) audioRef.current.currentTime = value; setPosition(value); }} /><span>{durationLabel(position)} / {durationLabel(duration)}</span></div> : null}
    {error ? <span role="alert" className="local-stage-error">{error}</span> : null}
  </div>;
}
