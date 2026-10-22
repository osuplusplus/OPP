import type {
  OsuClient,
  ViewTrainerRequest,
  ViewTrainerTimeline,
} from "../../shared/types/osu";

// view-trainer 的纯逻辑层：草稿状态、AR/OD 变换、时间窗口与请求构造。
// 页面只负责状态编排与 UI，所有可测试的换算集中在这里。

export type ViewTrainerDraft = {
  rate: number;
  bpm_locked: boolean;
  target_bpm: number | null;
  scale_ar: boolean;
  scale_od: boolean;
  lock_ar: boolean;
  lock_od: boolean;
  lock_cs: boolean;
  lock_hp: boolean;
  ar: number;
  od: number;
  cs: number;
  hp: number;
  min_bpm: number | null;
  max_bpm: number | null;
  start_time_ms: number | null;
  end_time_ms: number | null;
  no_spinners: boolean;
  change_pitch: boolean;
  window_ms: number;
};

export const DEFAULT_DRAFT: ViewTrainerDraft = {
  rate: 1,
  bpm_locked: false,
  target_bpm: null,
  scale_ar: true,
  scale_od: true,
  lock_ar: false,
  lock_od: false,
  lock_cs: false,
  lock_hp: false,
  ar: 5,
  od: 5,
  cs: 4,
  hp: 5,
  min_bpm: null,
  max_bpm: null,
  start_time_ms: null,
  end_time_ms: null,
  no_spinners: false,
  change_pitch: false,
  window_ms: 30_000,
};

export const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

export const formatTime = (ms: number) => {
  const total = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
};

const arToMs = (ar: number) =>
  ar <= 5 ? 1800 - ar * 120 : 1200 - (ar - 5) * 150;

export const scaledAr = (ar: number, rate: number) =>
  clamp(
    arToMs(ar) / rate >= 1200
      ? (1800 - arToMs(ar) / rate) / 120
      : 5 + (1200 - arToMs(ar) / rate) / 150,
    0,
    11,
  );

export const scaledOd = (od: number, rate: number) =>
  Math.round(clamp((79.5 - (-6 * od + 79.5) / rate) / 6, 0, 11) * 10) / 10;

export function numberValue(value: string, fallback: number | null) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

/** 按锁/倍率规则推导实际生效的 rate 与四维：锁定用草稿值，未锁定跟随原谱并按 rate 缩放。 */
export function resolveEffectiveValues(
  draft: ViewTrainerDraft,
  timeline: ViewTrainerTimeline | null,
) {
  const rate =
    draft.bpm_locked && draft.target_bpm && timeline?.primaryBpm
      ? draft.target_bpm / timeline.primaryBpm
      : draft.rate;
  const ar =
    !draft.lock_ar &&
    draft.scale_ar &&
    timeline &&
    timeline.mode !== 1 &&
    timeline.mode !== 3
      ? scaledAr(timeline.ar, rate)
      : draft.ar;
  const od =
    !draft.lock_od && draft.scale_od && timeline
      ? scaledOd(timeline.od, rate)
      : draft.od;
  const cs = draft.lock_cs ? draft.cs : (timeline?.cs ?? draft.cs);
  const hp = draft.lock_hp ? draft.hp : (timeline?.hp ?? draft.hp);
  return { rate, ar, od, cs, hp };
}

/** 训练窗口：时长 5 秒到谱面全长，起点 clamp 到 [0, mapDuration - windowLength]。 */
export function resolveWindow(draft: ViewTrainerDraft, mapDuration: number) {
  const windowLength = clamp(
    draft.window_ms ?? 30_000,
    5_000,
    Math.max(5_000, mapDuration),
  );
  const start = clamp(
    draft.start_time_ms ?? 0,
    0,
    Math.max(0, mapDuration - windowLength),
  );
  const end = Math.min(mapDuration, start + windowLength);
  return { windowLength, start, end };
}

export function buildViewTrainerRequest(input: {
  client: OsuClient;
  resourceId: string;
  draft: ViewTrainerDraft;
  effective: { rate: number; ar: number; od: number; cs: number; hp: number };
  start: number;
  end: number;
  mapDuration: number;
}): ViewTrainerRequest {
  const { client, resourceId, draft, effective, start, end, mapDuration } = input;
  return {
    client,
    resourceId,
    rate: effective.rate,
    bpmLocked: draft.bpm_locked,
    targetBpm: draft.target_bpm,
    ar: effective.ar,
    od: effective.od,
    cs: effective.cs,
    hp: effective.hp,
    scaleAr: draft.scale_ar,
    scaleOd: draft.scale_od,
    lockAr: draft.lock_ar,
    lockOd: draft.lock_od,
    lockCs: draft.lock_cs,
    lockHp: draft.lock_hp,
    noSpinners: draft.no_spinners,
    changePitch: draft.change_pitch,
    previewOnly: false,
    minBpm: draft.min_bpm,
    maxBpm: draft.max_bpm,
    startTimeMs: start || null,
    endTimeMs: end >= mapDuration - 1 ? null : end,
  };
}
