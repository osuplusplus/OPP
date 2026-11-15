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
  // auto 的推荐档位：HEVC NVENC p1/CQ30（实测最快且体积最小）。
  quality: 30,
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

/** 各编码器的实测性价比档位（1080p ReI 全流程基准）。 */
export interface RecommendedSettings {
  preset: string;
  tune: string;
  quality: number;
}

export const RECOMMENDED_SETTINGS: Record<LiveEncoderId, RecommendedSettings> = {
  // 707 fps / 161 MB / SSIM 0.991
  hevc_nvenc: { preset: "p1", tune: "ull", quality: 30 },
  // 656 fps / 247 MB
  nvenc: { preset: "p1", tune: "ull", quality: 26 },
  // 443 fps / 201 MB（superfast + CRF22，观感≈veryfast/18）
  x264: { preset: "superfast", tune: "", quality: 22 },
  // 软件 HEVC 慢，保持画质档
  x265: { preset: "fast", tune: "", quality: 22 },
};

export function recommendedSettings(encoder: LiveEncoderId): RecommendedSettings {
  return RECOMMENDED_SETTINGS[encoder];
}

/** preset 下拉里的简短提示（速度/画质倾向）。 */
export function presetHint(encoder: LiveEncoderId, preset: string): string {
  if (!preset) return "";
  if (isHardwareEncoder(encoder)) {
    const hints: Record<string, string> = { p1: "最快", p2: "很快", p3: "偏快", p4: "平衡", p5: "偏清晰", p6: "清晰", p7: "最清晰" };
    return hints[preset] ?? "";
  }
  const hints: Record<string, string> = {
    ultrafast: "最快", superfast: "推荐", veryfast: "快", faster: "较快", fast: "平衡",
    medium: "偏清晰", slow: "清晰", slower: "更清晰", veryslow: "很慢", placebo: "最慢",
  };
  return hints[preset] ?? "";
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
 * auto 编码器:优先 HEVC NVENC,其次 H.264 NVENC,最后软件 x264。
 * 顺序即实测性价比(1080p hevc p1/CQ30 约 707 fps、161 MB;nvenc p1/CQ26
 * 约 656 fps;x264 superfast/CRF22 约 443 fps)。x265 软件 HEVC 太慢,
 * 只在用户显式选择时使用。
 */
export function resolveAutoEncoder(probe: LiveEncoderProbe | undefined): LiveEncoderId {
  if (!probe) return "x264";
  const available = (id: LiveEncoderId) => probe.encoders.some((encoder) => encoder.id === id && encoder.available);
  if (available("hevc_nvenc")) return "hevc_nvenc";
  if (available("nvenc")) return "nvenc";
  return "x264";
}

/** 取文件的父目录（兼容 Windows 反斜杠路径）；没有分隔符时返回空串。 */
export function parentDir(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const index = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return index > 0 ? trimmed.slice(0, index) : "";
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
