import { BadgeCheck, CheckCheck, CircleHelp, Clock3, Heart, Pencil, Archive, CircleCheck, Database, Layers, FileX2 } from "lucide-react";

const statuses = {
  ranked: { label: "上架 · Ranked", Icon: BadgeCheck },
  approved: { label: "批准 · Approved", Icon: CheckCheck },
  qualified: { label: "过审 · Qualified", Icon: CircleCheck },
  loved: { label: "社区喜爱 · Loved", Icon: Heart },
  pending: { label: "待定 · Pending", Icon: Clock3 },
  wip: { label: "制作中 · WIP", Icon: Pencil },
  graveyard: { label: "坟场 · Graveyard", Icon: Archive },
  local_present: { label: "本地已有", Icon: Database },
  local_partial: { label: "部分已有", Icon: Layers },
  local_unknown: { label: "本地状态未知", Icon: CircleHelp },
  local_missing: { label: "本地未收录", Icon: FileX2 },
};

export function OnlineStatusBadge({ status, label: customLabel, title }: { status: string; label?: string; title?: string }) {
  const { label, Icon } = statuses[status as keyof typeof statuses] ?? { label: status || "未知状态", Icon: CircleHelp };
  return <span className="online-status-badge" data-status={status} title={title ?? customLabel ?? label}><Icon aria-hidden="true" /><span>{customLabel ?? label}</span></span>;
}
