import { useMutation, useQuery } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  SimilarityQueryRequest,
  SimilarityRecommendationRequest,
  SimilarityRuleset,
} from "../../shared/types/osu";

export function similarityIndexStatusKey(ruleset: SimilarityRuleset) {
  return ["similarity-index-status", ruleset] as const;
}

export function similarityRecommendationKey(request: SimilarityRecommendationRequest) {
  return ["similarity-recommendation", request] as const;
}

export function useSimilarityIndexStatus(ruleset: SimilarityRuleset) {
  return useQuery({
    queryKey: similarityIndexStatusKey(ruleset),
    queryFn: () => desktopApi.getSimilarityIndexStatus(ruleset),
    staleTime: 30_000,
    retry: false,
  });
}

export function useSimilarityQuery(ruleset: SimilarityRuleset) {
  return useMutation({
    mutationKey: ["similarity-query", ruleset],
    mutationFn: (request: SimilarityQueryRequest) =>
      desktopApi.querySimilarBeatmaps(request),
  });
}

export function useSimilarityRecommendation(ruleset: SimilarityRuleset) {
  return useMutation({
    mutationKey: ["similarity-recommendation", ruleset],
    mutationFn: (request: SimilarityRecommendationRequest) =>
      desktopApi.recommendSimilarBeatmaps(request),
  });
}
