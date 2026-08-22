import type {
  SimilarityBaseFeatures,
  SimilarityFilters,
  SimilarityIndexStatus,
  OsuSimilarityQueryRequest,
  SimilarityPreferences,
} from "../../shared/types/osu";
import { manualWeightingFromPreferences } from "./defaults";
import { APP_TIME_ZONE } from "../../shared/lib/format";

export const similarityIndexStateCopy: Record<
  Exclude<SimilarityIndexStatus["state"], "ready">,
  { title: string; description: string }
> = {
  unconfigured: {
    title: "本地索引未配置",
    description: "请选择一个兼容的本地索引目录。相似谱面功能只会在本机以只读方式使用该目录。",
  },
  missing: {
    title: "本地索引目录不可用",
    description: "此前选择的目录已移动或删除，请重新选择目录或重新校验。",
  },
  invalid: {
    title: "本地索引校验失败",
    description: "目录中的必要文件缺失、校验值不一致，或索引内容已损坏。请查看索引说明并取得与当前运行时匹配的版本，或重新校验。",
  },
  incompatible: {
    title: "本地索引版本不兼容",
    description: "该索引使用的分析器、归一化器或索引格式与当前版本不兼容。",
  },
  unsupported: {
    title: "当前模式暂不支持相似谱面",
    description: "请切换到 osu!standard 或 osu!mania 后再使用相似谱面。",
  },
};

export function matchesCandidateFilters(
  base: SimilarityBaseFeatures,
  starRating: number | null,
  filters: SimilarityFilters,
) {
  return (
    (filters.min_star == null || (starRating != null && starRating >= filters.min_star)) &&
    (filters.max_star == null || (starRating != null && starRating <= filters.max_star)) &&
    (filters.min_ar == null || base.ar >= filters.min_ar) &&
    (filters.max_ar == null || base.ar <= filters.max_ar) &&
    (filters.min_cs == null || base.cs >= filters.min_cs) &&
    (filters.max_cs == null || base.cs <= filters.max_cs) &&
    (filters.min_od == null || base.od >= filters.min_od) &&
    (filters.max_od == null || base.od <= filters.max_od) &&
    (filters.min_bpm == null || base.bpm >= filters.min_bpm) &&
    (filters.max_bpm == null || base.bpm <= filters.max_bpm) &&
    (filters.min_length_seconds == null || base.length_seconds >= filters.min_length_seconds) &&
    (filters.max_length_seconds == null || base.length_seconds <= filters.max_length_seconds) &&
    (filters.min_object_density == null || base.object_density >= filters.min_object_density) &&
    (filters.max_object_density == null || base.object_density <= filters.max_object_density) &&
    (filters.min_circle_ratio == null || base.circle_ratio >= filters.min_circle_ratio) &&
    (filters.max_circle_ratio == null || base.circle_ratio <= filters.max_circle_ratio) &&
    (filters.min_slider_ratio == null || base.slider_ratio >= filters.min_slider_ratio) &&
    (filters.max_slider_ratio == null || base.slider_ratio <= filters.max_slider_ratio)
  );
}

export function resolveSimilarityWeighting(
  request: OsuSimilarityQueryRequest,
  preferences: SimilarityPreferences,
) {
  if (preferences.advanced_enabled && preferences.mode === "manual" && request.weighting.mode === "manual") {
    return request.weighting;
  }
  // Also migrates old saved dynamic preferences without resetting the user settings.
  return manualWeightingFromPreferences(preferences);
}

export function formatSimilarityMetric(value: number | null, digits = 2) {
  return value == null ? "—" : value.toFixed(digits);
}

export function formatDataCutoff(value: number | null) {
  if (value == null) return "索引未声明数据截止时间";
  return new Intl.DateTimeFormat("zh-CN", {
    dateStyle: "long",
    timeStyle: "short",
    timeZone: APP_TIME_ZONE,
  }).format(new Date(value * 1000));
}
