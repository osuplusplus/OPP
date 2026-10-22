import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { Ruleset, ScoreCategory } from "../../shared/types/osu";
import { desktopApi } from "../../shared/lib/tauri";

export const scoresQueryKey = (
  ruleset: Ruleset,
  category: ScoreCategory,
  offset: number,
  limit: number,
) => ["scores", ruleset, category, offset, limit] as const;

export function useScores(
  ruleset: Ruleset,
  category: ScoreCategory,
  offset: number,
  limit: number,
  enabled: boolean,
) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: scoresQueryKey(ruleset, category, offset, limit),
    queryFn: () => desktopApi.getScores(ruleset, category, offset, limit),
    enabled,
    staleTime: 10 * 60_000,
  });

  const refresh = async () => {
    const refreshed = await desktopApi.getScores(ruleset, category, offset, limit, true);
    queryClient.setQueryData(scoresQueryKey(ruleset, category, offset, limit), refreshed);
    return refreshed;
  };

  return { ...query, refresh };
}

export function useBestScores(ruleset: Ruleset, enabled: boolean) {
  return useScores(ruleset, "best", 0, 100, enabled);
}
