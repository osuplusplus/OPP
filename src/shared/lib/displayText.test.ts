import { describe, expect, it } from "vitest";

import { displayFileName } from "./displayText";

describe("displayFileName", () => {
  it("decodes a URL-escaped downloaded file name", () => {
    expect(displayFileName("C:\\Maps\\2069950%20Vivid%20Lila.osz")).toBe("2069950 Vivid Lila.osz");
  });

  it("supports encoded Unicode and both path separators", () => {
    expect(displayFileName("/maps/%E6%B5%8B%E8%AF%95%20%E8%B0%B1%E9%9D%A2.osz")).toBe("测试 谱面.osz");
  });

  it("preserves malformed percent text", () => {
    expect(displayFileName("C:\\Maps\\100%broken.osz")).toBe("100%broken.osz");
  });
});

