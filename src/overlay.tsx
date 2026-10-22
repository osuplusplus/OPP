import ReactDOM from "react-dom/client";
import { desktopApi } from "./shared/lib/tauri";
import { OverlayWindow } from "./features/overlay/OverlayWindow";
import { applyEntryTheme, blockContextMenu } from "./shared/lib/entryBootstrap";
import "./features/overlay/overlay.css";

applyEntryTheme(desktopApi.getSettings());
blockContextMenu();
ReactDOM.createRoot(document.getElementById("root")!).render(<OverlayWindow />);
