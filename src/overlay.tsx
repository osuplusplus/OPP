import ReactDOM from "react-dom/client";
import { desktopApi } from "./shared/lib/tauri";
import { OverlayWindow } from "./features/overlay/OverlayWindow";
import "./features/overlay/overlay.css";

void desktopApi.getSettings().then((settings) => {
  document.documentElement.dataset.themeMode = settings.theme_mode ?? "dark";
  document.documentElement.dataset.themePrimary = settings.theme_primary;
}).catch(() => {});
document.addEventListener("contextmenu", (event) => event.preventDefault());
ReactDOM.createRoot(document.getElementById("root")!).render(<OverlayWindow />);
