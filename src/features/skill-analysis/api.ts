import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { OsuClient, SkillAnalysisRequest } from "../../shared/types/osu";

export const skillAnalysisQueryKey = (client: OsuClient) => ["skill-analysis", client] as const;

export function useSkillAnalysis(client: OsuClient) {
  const queryClient = useQueryClient();
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<unknown>(null);
  const request = (forceRefresh = false): SkillAnalysisRequest => ({
    ruleset: "osu",
    client,
    score_limit: 200,
    include_online: true,
    force_refresh: forceRefresh,
  });
  const query = useQuery({
    queryKey: skillAnalysisQueryKey(client),
    queryFn: () => desktopApi.analyzePlayerSkills(request()),
    staleTime: 10 * 60_000,
    retry: false,
  });
  const refresh = async () => {
    setIsRefreshing(true);
    setRefreshError(null);
    try {
      const result = await desktopApi.analyzePlayerSkills(request(true));
      queryClient.setQueryData(skillAnalysisQueryKey(client), result);
      return result;
    } catch (error) {
      setRefreshError(error);
      throw error;
    } finally {
      setIsRefreshing(false);
    }
  };
  return { ...query, refresh, isRefreshing, refreshError };
}
