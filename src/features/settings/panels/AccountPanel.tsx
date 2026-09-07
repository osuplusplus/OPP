import { useState } from "react";
import { LogOut, RotateCcw } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Card, SectionTitle, Button, Badge, DataLine } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import { authQueryKey, useAuthStatus } from "../../auth/api";

export function AccountPanel() {
  const queryClient = useQueryClient();
  const auth = useAuthStatus();
  const [accountBusy, setAccountBusy] = useState<"reauth" | "logout" | null>(null);
  const [accountError, setAccountError] = useState<string | null>(null);

  const reauthenticate = async () => {
    setAccountBusy("reauth");
    setAccountError(null);
    try {
      await desktopApi.beginOAuthLogin();
      await queryClient.invalidateQueries({ queryKey: authQueryKey });
    } catch (error) {
      setAccountError(String(error));
    } finally {
      setAccountBusy(null);
    }
  };

  const logout = async () => {
    setAccountBusy("logout");
    setAccountError(null);
    try {
      await desktopApi.disconnectOsu();
      await queryClient.invalidateQueries({ queryKey: authQueryKey });
    } catch (error) {
      setAccountError(String(error));
    } finally {
      setAccountBusy(null);
    }
  };

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-2xl font-bold text-white">账户</h2>
        <p className="mt-1 text-sm text-slate-400">
          管理 osu! 账户连接和认证状态。
        </p>
      </div>

      <Card className="p-6">
        <div className="flex justify-between">
          <SectionTitle title="账户状态" />
          <Badge tone={auth.data?.connected ? "success" : "warning"}>
            {auth.data?.connected ? "已连接" : "未连接"}
          </Badge>
        </div>

        <div className="mt-4">
          <DataLine label="账户" value={auth.data?.username ?? "—"} />
          <DataLine label="用户 ID" value={auth.data?.user_id ?? "—"} />
        </div>

        <div className="mt-4 flex flex-wrap gap-2 border-t border-[var(--line-subtle)] pt-4">
          <Button
            disabled={accountBusy !== null}
            loading={accountBusy === "reauth"}
            onClick={() => void reauthenticate()}
            size="sm"
            variant="secondary"
          >
            <RotateCcw className="size-4" />
            重新认证
          </Button>

          <Button
            disabled={!auth.data?.connected || accountBusy !== null}
            loading={accountBusy === "logout"}
            onClick={() => void logout()}
            size="sm"
            variant="ghost"
          >
            <LogOut className="size-4" />
            退出账号
          </Button>
        </div>

        {accountError ? (
          <p className="mt-3 text-xs text-rose-200">{accountError}</p>
        ) : null}
      </Card>
    </div>
  );
}
