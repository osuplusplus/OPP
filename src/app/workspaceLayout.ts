// 谱面工作区已有专用的高密度布局，其余页面统一使用精简工作台样式。
export function usesBeatmapWorkspaceLayout(pathname: string) {
  return ["/online/beatmaps", "/collections", "/local/maps"].some(
    (path) => pathname === path || pathname.startsWith(`${path}/`),
  );
}
