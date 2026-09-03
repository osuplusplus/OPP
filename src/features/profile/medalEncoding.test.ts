import { describe, expect, it } from "vitest";
import { repairMedalText } from "./medalEncoding";

describe("repairMedalText", () => {
  it("restores Japanese text mangled through Windows-1252", () => {
    expect(repairMedalText("çµ¶æœ›æ€§:ãƒ’ãƒ¼ãƒ­ãƒ¼æ²»ç™‚è–¬ (TV Size)")).toBe("絶望性:ヒーロー治療薬 (TV Size)");
    expect(repairMedalText("æ—¥æœ¬èªž")).toBe("日本語");
  });

  it("leaves valid and non-mojibake text unchanged", () => {
    expect(repairMedalText("Hacking to the Gate (TV Size)")).toBe("Hacking to the Gate (TV Size)");
    expect(repairMedalText("日本語の曲名")).toBe("日本語の曲名");
  });
});
