import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Copy, Link2, ShieldCheck } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import { useAuthStatus } from "../auth/api";
import { hubApi, hubKey, useBeatmapHubAuth, useBeatmapHubProfile } from "./api";
import { copyHubCode, hubError } from "./model";

export function HubIdentityDialog({ open, onClose, onConnected }: { open: boolean; onClose: () => void; onConnected: () => void }) {
  const client = useQueryClient();
  const auth = useBeatmapHubAuth();
  const osu = useAuthStatus();
  const profile = useBeatmapHubProfile(open && Boolean(auth.data?.connected));
  const [mode, setMode] = useState<"create" | "link">("create");
  const [name, setName] = useState<string | null>(null);
  const [device, setDevice] = useState<string | null>(null);
  const [token, setToken] = useState("");
  const [replace, setReplace] = useState(false);
  const [link, setLink] = useState<{ link_token: string; expires_at: string } | null>(null);
  const [notice, setNotice] = useState("");
  const displayName = name ?? auth.data?.display_name ?? osu.data?.username ?? "Player";
  const deviceName = device ?? auth.data?.device_name ?? "Desktop PC";
  const mutation = useMutation({ mutationFn: async (action: "connect" | "logout" | "link" | { revoke: string }) => {
    setNotice("");
    if (typeof action === "object") await hubApi.revokeDevice(action.revoke);
    else if (action === "link") { setLink(await hubApi.createDeviceLink()); return; }
    else if (action === "logout") { await hubApi.logout(); setLink(null); }
    else if (auth.data?.has_identity && !replace) await hubApi.login();
    else if (mode === "create") await hubApi.createProfile(displayName.trim(), deviceName.trim());
    else await hubApi.linkDevice(token.trim(), deviceName.trim());
    await client.invalidateQueries({ queryKey: hubKey });
    if (action === "connect") { setToken(""); setReplace(false); onConnected(); }
  } });
  const close = () => { if (mutation.isPending) return; setLink(null); setToken(""); setNotice(""); mutation.reset(); onClose(); };
  const setup = !auth.data?.has_identity || replace;
  return <AppDialog overlayProps={{ className: "hub-overlay" }} open={open} onOpenChange={(value) => { if (!value) close(); }} title={auth.data?.connected ? "Hub 身份与设备" : "连接 BeatmapHub"} description="浏览和导入无需连接。连接后即可发布曲包、收藏和参与讨论。" icon={<ShieldCheck size={22} />} closeDisabled={mutation.isPending} contentClassName="hub-dialog">
    {auth.isPending ? <p role="status">正在读取身份…</p> : auth.error ? <div className="hub-error" role="alert">{hubError(auth.error)}<Button onClick={() => void auth.refetch()}>重试</Button></div> : auth.data?.connected ? <div className="hub-form">
      <div className="hub-identity-summary"><ShieldCheck size={28} /><div><h3>{auth.data.display_name}</h3><p className="hub-muted">当前设备 · {auth.data.device_name}</p></div></div>
      <div className="hub-inline-actions"><Button disabled={mutation.isPending} onClick={() => mutation.mutate("link")}><Link2 size={16} />链接另一台设备</Button><Button disabled={mutation.isPending} variant="ghost" onClick={() => mutation.mutate("logout")}>退出连接</Button></div>
      {link ? <div className="hub-feedback"><label>一次性链接码<input className="opp-input mt-2 font-mono" readOnly value={link.link_token} /></label><p>有效期至 {new Date(link.expires_at).toLocaleString()}，请在另一台设备选择“链接已有档案”。</p><Button size="sm" onClick={() => void copyHubCode(link.link_token).then((ok) => setNotice(ok ? "链接码已复制" : "无法自动复制，请选中链接码手动复制。"))}><Copy size={14} />复制链接码</Button></div> : null}
      <h3>已登记设备</h3>
      {profile.isPending ? <p role="status">正在读取设备…</p> : profile.error ? <div className="hub-error" role="alert">设备加载失败<Button onClick={() => void profile.refetch()}>重试</Button></div> : profile.data?.devices.filter((item) => !item.revoked_at).map((item) => <div className="hub-device" key={item.id}><div><strong>{item.device_name}{item.id === profile.data?.current_device_id ? "（当前）" : ""}</strong><p className="hub-muted">最近使用 {new Date(item.last_seen_at).toLocaleString()}</p></div>{item.id !== profile.data?.current_device_id ? <Button size="sm" disabled={mutation.isPending} onClick={() => mutation.mutate({ revoke: item.id })}>撤销</Button> : null}</div>)}
    </div> : <form className="hub-form" onSubmit={(event) => { event.preventDefault(); mutation.mutate("connect"); }}>
      {setup ? <>
        <div className="hub-inline-actions"><Button type="button" variant={mode === "create" ? "primary" : "secondary"} onClick={() => setMode("create")}>创建新档案</Button><Button type="button" variant={mode === "link" ? "primary" : "secondary"} onClick={() => setMode("link")}>链接已有档案</Button></div>
        {mode === "create" ? <label>显示名<input className="opp-input" maxLength={64} value={displayName} onChange={(event) => setName(event.target.value)} required /></label> : <label>一次性链接码<input className="opp-input" value={token} onChange={(event) => setToken(event.target.value)} placeholder="粘贴旧设备生成的 43 位链接码" required /></label>}
        <label>设备名<input className="opp-input" value={deviceName} maxLength={64} onChange={(event) => setDevice(event.target.value)} required /></label>
        <p className="hub-muted">独立于 osu! 账号。私钥保存在系统安全存储中；所有已登记设备均丢失时，档案无法人工找回。</p>
      </> : <><p>当前身份：{auth.data?.display_name}。重新验证设备即可继续。</p><Button type="button" variant="ghost" onClick={() => setReplace(true)}>设备已失效？创建或链接档案</Button></>}
      <Button type="submit" variant="primary" loading={mutation.isPending} disabled={setup && (!deviceName.trim() || (mode === "create" ? !displayName.trim() : token.trim().length !== 43))}>{setup ? mode === "create" ? "创建并连接" : "链接此设备" : "重新连接"}</Button>
    </form>}
    {mutation.error ? <p className="hub-error" role="alert">{hubError(mutation.error)}</p> : null}
    {notice ? <p className="hub-feedback" role="status">{notice}</p> : null}
  </AppDialog>;
}
