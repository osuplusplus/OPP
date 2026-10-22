const OSU_BASE_URL = "https://osu.ppy.sh";

/** osu! 网站链接统一构造，避免散落在各组件里的手拼 URL。 */

export function osuUserUrl(userId: number | string): string {
  return `${OSU_BASE_URL}/users/${userId}`;
}

export function osuBeatmapUrl(beatmapId: number | string): string {
  return `${OSU_BASE_URL}/beatmaps/${beatmapId}`;
}

export function osuBeatmapsetUrl(beatmapsetId: number | string): string {
  return `${OSU_BASE_URL}/beatmapsets/${beatmapsetId}`;
}

/** 难度深链：谱面集页面 + `#<mode>/<难度ID>` 锚点。 */
export function osuBeatmapDifficultyUrl(
  beatmapsetId: number | string,
  mode: string,
  beatmapId: number | string,
): string {
  return `${osuBeatmapsetUrl(beatmapsetId)}#${mode}/${beatmapId}`;
}

export function osuScoreUrl(scoreId: number | string): string {
  return `${OSU_BASE_URL}/scores/${scoreId}`;
}

/** 把站内相对路径（如 `/beatmapsets/123`）拼接为完整地址。 */
export function absoluteOsuUrl(path: string): string {
  return `${OSU_BASE_URL}${path}`;
}
