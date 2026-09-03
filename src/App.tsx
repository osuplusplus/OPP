import { useEffect, useState } from "react";
import * as Tooltip from "@radix-ui/react-tooltip";
import { HashRouter } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppConnectionGate } from "./app/AppConnectionGate";
import { TitleBar } from "./shared/components/TitleBar";
import { useSettings } from "./features/settings/api";
import { Button, Card } from "./shared/components/ui";
import { desktopApi, isTauri } from "./shared/lib/tauri";

function ThemeController() {
  const settings = useSettings();
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.themePrimary = settings.data?.theme_primary ?? "cyan";
    root.dataset.themeSecondary = settings.data?.theme_secondary ?? "cyan";
    root.dataset.themeMode = settings.data?.theme_mode ?? "dark";
  }, [
    settings.data?.theme_mode,
    settings.data?.theme_primary,
    settings.data?.theme_secondary,
  ]);
  return null;
}

function WebContextMenuBlocker() {
  useEffect(() => {
    const block = (event: MouseEvent) => event.preventDefault();
    window.addEventListener("contextmenu", block);
    return () => window.removeEventListener("contextmenu", block);
  }, []);
  return null;
}

function ClientErrorLogging() {
  useEffect(() => {
    const onError = (event: ErrorEvent) => { void desktopApi.writeClientLog("error", "frontend.window", `${event.message} (${event.filename}:${event.lineno})`); };
    const onRejection = (event: PromiseRejectionEvent) => { void desktopApi.writeClientLog("error", "frontend.promise", String(event.reason)); };
    window.addEventListener("error", onError); window.addEventListener("unhandledrejection", onRejection);
    return () => { window.removeEventListener("error", onError); window.removeEventListener("unhandledrejection", onRejection); };
  }, []);
  return null;
}

function ShutdownChoice() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    const appWindow = getCurrentWindow();
    const requestChoice = () => setOpen(true);
    const onCloseRequested = (event: { preventDefault: () => void }) => {
      event.preventDefault();
      requestChoice();
    };
    let unlisten: (() => void) | undefined;
    void appWindow.onCloseRequested(onCloseRequested).then((dispose) => {
      unlisten = dispose;
    });
    window.addEventListener("opp:request-close", requestChoice);
    return () => {
      unlisten?.();
      window.removeEventListener("opp:request-close", requestChoice);
    };
  }, []);

  const minimizeToTray = async () => {
    setOpen(false);
    await getCurrentWindow().hide();
  };

  const closeApp = async () => {
    setOpen(false);
    await desktopApi.exitApp();
  };

  if (!open) return null;
  return (
    <div className="fixed inset-0 z-[300] grid place-items-center bg-black/55 p-6 backdrop-blur-sm">
      <Card aria-describedby="shutdown-choice-description" aria-labelledby="shutdown-choice-title" className="w-full max-w-md p-6 shadow-2xl" role="dialog">
        <h2 className="text-lg font-semibold text-white" id="shutdown-choice-title">关闭 OPP？</h2>
        <p className="mt-2 text-sm leading-6 text-slate-400" id="shutdown-choice-description">
          你可以直接退出程序，或将窗口最小化到系统托盘；点击托盘图标即可重新打开。
        </p>
        <div className="mt-6 flex justify-end gap-2">
          <Button onClick={() => setOpen(false)} variant="ghost">取消</Button>
          <Button onClick={() => void minimizeToTray()} variant="secondary">最小化到托盘</Button>
          <Button onClick={() => void closeApp()} variant="primary">直接关闭</Button>
        </div>
      </Card>
    </div>
  );
}

export default function App() {
  return (
    <Tooltip.Provider delayDuration={350}>
      <ThemeController />
      <WebContextMenuBlocker />
      <ClientErrorLogging />
      <ShutdownChoice />
      <HashRouter>
        <TitleBar />
        <AppConnectionGate />
      </HashRouter>
    </Tooltip.Provider>
  );
}
