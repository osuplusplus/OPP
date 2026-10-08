import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ShieldCheck } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import { communityIdentityKey, communityProfileKey, connectCommunityIdentity } from "../../shared/lib/communityIdentity";
import { communityError } from "./model";

export function CommunityIdentityDialog({ onClose, onConnected }: { onClose: () => void; onConnected: () => void }) {
  const client = useQueryClient();
  const connect = useMutation({ mutationFn: connectCommunityIdentity, onSuccess: async () => {
    await Promise.all([client.invalidateQueries({ queryKey: communityIdentityKey }), client.invalidateQueries({ queryKey: communityProfileKey })]);
    onConnected();
  } });
  return <AppDialog open onOpenChange={(open) => { if (!open && !connect.isPending) onClose(); }} title="连接社区身份" icon={<ShieldCheck size={22} />} description="使用当前 OPP 设备身份，无需单独注册。" closeDisabled={connect.isPending} size="sm" footer={<><Button disabled={connect.isPending} onClick={onClose}>稍后再说</Button><Button variant="primary" loading={connect.isPending} onClick={() => connect.mutate()}>连接身份</Button></>}>
    <p>发布和管理约玩信息需要连接社区身份。已经填写的内容会保留，连接后请再次确认提交。</p>{connect.error ? <p className="community-error" role="alert">{communityError(connect.error)}</p> : null}
  </AppDialog>;
}
