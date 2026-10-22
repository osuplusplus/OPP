import type { Cached, OwnProfile, Ruleset, Score, ScoreCategory, SkillAnalysisRequest, SkillAnalysisResult } from "../../types/osu";
import { call } from "./invoke";

// profile 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const profileApi = {
  getOwnProfile: (ruleset: Ruleset, forceRefresh = false) =>
    call<Cached<OwnProfile>>("get_own_profile", {
      ruleset,
      forceRefresh,
    }),

  getOsekaiMedals: () => call<{ content?: unknown[] }>("get_osekai_medals"),

  getOsekaiMedalDetail: (medalId: number) => call<{ content?: unknown[] }>("get_osekai_medal_detail", { medalId }),

  getOsekaiMedalBeatmaps: (medalId: number) => call<{ content?: unknown[] }>("get_osekai_medal_beatmaps", { medalId }),

  getScores: (ruleset: Ruleset, category: ScoreCategory, offset = 0, limit = 100, forceRefresh = false) =>
    call<Cached<Score[]>>("get_scores", {
      ruleset,
      category,
      offset,
      limit,
      forceRefresh,
    }),

  analyzePlayerSkills: (request: SkillAnalysisRequest) =>
    call<SkillAnalysisResult>("analyze_player_skills", { request }),
};
