import { useState } from "react";
import { SettingsLayout, type SettingsCategory } from "./SettingsLayout";
import { LogsPanel } from "./panels/LogsPanel";
import { AccountPanel } from "./panels/AccountPanel";
import { AppearancePanel } from "./panels/AppearancePanel";
import { OnlinePanel } from "./panels/OnlinePanel";
import { DirectoriesPanel } from "./panels/DirectoriesPanel";
import { AboutPanel } from "./panels/AboutPanel";
import { useSettings } from "./api";
import { desktopApi } from "../../shared/lib/tauri";
import { Card } from "../../shared/components/ui";

// 临时占位组件
function ComingSoonPanel({ title }: { title: string }) {
  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-2xl font-bold text-white">{title}</h2>
        <p className="mt-1 text-sm text-slate-400">此面板正在迁移中...</p>
      </div>
      <Card className="p-6">
        <p className="text-slate-400">内容即将推出</p>
      </Card>
    </div>
  );
}

export function SettingsPageNew() {
  const [activeCategory, setActiveCategory] = useState<SettingsCategory>("logs");
  const [busy, setBusy] = useState(false);
  const { data: settings, refetch: refetchSettings } = useSettings();

  const save = async (newSettings: typeof settings) => {
    if (!newSettings) return;
    setBusy(true);
    try {
      await desktopApi.updateSettings(newSettings);
      await refetchSettings();
    } finally {
      setBusy(false);
    }
  };

  const renderPanel = () => {
    if (!settings) {
      return (
        <div className="flex items-center justify-center py-12">
          <p className="text-slate-400">加载中...</p>
        </div>
      );
    }

    switch (activeCategory) {
      case "logs":
        return <LogsPanel />;
      case "account":
        return <AccountPanel />;
      case "appearance":
        return <AppearancePanel settings={settings} save={save} />;
      case "online":
        return <OnlinePanel settings={settings} save={save} busy={busy} />;
      case "directories":
        return <DirectoriesPanel />;
      case "replay":
        return <ComingSoonPanel title="回放渲染" />;
      case "tools":
        return <ComingSoonPanel title="工具与缓存" />;
      case "similarity":
        return <ComingSoonPanel title="相似谱面" />;
      case "about":
        return <AboutPanel />;
      default:
        return <ComingSoonPanel title="未知" />;
    }
  };

  return (
    <SettingsLayout
      activeCategory={activeCategory}
      onCategoryChange={setActiveCategory}
    >
      {renderPanel()}
    </SettingsLayout>
  );
}
