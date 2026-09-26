import { Database, FolderOpen, RotateCcw } from "lucide-react";
import type { ReactNode } from "react";
import { useState } from "react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button, Card, SectionTitle } from "../../shared/components/ui";
import type { CommandError, LocalDatabaseStatus } from "../../shared/types/osu";
import { useLocalDatabaseAction, useLocalDatabaseStatus, useLocalLibraryStorage, useMigrateLocalLibrary } from "./api";

function errorMessage(error: unknown): string | undefined {
  if (!error) return undefined;
  if (typeof error === "object" && "message" in error) return String((error as CommandError).message);
  return String(error);
}

function DatabaseDetails({ status }: { status: LocalDatabaseStatus }) {
  return <div className="space-y-3 text-sm text-slate-300">
    <p>本地数据库保存谱面、皮肤和资源索引。配置后会自动导入已有的本地索引，后续扫描增量更新；音频、背景和回放文件仍保存在游戏目录。</p>
    <div className="rounded-lg border border-white/10 bg-white/[0.035] p-3">
      <p className="text-xs text-slate-500">{status.directory ? "保存目录" : "推荐保存目录"}</p>
      <p className="mt-1 break-all select-text">{status.directory ?? status.recommended_directory}</p>
    </div>
    {status.phase === "ready" ? <p>数据库可用 · 结构版本 {status.schema_version}。读取优先使用数据库；数据库不可用时使用兼容的 JSON 索引。</p> : null}
    {status.directory && status.phase === "error" ? <p>原数据库暂时无法使用。请检查磁盘和权限，或重新定位原数据库；现有功能仍可使用。</p> : null}
  </div>;
}

function DatabaseButtons({ status, action }: { status?: LocalDatabaseStatus; action: ReturnType<typeof useLocalDatabaseAction> }) {
  const busy = action.isPending;
  if (!status || status.phase === "ready") return null;
  return <>
    {status.can_initialize ? <>
      <Button disabled={busy} onClick={() => action.mutate({ kind: "recommended", directory: status.recommended_directory })} variant="primary">使用推荐位置</Button>
      <Button disabled={busy} onClick={() => action.mutate({ kind: "choose", directory: status.recommended_directory })}><FolderOpen className="size-4" />选择文件夹</Button>
    </> : <>
      <Button disabled={busy} onClick={() => action.mutate({ kind: "retry" })}><RotateCcw className="size-4" />重试打开</Button>
      {status.database_uuid ? <Button disabled={busy} onClick={() => action.mutate({ kind: "locate", directory: status.directory ?? status.recommended_directory })}><FolderOpen className="size-4" />重新定位原数据库</Button> : null}
    </>}
  </>;
}

/** Resolves local storage before authentication and feature onboarding can mount. */
export function LocalDatabaseGate({ children }: { children: ReactNode }) {
  const query = useLocalDatabaseStatus();
  const action = useLocalDatabaseAction();
  const [skipped, setSkipped] = useState(false);
  if (skipped || query.data?.phase === "ready") return children;
  if (query.isPending) return <main className="grid min-h-screen place-items-center"><p role="status" className="text-sm text-slate-400">正在打开本地数据库…</p></main>;
  const error = errorMessage(action.error ?? query.error ?? query.data?.error);
  return <AppDialog
    open
    title={query.data?.can_initialize ? "设置本地数据库保存位置" : "本地数据库需要处理"}
    description="你可以先完成配置，也可以稍后在设置中的本地数据页面处理。"
    icon={<Database className="size-5" />}
    closeDisabled={action.isPending}
    onOpenChange={(open) => { if (!open && !action.isPending) setSkipped(true); }}
    footer={<>
      <Button disabled={action.isPending} onClick={() => setSkipped(true)} variant="ghost">稍后设置</Button>
      <DatabaseButtons status={query.data} action={action} />
      {query.error ? <Button disabled={query.isFetching} onClick={() => void query.refetch()}>重试</Button> : null}
    </>}
  >
    {query.data ? <DatabaseDetails status={query.data} /> : null}
    {error ? <p role="alert" className="mt-3 text-sm text-rose-300">{error}</p> : null}
    {action.isPending ? <p role="status" className="mt-3 text-sm text-slate-400">正在处理数据库，请稍候…</p> : null}
  </AppDialog>;
}

export function LocalDatabaseCard() {
  const query = useLocalDatabaseStatus();
  const action = useLocalDatabaseAction();
  const storage = useLocalLibraryStorage(Boolean(query.data));
  const migration = useMigrateLocalLibrary();
  const error = errorMessage(action.error ?? query.error ?? query.data?.error);
  return <Card className="p-6">
    <SectionTitle title="本地数据库" description="管理谱面索引数据库的保存位置与可用状态。" />
    <div className="mt-4">
      {query.isPending ? <p role="status">正在读取数据库状态…</p> : null}
      {query.data ? <DatabaseDetails status={query.data} /> : null}
      {query.data?.phase === "unconfigured" ? <p className="mt-3 text-sm text-slate-400">尚未配置</p> : null}
      {error ? <p role="alert" className="mt-3 text-sm text-rose-300">{error}</p> : null}
      <div className="mt-4 flex flex-wrap gap-2"><DatabaseButtons status={query.data} action={action} />{query.error ? <Button onClick={() => void query.refetch()}>重试</Button> : null}</div>
      {action.isPending ? <p role="status" className="mt-3 text-sm text-slate-400">正在处理数据库，请稍候…</p> : null}
      {query.data ? <div className="mt-4 space-y-3 text-sm text-slate-300">
        {(storage.data ?? []).map((item) => <div key={item.client} className="rounded-lg border border-white/10 p-3">
          <p>{item.client === "stable" ? "Stable" : "lazer"} · {item.storage === "database" ? "数据库" : item.storage === "json" ? "JSON 回退" : "尚未扫描"} · {item.beatmap_count} 张谱面 · {item.entry_count} 个资源条目</p>
          {item.revision ? <p className="mt-1 text-xs text-slate-400">索引更新：{item.revision}</p> : null}
          {item.error ? <p role="alert" className="mt-1 text-rose-300">{item.error}</p> : null}
        </div>)}
        <p className="text-xs text-slate-400">未扫描的客户端请先完成本地扫描。同步只导入兼容索引，不会重新扫描游戏文件，也不会采集生涯数据。</p>
        <Button disabled={query.data.phase !== "ready" || migration.isPending || action.isPending} onClick={() => migration.mutate()}><RotateCcw className="size-4" />{migration.isPending ? "正在同步索引…" : "导入／同步现有索引"}</Button>
        {query.data.phase === "ready" && storage.data?.some((item) => item.error) ? <Button disabled={action.isPending || migration.isPending} onClick={() => action.mutate({ kind: "retry" })}>重试数据库并同步</Button> : null}
        {storage.error || migration.error ? <p role="alert" className="text-rose-300">{errorMessage(storage.error ?? migration.error)}</p> : null}
      </div> : null}
    </div>
  </Card>;
}
