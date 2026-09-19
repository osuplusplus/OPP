import { Suspense } from "react";
import { Outlet, useLocation } from "react-router-dom";

/** A lazy feature may suspend; navigation and global controls must stay mounted. */
export function RouteContent() {
  const { pathname } = useLocation();
  return <Suspense key={pathname} fallback={<div role="status" className="grid min-h-64 place-items-center text-sm text-slate-400">正在加载页面…</div>}>
    <Outlet />
  </Suspense>;
}
