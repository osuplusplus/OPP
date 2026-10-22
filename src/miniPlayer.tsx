import ReactDOM from "react-dom/client";
import { MusicPlayer } from "./features/music-player/MusicPlayer";
import { musicApi } from "./features/music-player/api";
import { applyEntryTheme, blockContextMenu } from "./shared/lib/entryBootstrap";
import "./features/music-player/miniBase.css";

applyEntryTheme(musicApi.settings());
blockContextMenu();
ReactDOM.createRoot(document.getElementById("root")!).render(<MusicPlayer mini />);
