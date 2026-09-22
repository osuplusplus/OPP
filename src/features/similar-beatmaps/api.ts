import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
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
  const client = useQueryClient();
  const query = useQuery({
    queryKey: similarityIndexStatusKey(ruleset),
    queryFn: () => desktopApi.getSimilarityIndexStatus(ruleset),
    staleTime: 30_000,
    retry: false,
  });
  const revalidate = useMutation({
    mutationFn: () => desktopApi.configureSimilarityIndex(ruleset, query.data?.directory ?? null),
    onSuccess: (value) => client.setQueryData(similarityIndexStatusKey(ruleset), value),
  });
  return { ...query, isFetching: query.isFetching || revalidate.isPending,
    revalidate: () => revalidate.mutate() };
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
