import { useEffect, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type { OverlayMpActivity } from "../../shared/types/osu";

const idle: OverlayMpActivity = { phase: "idle", title: "", message: "" };

/**
 * MP 补缺状态订阅：补缺编排完全在后端（osu! IRC 监听 → 检测 → 下载），
 * 与覆盖层窗口生命周期无关；这里仅通过事件/初始查询同步展示状态。
 */
export function useMpStatus(): OverlayMpActivity {
  const [activity, setActivity] = useState<OverlayMpActivity>(idle);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void desktopApi
      .overlayGetMpStatus()
      .then((status) => {
        if (!disposed) setActivity(status.mp);
      })
      .catch(() => {});
    void desktopApi.onOverlayMpActivity((value) => setActivity(value)).then((unlisten) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    });
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  return activity;
}
