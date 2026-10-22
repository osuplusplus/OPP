import type { Ruleset, SimilarityIndexStatus, SimilarityQueryRequest, SimilarityQueryResponse, SimilarityRecommendationRequest, SimilarityRecommendationResponse } from "../../types/osu";
import { call } from "./invoke";

// similarity 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const similarityApi = {
  getSimilarityIndexStatus: (ruleset: Ruleset) =>
    call<SimilarityIndexStatus>("get_similarity_index_status", { ruleset }),

  configureSimilarityIndex: (ruleset: Ruleset, directory: string | null) =>
    call<SimilarityIndexStatus>("configure_similarity_index", { ruleset, directory }),

  querySimilarBeatmaps: (request: SimilarityQueryRequest) =>
    call<SimilarityQueryResponse>("query_similar_beatmaps", { request }),

  recommendSimilarBeatmaps: (request: SimilarityRecommendationRequest) =>
    call<SimilarityRecommendationResponse>("recommend_similar_beatmaps", { request }),
};
