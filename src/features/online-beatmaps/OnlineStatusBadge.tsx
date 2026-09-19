import { BadgeCheck, CheckCheck, CircleHelp, Clock3, Heart, Pencil, Archive, CircleCheck } from "lucide-react";

const statuses = {
  ranked: { label: "上架 · Ranked", Icon: BadgeCheck },
  approved: { label: "批准 · Approved", Icon: CheckCheck },
  qualified: { label: "过审 · Qualified", Icon: CircleCheck },
  loved: { label: "社区喜爱 · Loved", Icon: Heart },
  pending: { label: "待定 · Pending", Icon: Clock3 },
  wip: { label: "制作中 · WIP", Icon: Pencil },
  graveyard: { label: "坟场 · Graveyard", Icon: Archive },
};

export function OnlineStatusBadge({ status }: { status: string }) {
  const { label, Icon } = statuses[status as keyof typeof statuses] ?? { label: status || "未知状态", Icon: CircleHelp };
  return <span className="online-status-badge" data-status={status} title={label}><Icon aria-hidden="true" /><span>{label}</span></span>;
}
