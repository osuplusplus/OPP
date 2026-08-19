import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowRight, Check, Copy, ExternalLink, ShieldCheck } from "lucide-react";
import type { ReactNode } from "react";
import type { AuthStatus, CommandError } from "../../shared/types/osu";
import { desktopApi } from "../../shared/lib/tauri";
import { Button, Card } from "../../shared/components/ui";
import { authQueryKey } from "./api";

const OSU_SETTINGS_URL = "https://osu.ppy.sh/home/account/edit";

function SetupStep({
  number,
  title,
  children,
}: {
  number: number;
  title: string;
  children: ReactNode;
}) {
  return (
    <li className="grid grid-cols-[2rem_1fr] gap-3">
      <span
        aria-hidden="true"
        className="grid size-8 place-items-center rounded-full border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-xs font-bold text-[var(--theme-primary-light)]"
      >
        {number}
      </span>
      <div className="min-w-0 pt-1">
        <h2 className="text-sm font-semibold text-white">{title}</h2>
        <div className="mt-1.5 text-xs leading-5 text-slate-400">{children}</div>
      </div>
    </li>
  );
}

export function AuthSetup({ status }: { status: AuthStatus }) {
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState(!status.credentials_configured);
  const [clientId, setClientId] = useState(status.client_id ?? "");
  const [clientSecret, setClientSecret] = useState("");
  const [saving, setSaving] = useState(false);
  const [authorizing, setAuthorizing] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void desktopApi.onOAuthResult(async (result) => {
      if (disposed) return;
      setAuthorizing(false);
      if (result.ok) {
        setError(null);
        await queryClient.invalidateQueries({ queryKey: authQueryKey });
      } else {
        setError(result.message);
      }
    }).then((remove) => {
      if (disposed) remove();
      else unlisten = remove;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);

  const copyCallback = async () => {
    await navigator.clipboard.writeText(status.callback_url);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  const saveCredentials = async () => {
    setSaving(true);
    setError(null);
    try {
      await desktopApi.saveOAuthCredentials(clientId, clientSecret);
      setClientSecret("");
      setEditing(false);
      await queryClient.invalidateQueries({ queryKey: authQueryKey });
    } catch (caught) {
      setError((caught as CommandError).message ?? "无法保存 OAuth 凭据");
    } finally {
      setSaving(false);
    }
  };

  const authorize = async () => {
    setAuthorizing(true);
    setError(null);
    try {
      const pending = await desktopApi.beginOAuthLogin();
      await desktopApi.openExternal(pending.authorization_url);
    } catch (caught) {
      setAuthorizing(false);
      setError((caught as CommandError).message ?? "无法启动授权");
    }
  };

  return (
    <main className="opp-auth-screen grid min-h-screen place-items-center overflow-x-hidden px-6 py-12">
      <div className="opp-auth-orbit opp-auth-orbit--one" />
      <div className="opp-auth-orbit opp-auth-orbit--two" />
      <Card className={`opp-auth-card relative z-10 w-full p-7 sm:p-9 ${editing ? "max-w-xl" : "max-w-md"}`}>
        <div className="flex items-center gap-3">
          <span className="grid size-11 place-items-center rounded-xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)]">
            <img alt="OPP" className="size-7 rounded-md" src="/03.png" />
          </span>
          <div><p className="text-lg font-semibold text-white">OPP</p><p className="text-xs text-slate-500">连接 osu! 账户</p></div>
        </div>

        <div className="mt-9">
          <p className="text-xs font-semibold uppercase tracking-[0.16em] text-[var(--theme-primary)]">{editing ? "应用设置" : "授权登录"}</p>
          <h1 className="mt-2 text-2xl font-semibold tracking-tight text-white">{editing ? "连接你的 osu! 账户" : "准备好开始了吗？"}</h1>
          <p className="mt-2 text-sm leading-6 text-slate-400">{editing ? "第一次配置？跟着下面 3 步操作，大约需要 1 分钟。" : "点击授权后将在浏览器中完成登录，然后自动返回。"}</p>
        </div>

        {editing ? <ol className="mt-7 space-y-6">
          <SetupStep number={1} title="创建 OAuth 应用">
            <p>打开 osu! 账户设置，找到页面底部的“OAuth Applications”，点击“New OAuth Application”。</p>
            <Button className="mt-3" onClick={() => void desktopApi.openExternal(OSU_SETTINGS_URL)} size="sm" variant="secondary">
              打开 osu! 账户设置 <ExternalLink className="size-3.5" />
            </Button>
          </SetupStep>

          <SetupStep number={2} title="填写应用信息">
            <p>应用名称可以填写“OPP”；将下面的地址完整复制到“Application Callback URL”。</p>
            <div className="mt-3 flex items-center gap-2 rounded-lg border border-[var(--line-subtle)] bg-black/10 p-1.5 pl-3">
              <code className="min-w-0 flex-1 select-all truncate font-mono text-[11px] text-slate-300" title={status.callback_url}>{status.callback_url}</code>
              <Button aria-label="复制回调地址" onClick={() => void copyCallback()} size="sm" variant="ghost">
                {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
                {copied ? "已复制" : "复制"}
              </Button>
            </div>
            <p className="mt-2 text-slate-500">保存应用后，请保持此页面打开，以便查看生成的凭据。</p>
          </SetupStep>

          <SetupStep number={3} title="粘贴应用凭据">
            <p>把 osu! 显示的 Client ID 和 Client Secret 分别粘贴到这里。</p>
            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <label className="block">
                <span className="mb-2 block font-medium text-slate-300">Client ID</span>
                <input autoComplete="off" className="opp-input" inputMode="numeric" onChange={(event) => setClientId(event.target.value)} placeholder="例如 12345" value={clientId} />
              </label>
              <label className="block">
                <span className="mb-2 block font-medium text-slate-300">Client Secret</span>
                <input autoComplete="off" className="opp-input" onChange={(event) => setClientSecret(event.target.value)} placeholder="粘贴 Client Secret" type="password" value={clientSecret} />
              </label>
            </div>
            <p className="mt-2 text-slate-500">凭据仅保存在本机的系统凭据管理器中，请勿分享给他人。</p>
            <Button className="mt-4 w-full" disabled={!clientId.trim() || !clientSecret.trim()} loading={saving} onClick={() => void saveCredentials()} variant="primary">
              保存并进入授权 <ArrowRight className="size-4" />
            </Button>
          </SetupStep>
        </ol> : <div className="mt-7">
          <div className="flex items-center gap-3 rounded-xl border border-white/[0.08] bg-white/[0.025] px-4 py-3"><ShieldCheck className="size-5 text-[var(--theme-primary)]" /><div><p className="text-sm font-medium text-white">应用已配置</p><p className="mt-0.5 text-xs text-slate-500">Client ID · {status.client_id}</p></div></div>
          <Button className="mt-5 w-full" loading={authorizing} onClick={() => void authorize()} variant="primary">{authorizing ? "等待浏览器授权" : "使用 osu! 授权"}{authorizing ? null : <ExternalLink className="size-4" />}</Button>
          <button className="opp-action mt-3 w-full py-2 text-xs text-slate-500 hover:text-slate-200" onClick={() => authorizing ? void desktopApi.cancelOAuthLogin().then(() => setAuthorizing(false)) : setEditing(true)} type="button">{authorizing ? "取消授权" : "修改应用凭据"}</button>
        </div>}

        {error ? <div className="mt-5 rounded-xl border border-rose-400/15 bg-rose-400/[0.08] px-3.5 py-3 text-sm leading-6 text-rose-200" role="alert">{error}</div> : null}
      </Card>
    </main>
  );
}
