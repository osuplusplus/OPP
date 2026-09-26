import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Cloud, FolderOpen, MonitorPlay, MonitorUp } from "lucide-react";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { ReplayWorkspaceContext, useReplayLibrary } from "./api";
import { ReplayLibrary } from "./ReplayWorkspace";
import { DanserRenderPanel } from "./DanserRenderPanel";
import { LivePreviewPanel } from "./LivePreviewPanel";
import { OrdrRenderPanel } from "./OrdrRenderPanel";
import type { RenderProvider } from "./model";
import "./replayStudio.css";
import { useReplayRenderEvents } from "./renderEvents";
import { StudioPanelsContext } from "./studioPanels";

const providers = [
  { id: "live", label: "实时预览", icon: MonitorPlay },
  { id: "danser", label: "本地 Danser", icon: MonitorUp },
  { id: "ordr", label: "在线 o!rdr", icon: Cloud },
] as const;

export function ReplayRenderPage() {
  useReplayRenderEvents();
  const [searchParams] = useSearchParams();
  const workspace = useReplayLibrary();
  const [provider, setProvider] = useState<RenderProvider>(() => {
    if (searchParams.get("danser") === "1") return "danser";
    if (searchParams.get("live") === "1") return "live";
    const stored = window.localStorage.getItem("opp:replay-render-provider");
    return stored === "danser" || stored === "ordr" ? stored : "live";
  });
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [tasksHost, setTasksHost] = useState<HTMLDivElement | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const choose = (next: RenderProvider) => { setProvider(next); window.localStorage.setItem("opp:replay-render-provider", next); };
  const chooseFiles = async () => {
    if (choosing) return;
    setChoosing(true); setError(null);
    try {
      const result = await workspace.chooseFiles();
      if (result.failures.length) setError({ code: "REPLAY_SELECTION_FAILED", message: result.failures.map((failure) => `${failure.path}: ${failure.error.message}`).join("\n") });
    } catch (value) { setError(value); }
    finally { setChoosing(false); }
  };
  return <ReplayWorkspaceContext.Provider value={workspace}><StudioPanelsContext.Provider value={{ host: tasksHost, provider }}>
    <div className="replay-studio">
      <header className="studio-toolbar">
        <div className="studio-brand"><span className="studio-eyebrow">REPLAY STUDIO</span><strong>回放渲染</strong></div>
        <div className="studio-modes" role="tablist" aria-label="渲染方式">{providers.map(({ id, label, icon: Icon }, index) => <button key={id} id={`studio-tab-${id}`} aria-controls={`studio-panel-${id}`} aria-selected={provider === id} tabIndex={provider === id ? 0 : -1} role="tab" onClick={() => choose(id)} onKeyDown={(event) => {
          let next: number | undefined;
          if (event.key === "ArrowRight") next = (index + 1) % providers.length;
          if (event.key === "ArrowLeft") next = (index + providers.length - 1) % providers.length;
          if (event.key === "Home") next = 0;
          if (event.key === "End") next = providers.length - 1;
          if (next !== undefined) { event.preventDefault(); choose(providers[next].id); document.getElementById(`studio-tab-${providers[next].id}`)?.focus(); }
        }}><Icon />{label}</button>)}</div>
        <div className="studio-toolbar-actions"><div ref={setTasksHost} className="studio-tasks-host" /><ReplayLibrary open={libraryOpen} onOpenChange={setLibraryOpen} chooseFiles={() => void chooseFiles()} /><button className="is-primary" disabled={choosing} onClick={() => void chooseFiles()}><FolderOpen />{choosing ? "正在读取…" : "选择回放文件"}</button></div>
      </header>
      {error || workspace.listError ? <div className="studio-error"><ErrorPanel error={error || workspace.listError} onRetry={() => { setError(null); void workspace.refresh(); }} /></div> : null}
      <div className="studio-body">
        <div id="studio-panel-live" role="tabpanel" aria-labelledby="studio-tab-live" hidden={provider !== "live"}><LivePreviewPanel visible={provider === "live"} /></div>
        <div id="studio-panel-danser" role="tabpanel" aria-labelledby="studio-tab-danser" hidden={provider !== "danser"}><DanserRenderPanel /></div>
        <div id="studio-panel-ordr" role="tabpanel" aria-labelledby="studio-tab-ordr" hidden={provider !== "ordr"}><OrdrRenderPanel /></div>
      </div>
    </div>
  </StudioPanelsContext.Provider></ReplayWorkspaceContext.Provider>;
}
