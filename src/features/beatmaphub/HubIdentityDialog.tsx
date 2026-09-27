import { useMutation, useQueryClient } from "@tanstack/react-query";
import { RefreshCw, ShieldCheck } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import { hubApi, hubKey, useBeatmapHubAuth, useBeatmapHubProfile } from "./api";
import { hubError } from "./model";

export function HubIdentityDialog({ open, onClose, onConnected }: { open: boolean; onClose: () => void; onConnected: () => void }) {
  const client = useQueryClient();
  const auth = useBeatmapHubAuth();
  const profile = useBeatmapHubProfile(open && Boolean(auth.data?.connected));
  const retry = useMutation({
    mutationFn: async () => {
      try {
        return await hubApi.login();
      } catch (error) {
        if ((error as { code?: string }).code === "DEVICE_REVOKED") return hubApi.reconnect();
        throw error;
      }
    },
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: hubKey });
      onConnected();
    },
  });
  return (
    <AppDialog overlayProps={{ className: "hub-overlay" }} open={open} onOpenChange={(value) => { if (!value) onClose(); }} title={auth.data?.connected ? "PackHub 连接状态" : "连接 BeatmapHub"} description="PackHub 使用当前 OPP 设备身份自动连接，不需要单独注册或输入链接码。" icon={<ShieldCheck size={22} />} closeDisabled={retry.isPending} contentClassName="hub-dialog">
      {auth.isPending ? <p role="status">正在读取连接状态…</p> : auth.data?.connected ? <div className="hub-form">
        <div className="hub-identity-summary"><ShieldCheck size={28} /><div><h3>{auth.data.display_name ?? "PackHub 设备"}</h3><p className="hub-muted">设备 · {auth.data.device_name ?? "OPP Desktop"}</p></div></div>
        {profile.isPending ? <p role="status">正在读取设备…</p> : profile.data?.devices.filter((item) => !item.revoked_at).map((item) => <div className="hub-device" key={item.id}><strong>{item.device_name}{item.id === profile.data?.current_device_id ? "（当前）" : ""}</strong><span className="hub-muted">最近使用 {new Date(item.last_seen_at).toLocaleString()}</span></div>)}
        <Button variant="ghost" onClick={() => void hubApi.logout().then(() => client.invalidateQueries({ queryKey: hubKey }))}>退出 PackHub 会话</Button>
      </div> : <div className="hub-form">
        <p>osu! 已登录，PackHub 尚未完成设备认证。</p>
        <Button variant="primary" loading={retry.isPending} onClick={() => retry.mutate()}><RefreshCw size={16} />重新连接</Button>
        {auth.error || retry.error ? <p className="hub-error" role="alert">{hubError(retry.error ?? auth.error)}</p> : null}
      </div>}
    </AppDialog>
  );
}
