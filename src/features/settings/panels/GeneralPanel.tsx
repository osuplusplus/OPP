import { useMode } from "../../../app/ModeContext";
import { ClientSwitch } from "../../../shared/components/ClientSwitch";
import { ModeSwitch } from "../../../shared/components/ModeSwitch";
import { Card, SectionTitle } from "../../../shared/components/ui";
import { LocalScanAction } from "../LocalScanAction";

export function GeneralPanel({ onConfigure }: { onConfigure: () => void }) {
  const { client, setClient, ruleset, setRuleset } = useMode();
  return (
    <div className="space-y-5" data-onboarding="mode-and-client">
      <Card className="p-6">
        <SectionTitle title="游戏模式" description="切换后立即应用于个人资料、谱面搜索和分析，并记住你的选择。" />
        <div className="mt-5 overflow-x-auto"><ModeSwitch value={ruleset} onChange={setRuleset} /></div>
      </Card>
      <Card className="p-6">
        <SectionTitle title="osu! 客户端" description="选择本地资源使用的客户端，也是左下角启动按钮默认打开的客户端。" />
        <div className="mt-5"><ClientSwitch value={client} onChange={setClient} /></div>
        <div className="mt-4"><LocalScanAction key={client} client={client} onConfigure={onConfigure} /></div>
      </Card>
    </div>
  );
}
