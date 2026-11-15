import { beforeEach, describe, expect, it } from "vitest";
import {
  clearExportProfile,
  defaultExportForm,
  loadExportProfile,
  normalizeExportForm,
  parentDir,
  presetOptions,
  recommendedSettings,
  resolveAutoEncoder,
  saveExportProfile,
  splitArgs,
  tuneOptions,
} from "./exportProfile";

describe("export profile", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("normalizes dirty values into the supported ranges", () => {
    const form = normalizeExportForm({
      resolution: "not-a-size",
      fps: 999,
      encoder: "av1",
      quality: 99,
      preset: "p9",
      tune: "bogus",
      rc: "weird",
      qp: 99,
      gop: -3,
      extraArgs: 42,
      audio: false,
      results: false,
      gpuYuv: false,
      start: -1,
      end: 1200,
      audioOffset: 15,
    });
    expect(form.resolution).toBe(defaultExportForm.resolution);
    expect(form.fps).toBe(240);
    expect(form.encoder).toBe("auto");
    expect(form.quality).toBe(51);
    expect(form.preset).toBe("");
    expect(form.tune).toBe("");
    expect(form.rc).toBe("vbr");
    expect(form.qp).toBe(51);
    expect(form.gop).toBe(1);
    expect(form.extraArgs).toBe("");
    expect(form.audio).toBe(false);
    expect(form.results).toBe(false);
    expect(form.gpuYuv).toBe(false);
    expect(form.start).toBeNull();
    expect(form.end).toBe(1200);
    expect(form.audioOffset).toBe(15);
  });

  it("round-trips the form and default folder", () => {
    const form = { ...defaultExportForm, encoder: "hevc_nvenc" as const, preset: "p7", quality: 21, resultsStyle: "stable" as const, extraArgs: "-b:v 20M" };
    saveExportProfile(form, "  /videos/out  ");
    const loaded = loadExportProfile();
    expect(loaded?.export.encoder).toBe("hevc_nvenc");
    expect(loaded?.export.preset).toBe("p7");
    expect(loaded?.export.quality).toBe(21);
    expect(loaded?.export.resultsStyle).toBe("stable");
    expect(loaded?.export.extraArgs).toBe("-b:v 20M");
    expect(loaded?.outputDirectory).toBe("/videos/out");
    expect(loaded?.updatedAt).toBeTruthy();
  });

  it("migrates the legacy profiles array and can be cleared", () => {
    window.localStorage.setItem("opp:replay-export-profiles.v1", JSON.stringify([{ export: { resolution: "1280x720", fps: 30 }, outputDirectory: "/old" }]));
    const loaded = loadExportProfile();
    expect(loaded?.export.resolution).toBe("1280x720");
    expect(loaded?.export.fps).toBe(30);
    expect(loaded?.outputDirectory).toBe("/old");
    clearExportProfile();
    expect(loadExportProfile()).toBeNull();
  });

  it("offers encoder-specific preset and tune lists", () => {
    expect(presetOptions("x264")).toContain("veryfast");
    expect(presetOptions("hevc_nvenc")).toContain("p7");
    expect(presetOptions("hevc_nvenc")).not.toContain("veryfast");
    expect(tuneOptions("x265")).toContain("animation");
    expect(tuneOptions("nvenc")).toContain("ull");
  });

  it("auto prefers HEVC NVENC, then H.264 NVENC, then x264", () => {
    const encoder = (available: string[]) => resolveAutoEncoder({
      ffmpegAvailable: true,
      hevcAvailable: available.includes("hevc_nvenc") || available.includes("x265"),
      nvencAvailable: available.includes("nvenc") || available.includes("hevc_nvenc"),
      encoders: ["x264", "x265", "nvenc", "hevc_nvenc"].map((id) => ({
        id: id as "x264" | "x265" | "nvenc" | "hevc_nvenc",
        codec: id,
        media: id === "x265" || id === "hevc_nvenc" ? "hevc" as const : "h264" as const,
        hardware: id === "nvenc" || id === "hevc_nvenc",
        available: available.includes(id),
      })),
    });
    expect(encoder(["x264", "x265", "nvenc", "hevc_nvenc"])).toBe("hevc_nvenc");
    expect(encoder(["x264", "x265"])).toBe("x264");
    expect(encoder(["x264", "nvenc"])).toBe("nvenc");
    expect(encoder(["x265", "hevc_nvenc"])).toBe("hevc_nvenc");
    expect(encoder(["x265"])).toBe("x264");
    expect(encoder(["x264"])).toBe("x264");
    expect(resolveAutoEncoder(undefined)).toBe("x264");
  });

  it("exposes measured best-value presets per encoder", () => {
    expect(recommendedSettings("hevc_nvenc")).toEqual({ preset: "p1", tune: "ull", quality: 30 });
    expect(recommendedSettings("nvenc")).toEqual({ preset: "p1", tune: "ull", quality: 26 });
    expect(recommendedSettings("x264")).toEqual({ preset: "superfast", tune: "", quality: 22 });
    expect(defaultExportForm.encoder).toBe("auto");
    expect(defaultExportForm.quality).toBe(30);
  });

  it("extracts the parent directory for remembering the last export path", () => {
    expect(parentDir("C:\\videos\\clips\\a.mp4")).toBe("C:\\videos\\clips");
    expect(parentDir("/home/user/videos/a.mp4")).toBe("/home/user/videos");
    expect(parentDir("a.mp4")).toBe("");
  });

  it("splits quoted extra ffmpeg arguments", () => {
    expect(splitArgs('-b:v 20M -metadata "title=my clip"')).toEqual(["-b:v", "20M", "-metadata", "title=my clip"]);
    expect(splitArgs("")).toEqual([]);
  });
});
