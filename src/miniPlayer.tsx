import ReactDOM from "react-dom/client";
import { MusicPlayer } from "./features/music-player/MusicPlayer";
import { musicApi } from "./features/music-player/api";
import "./features/music-player/miniBase.css";

void musicApi.settings().then((settings) => {
  document.documentElement.dataset.themeMode = settings.theme_mode;
  document.documentElement.dataset.themePrimary = settings.theme_primary;
}).catch(() => {});
document.addEventListener("contextmenu", (event) => event.preventDefault());
ReactDOM.createRoot(document.getElementById("root")!).render(<MusicPlayer mini />);
