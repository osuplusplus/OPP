import { useState, type ReactNode } from "react";
import { Button } from "./ui";

/** Bound the mounted DOM even when the cached collection contains thousands of items. */
export function PaginatedList<T>({ items, pageSize, label, children }: {
  items: readonly T[];
  pageSize: number;
  label: string;
  children: (page: readonly T[]) => ReactNode;
}) {
  const [requestedPage, setPage] = useState(0);
  const pages = Math.max(1, Math.ceil(items.length / pageSize));
  const page = Math.min(requestedPage, pages - 1);
  return <>
    {children(items.slice(page * pageSize, (page + 1) * pageSize))}
    {pages > 1 ? <nav aria-label={`${label}分页`} className="flex items-center justify-between gap-3 px-3 py-3 text-xs text-slate-400">
      <span>{page * pageSize + 1}–{Math.min((page + 1) * pageSize, items.length)} / {items.length}</span>
      <span className="flex items-center gap-2">
        <Button aria-label={`${label}上一页`} disabled={page === 0} onClick={() => setPage(page - 1)} size="sm" variant="ghost">上一页</Button>
        <span>{page + 1} / {pages}</span>
        <Button aria-label={`${label}下一页`} disabled={page === pages - 1} onClick={() => setPage(page + 1)} size="sm" variant="ghost">下一页</Button>
      </span>
    </nav> : null}
  </>;
}
