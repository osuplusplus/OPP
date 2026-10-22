import { useState } from "react";
import { LogOut, RotateCcw } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Badge, Button, Card, DataLine, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import { authQueryKey, useAuthStatus } from "../../auth/api";

export function AccountPanel({ className }: { className?: string }) {
  const auth = useAuthStatus();
  const queryClient = useQueryClient();
  const [accountBusy, setAccountBusy] = useState<"logout" | "reauth" | null>(null);
  const [accountError, setAccountError] = useState<string | null>(null);

  const refreshAccount = async () => {
    await queryClient.invalidateQueries({ queryKey: authQueryKey });
    await queryClient.invalidateQueries({ queryKey: ["own-profile"] });
    await queryClient.invalidateQueries({ queryKey: ["scores"] });
  };

  const logout = async () => {
    setAccountBusy("logout");
    setAccountError(null);
    try {
      await desktopApi.disconnectOsu(true);
      await refreshAccount();
    } catch (error) {
      setAccountError((error as { message?: string }).message ?? String(error));
    } finally {
      setAccountBusy(null);
    }
  };

  const reauthenticate = async () => {
    setAccountBusy("reauth");
    setAccountError(null);
    try {
      await desktopApi.disconnectOsu(false);
      await refreshAccount();
      const pending = await desktopApi.beginOAuthLogin();
      await desktopApi.openExternal(pending.authorization_url);
    } catch (error) {
      setAccountError((error as { message?: string }).message ?? String(error));
    } finally {
      setAccountBusy(null);
    }
  };

  return (
    <Card className={className}>
      <div className="flex justify-between">
        <SectionTitle title="账户" />
        <Badge tone={auth.data?.connected ? "success" : "warning"}>
          {auth.data?.connected ? "已连接" : "未连接"}
        </Badge>
      </div>
      <div className="mt-4">
        <DataLine label="账户" value={auth.data?.username ?? "—"} />
        <DataLine label="用户 ID" value={auth.data?.user_id ?? "—"} />
      </div>
      <div className="mt-4 flex flex-wrap gap-2 border-t border-[var(--line-subtle)] pt-4">
        <Button disabled={accountBusy !== null} loading={accountBusy === "reauth"} onClick={() => void reauthenticate()} size="sm" variant="secondary">
          <RotateCcw className="size-4" />重新认证
        </Button>
        <Button disabled={!auth.data?.connected || accountBusy !== null} loading={accountBusy === "logout"} onClick={() => void logout()} size="sm" variant="ghost">
          <LogOut className="size-4" />退出账号
        </Button>
      </div>
      {accountError ? <p className="mt-3 text-xs text-rose-200">{accountError}</p> : null}
    </Card>
  );
}
