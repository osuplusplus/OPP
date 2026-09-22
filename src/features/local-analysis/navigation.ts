import type { MusicLocation } from "../../shared/types/music";
import type { OsuClient, Ruleset } from "../../shared/types/osu";

export function explicitLocalTarget(params: URLSearchParams): MusicLocation | null {
  const client = params.get("client"), ruleset = params.get("ruleset"), resource = params.get("resource"), set = params.get("set");
  if (!resource || !set || !["stable", "lazer"].includes(client ?? "") || !["osu", "taiko", "fruits", "mania"].includes(ruleset ?? "")) return null;
  return { client: client as OsuClient, ruleset: ruleset as Ruleset, resource_id: resource, set_key: set };
}
