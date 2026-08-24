import { describe, expect, it } from "vitest";

import { usesBeatmapWorkspaceLayout } from "./workspaceLayout";

describe("usesBeatmapWorkspaceLayout", () => {
  it.each(["/online/beatmaps", "/collections", "/local/maps"])(
    "保留 %s 的谱面专用布局",
    (pathname) => {
      expect(usesBeatmapWorkspaceLayout(pathname)).toBe(true);
    },
  );

  it.each(["/settings", "/tools", "/data/overview", "/local/media"])(
    "为 %s 启用统一工作台样式",
    (pathname) => {
      expect(usesBeatmapWorkspaceLayout(pathname)).toBe(false);
    },
  );
});
