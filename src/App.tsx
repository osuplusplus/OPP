import { useEffect, useState } from "react";
import * as Tooltip from "@radix-ui/react-tooltip";
import { HashRouter } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppConnectionGate } from "./app/AppConnectionGate";
import { TitleBar } from "./shared/components/TitleBar";
import { useSettings } from "./features/settings/api";
import { Button } from "./shared/components/ui";
import { AppDialog } from "./shared/components/AppDialog";
import { NotificationViewport } from "./shared/components/notifications";
import { desktopApi, isTauri } from "./shared/lib/tauri";
import { musicApi } from "./features/music-player/api";
import { LocalDatabaseGate } from "./features/local-database/DatabaseSetup";

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
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void appWindow.onCloseRequested(onCloseRequested).then((dispose) => {
      if (disposed) dispose(); else unlisten = dispose;
    });
    window.addEventListener("opp:request-close", requestChoice);
    return () => {
      disposed = true;
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

  return (
    <AppDialog
      footer={(
        <>
          <Button onClick={() => setOpen(false)} variant="ghost">取消</Button>
          <Button onClick={() => void minimizeToTray()} variant="secondary">最小化到托盘</Button>
          <Button onClick={() => void closeApp()} variant="primary">直接关闭</Button>
        </>
      )}
      onOpenChange={setOpen}
      open={open}
      overlayProps={{ className: "z-[300]" }}
      size="sm"
      title="关闭 OPP？"
      description="你可以直接退出程序，或将窗口最小化到系统托盘；点击托盘图标即可重新打开。"
      contentClassName="z-[310]"
    >
      <p className="text-sm leading-6 text-slate-300">后台下载与分析任务会在直接关闭后停止。</p>
    </AppDialog>
  );
}

export default function App() {
  useEffect(() => {
    void musicApi.ready().catch((error) => { void desktopApi.writeClientLog("error", "music.window", String(error)); });
  }, []);
  return (
    <Tooltip.Provider delayDuration={350}>
      <ThemeController />
      <WebContextMenuBlocker />
      <ClientErrorLogging />
      <ShutdownChoice />
      <NotificationViewport />
      <HashRouter>
        <TitleBar />
        <LocalDatabaseGate><AppConnectionGate /></LocalDatabaseGate>
      </HashRouter>
    </Tooltip.Provider>
  );
}
