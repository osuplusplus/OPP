import type { LiveRenderOptions } from "../../shared/lib/tauri";
import { defaultLiveRenderOptions } from "./model";

const KEY = "opp:replay-live-options.v1";

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const boolOr = (value: unknown, fallback: boolean) => (typeof value === "boolean" ? value : fallback);
const numberOr = (value: unknown, fallback: number) =>
  typeof value === "number" && Number.isFinite(value) ? value : fallback;
const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/** 脏数据防御:旧版本缺少字段/被手工改坏时退回默认值。 */
export function normalizeLiveRenderOptions(input: unknown): LiveRenderOptions {
  const value = isObject(input) ? input : {};
  return {
    hud: boolOr(value.hud, defaultLiveRenderOptions.hud),
    storyboard: boolOr(value.storyboard, defaultLiveRenderOptions.storyboard),
    video: boolOr(value.video, defaultLiveRenderOptions.video),
    urBar: boolOr(value.urBar, defaultLiveRenderOptions.urBar),
    followPoints: boolOr(value.followPoints, defaultLiveRenderOptions.followPoints),
    keyOverlay: boolOr(value.keyOverlay, defaultLiveRenderOptions.keyOverlay),
    ppDisplay: boolOr(value.ppDisplay, defaultLiveRenderOptions.ppDisplay),
    bg: boolOr(value.bg, defaultLiveRenderOptions.bg),
    bgOpacity: clamp(numberOr(value.bgOpacity, defaultLiveRenderOptions.bgOpacity), 0, 1),
    audio: boolOr(value.audio, defaultLiveRenderOptions.audio),
    audioOffset: numberOr(value.audioOffset, defaultLiveRenderOptions.audioOffset),
    hitsounds: boolOr(value.hitsounds, defaultLiveRenderOptions.hitsounds),
    cursorSize: clamp(numberOr(value.cursorSize, defaultLiveRenderOptions.cursorSize), 0.1, 2),
    skinPath: typeof value.skinPath === "string" ? value.skinPath : null,
    skinColours: boolOr(value.skinColours, defaultLiveRenderOptions.skinColours),
    // 头像路径是每次打开预览时按账号解析的临时文件,不跨启动复用。
    avatarPath: null,
    offsetHeatmap: boolOr(value.offsetHeatmap, defaultLiveRenderOptions.offsetHeatmap),
    urGuides: boolOr(value.urGuides, defaultLiveRenderOptions.urGuides),
    showCursor: boolOr(value.showCursor, defaultLiveRenderOptions.showCursor),
    showCursorTrail: boolOr(value.showCursorTrail, defaultLiveRenderOptions.showCursorTrail),
    showCursorRipples: boolOr(value.showCursorRipples, defaultLiveRenderOptions.showCursorRipples),
    snakingIn: boolOr(value.snakingIn, defaultLiveRenderOptions.snakingIn),
    snakingOut: boolOr(value.snakingOut, defaultLiveRenderOptions.snakingOut),
    hitAnimations: boolOr(value.hitAnimations, defaultLiveRenderOptions.hitAnimations),
    breakLighten: boolOr(value.breakLighten, defaultLiveRenderOptions.breakLighten),
    argonHud: boolOr(value.argonHud, defaultLiveRenderOptions.argonHud),
    classicScore: boolOr(value.classicScore, defaultLiveRenderOptions.classicScore),
    hd: value.hd === "on" || value.hd === "off" ? value.hd : "auto",
    resultsStyle: value.resultsStyle === "stable" ? "stable" : "lazer",
  };
}

export function loadLiveRenderOptions(client: string): LiveRenderOptions {
  try {
    const raw = window.localStorage.getItem(KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    return normalizeLiveRenderOptions(isObject(parsed) ? parsed[client] : null);
  } catch {
    return normalizeLiveRenderOptions(null);
  }
}

export function saveLiveRenderOptions(client: string, options: LiveRenderOptions) {
  try {
    const raw = window.localStorage.getItem(KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    const all = isObject(parsed) ? parsed : {};
    all[client] = { ...options, avatarPath: null };
    window.localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    // 隐私模式/配额满:记住设置失败不影响预览。
  }
}
