import type { LiveEncoderChoice, LiveEncoderId, LiveEncoderProbe } from "../../shared/lib/tauri";

/** 导出弹窗的完整参数(字符串字段保留输入框原样,提交前再解析)。 */
export interface ExportForm {
  resolution: string;
  fps: number;
  encoder: LiveEncoderChoice;
  quality: number;
  preset: string;
  tune: string;
  rc: "vbr" | "constqp" | "cbr";
  qp: number | null;
  gop: number | null;
  extraArgs: string;
  audio: boolean;
  hitsounds: boolean;
  results: boolean;
  resultsStyle: "lazer" | "stable";
  gpuYuv: boolean;
  start: number | null;
  end: number | null;
  audioOffset: number;
}

export const defaultExportForm: ExportForm = {
  resolution: "1920x1080",
  fps: 60,
  encoder: "auto",
  quality: 22,
  preset: "",
  tune: "",
  rc: "vbr",
  qp: null,
  gop: null,
  extraArgs: "",
  audio: true,
  hitsounds: true,
  results: true,
  resultsStyle: "lazer",
  gpuYuv: true,
  start: null,
  end: null,
  audioOffset: 0,
};

export interface ExportProfile {
  export: ExportForm;
  /** 默认导出文件夹;空 = 每次都询问保存位置。 */
  outputDirectory: string;
  updatedAt: string;
}

const PROFILE_KEY = "opp:replay-export-profile.v1";
const LEGACY_PROFILES_KEY = "opp:replay-export-profiles.v1";

const ENCODERS: LiveEncoderChoice[] = ["auto", "x264", "x265", "nvenc", "hevc_nvenc"];
const SOFTWARE_PRESETS = new Set(["", "ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow", "placebo"]);
const NVENC_PRESETS = new Set(["", "p1", "p2", "p3", "p4", "p5", "p6", "p7"]);

export function presetOptions(encoder: LiveEncoderId): string[] {
  return encoder === "nvenc" || encoder === "hevc_nvenc" ? [...NVENC_PRESETS] : [...SOFTWARE_PRESETS];
}

export function tuneOptions(encoder: LiveEncoderId): string[] {
  return encoder === "nvenc" || encoder === "hevc_nvenc"
    ? ["", "hq", "ll", "ull"]
    : ["", "animation", "film", "grain", "stillimage"];
}

export function isHardwareEncoder(encoder: LiveEncoderId): boolean {
  return encoder === "nvenc" || encoder === "hevc_nvenc";
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const numberOr = (value: unknown, fallback: number): number =>
  typeof value === "number" && Number.isFinite(value) ? value : fallback;

const optionalNumber = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) ? value : null;

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/** 脏数据防御:localStorage 可能来自旧版本或被手工改坏。 */
export function normalizeExportForm(input: unknown): ExportForm {
  const value = isObject(input) ? input : {};
  const resolution = (() => {
    if (typeof value.resolution !== "string" || !/^\d+x\d+$/.test(value.resolution)) return defaultExportForm.resolution;
    const [width, height] = value.resolution.split("x").map(Number);
    return width >= 16 && width <= 7680 && height >= 16 && height <= 4320 ? value.resolution : defaultExportForm.resolution;
  })();
  const encoder = ENCODERS.includes(value.encoder as LiveEncoderChoice) ? (value.encoder as LiveEncoderChoice) : defaultExportForm.encoder;
  const rc = value.rc === "constqp" || value.rc === "cbr" ? value.rc : "vbr";
  const preset = typeof value.preset === "string" ? value.preset : "";
  const tune = typeof value.tune === "string" ? value.tune : "";
  const start = optionalNumber(value.start);
  const end = optionalNumber(value.end);
  return {
    resolution,
    fps: Math.round(clamp(numberOr(value.fps, defaultExportForm.fps), 1, 240)),
    encoder,
    quality: Math.round(clamp(numberOr(value.quality, defaultExportForm.quality), 0, 51)),
    preset:
      encoder === "auto"
        ? SOFTWARE_PRESETS.has(preset) || NVENC_PRESETS.has(preset) ? preset : ""
        : (isHardwareEncoder(encoder) ? NVENC_PRESETS : SOFTWARE_PRESETS).has(preset) ? preset : "",
    tune: encoder === "auto" ? (tuneOptions("x264").includes(tune) || tuneOptions("nvenc").includes(tune) ? tune : "") : tuneOptions(encoder).includes(tune) ? tune : "",
    rc,
    qp: value.qp === null || value.qp === undefined ? null : Math.round(clamp(numberOr(value.qp, 23), 1, 51)),
    gop: value.gop === null || value.gop === undefined ? null : Math.round(clamp(numberOr(value.gop, 120), 1, 10_000)),
    extraArgs: typeof value.extraArgs === "string" ? value.extraArgs : "",
    audio: value.audio !== false,
    hitsounds: value.hitsounds !== false,
    results: value.results !== false,
    resultsStyle: value.resultsStyle === "stable" ? "stable" : "lazer",
    gpuYuv: value.gpuYuv !== false,
    start: start !== null && start >= 0 ? start : null,
    end: end !== null && end >= 0 ? end : null,
    audioOffset: numberOr(value.audioOffset, 0),
  };
}

/**
 * auto 编码器:固定 H.264,不自动选 H.265。硬件 NVENC 优先
 * (nvenc),不可用时回退软件 x264;即使机器支持 HEVC 也不自动使用。
 */
export function resolveAutoEncoder(probe: LiveEncoderProbe | undefined): LiveEncoderId {
  if (!probe) return "x264";
  return probe.encoders.some((encoder) => encoder.id === "nvenc" && encoder.available) ? "nvenc" : "x264";
}

export function loadExportProfile(): ExportProfile | null {
  try {
    const raw = window.localStorage.getItem(PROFILE_KEY);
    if (raw) {
      const parsed: unknown = JSON.parse(raw);
      if (isObject(parsed)) {
        return {
          export: normalizeExportForm(parsed.export),
          outputDirectory: typeof parsed.outputDirectory === "string" ? parsed.outputDirectory : "",
          updatedAt: typeof parsed.updatedAt === "string" ? parsed.updatedAt : new Date().toISOString(),
        };
      }
    }
    // 兼容早期多配置档数组格式:取第一份迁移到单配置。
    const legacy: unknown = JSON.parse(window.localStorage.getItem(LEGACY_PROFILES_KEY) ?? "null");
    if (Array.isArray(legacy) && legacy.length > 0 && isObject(legacy[0])) {
      const first = legacy[0];
      const profile: ExportProfile = {
        export: normalizeExportForm(first.export),
        outputDirectory: typeof first.outputDirectory === "string" ? first.outputDirectory : "",
        updatedAt: new Date().toISOString(),
      };
      saveExportProfile(profile.export, profile.outputDirectory);
      return profile;
    }
    return null;
  } catch {
    return null;
  }
}

export function saveExportProfile(form: ExportForm, outputDirectory: string): ExportProfile {
  const profile: ExportProfile = {
    export: normalizeExportForm(form),
    outputDirectory: outputDirectory.trim(),
    updatedAt: new Date().toISOString(),
  };
  try {
    window.localStorage.setItem(PROFILE_KEY, JSON.stringify(profile));
  } catch {
    // 隐私模式/配额满:记住设置失败不应打断导出。
  }
  return profile;
}

export function clearExportProfile() {
  try {
    window.localStorage.removeItem(PROFILE_KEY);
    window.localStorage.removeItem(LEGACY_PROFILES_KEY);
  } catch {
    // 同上。
  }
}

/** 拆分额外参数输入(支持单双引号,转义由 ffmpeg 自己处理)。 */
export function splitArgs(input: string): string[] {
  const out: string[] = [];
  let current = "";
  let quote: string | null = null;
  for (const ch of input) {
    if (quote) {
      if (ch === quote) quote = null;
      else current += ch;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
    } else if (/\s/.test(ch)) {
      if (current) {
        out.push(current);
        current = "";
      }
    } else {
      current += ch;
    }
  }
  if (current) out.push(current);
  return out;
}
