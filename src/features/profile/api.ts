import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { Ruleset } from "../../shared/types/osu";
import type { CareerCalendar, CareerDayDetail, CareerStatus } from "../../shared/types/osu";

export const profileQueryKey = (ruleset: Ruleset) =>
  ["own-profile", ruleset] as const;

export function useOwnProfile(ruleset: Ruleset) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: profileQueryKey(ruleset),
    queryFn: () => desktopApi.getOwnProfile(ruleset),
    staleTime: 5 * 60_000,
    retry: false,
  });

  const refresh = async () => {
    const refreshed = await desktopApi.getOwnProfile(ruleset, true);
    queryClient.setQueryData(profileQueryKey(ruleset), refreshed);
    return refreshed;
  };

  return { ...query, refresh };
}

export const careerCalendarKey = (ruleset: Ruleset, start: string, end: string) =>
  ["career-calendar", ruleset, start, end] as const;

export function useCareerCalendar(ruleset: Ruleset, start: string, end: string) {
  return useQuery<CareerCalendar>({
    queryKey: careerCalendarKey(ruleset, start, end),
    queryFn: () => desktopApi.getCareerCalendar(ruleset, start, end),
    staleTime: 30_000,
  });
}

export function useCareerDay(ruleset: Ruleset, date: string | null) {
  return useQuery<CareerDayDetail>({
    queryKey: ["career-day", ruleset, date],
    queryFn: () => desktopApi.getCareerDay(ruleset, date!),
    enabled: Boolean(date),
    staleTime: 30_000,
  });
}

export function useCareerStatus() {
  return useQuery<CareerStatus>({
    queryKey: ["career-status"],
    queryFn: desktopApi.getCareerStatus,
    staleTime: 30_000,
  });
}

export function useCareerActions(ruleset: Ruleset) {
  const queryClient = useQueryClient();
  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["career-calendar"] }),
      queryClient.invalidateQueries({ queryKey: ["career-day"] }),
      queryClient.invalidateQueries({ queryKey: ["career-status"] }),
    ]);
  };
  const capture = useMutation({
    mutationFn: () => desktopApi.captureCareerSnapshot(ruleset, true),
    onSettled: refresh,
  });
  const clear = useMutation({
    mutationFn: () => desktopApi.clearCareerHistory(),
    onSuccess: refresh,
  });
  return { capture, clear };
}
