import { useState } from "react";
import { ExternalLink, Gamepad2, RefreshCw, RotateCcw } from "lucide-react";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import { START_ONBOARDING_EVENT } from "../../../shared/lib/onboardingEvents";
import { requestManualUpdateCheck } from "../../updates/events";

export function AboutPanel({ className }: { className?: string }) {
  const [updateBusy, setUpdateBusy] = useState(false);
  const checkForUpdates = () => {
    setUpdateBusy(true);
    requestManualUpdateCheck(() => setUpdateBusy(false));
  };

  return (
    <Card className={className}>
      <div className="flex items-start justify-between gap-4">
        <div>
          <SectionTitle title="关于" />
          <p className="mt-3 flex items-center gap-2 text-sm text-slate-300">
            <Gamepad2 className="size-4 text-[var(--theme-primary)]" />
            OPP v{__APP_VERSION__}
          </p>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button disabled={updateBusy} loading={updateBusy} onClick={() => void checkForUpdates()} size="sm" variant="secondary">
            <RefreshCw className="size-4" />检查更新
          </Button>
          <Button onClick={() => window.dispatchEvent(new Event(START_ONBOARDING_EVENT))} size="sm" variant="secondary">
            <RotateCcw className="size-4" />重新查看新手引导
          </Button>
          <Button onClick={() => void desktopApi.openExternal("https://github.com/osuplusplus/OPP")} size="sm" variant="secondary">
            <ExternalLink className="size-4" />项目仓库
          </Button>
        </div>
      </div>
    </Card>
  );
}
