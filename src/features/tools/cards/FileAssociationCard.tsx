import { Save } from "lucide-react";
import { ClientSwitch } from "../../../shared/components/ClientSwitch";
import { Badge, Button } from "../../../shared/components/ui";
import type { OsuClient } from "../../../shared/types/osu";

export function FileAssociationCard({ title, description, value, saving, onSave }: { title: string; description: string; value: OsuClient; saving: boolean; onSave: (client: OsuClient) => void }) {
  return <div className="rounded-2xl border border-white/[0.07] bg-white/[0.025] p-5"><div className="flex items-start justify-between gap-3"><div><h3 className="font-semibold text-white">{title}</h3><p className="mt-1 text-xs leading-5 text-slate-400">{description}</p></div><Badge tone={value === "stable" ? "pink" : "cyan"}>{value === "stable" ? "Stable" : "Lazer"}</Badge></div><div className="mt-5 flex flex-wrap items-center justify-between gap-3"><ClientSwitch value={value} onChange={onSave} /><Button loading={saving} onClick={() => onSave(value)} size="sm"><Save className="size-3.5" />保存设置</Button></div></div>;
}
