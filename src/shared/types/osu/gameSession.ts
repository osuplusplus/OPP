// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { OsuClient, Ruleset } from "./common";

export interface DefaultFileClients {
  beatmap: OsuClient;
  skin: OsuClient;
}

export interface OtdConfigSummary {
  output_mode: string | null;
  area: string | null;
  filter_count: number;
}

export interface OtdStatus {
  installed: boolean;
  executable_path: string | null;
  daemon_running: boolean;
  owned_by_opp: boolean;
  version: string | null;
  tablet_name: string | null;
  config_path: string | null;
  config_summary: OtdConfigSummary | null;
  last_error: string | null;
}

export interface UserSnapshot {
  captured_at: string;
  username: string;
  pp: number | null;
  ranked_score: number | null;
  hit_accuracy: number | null;
  total_hits: number | null;
  total_score: number | null;
}

export interface GameSessionSummary {
  started_at: string;
  ended_at: string | null;
  ruleset: Ruleset;
  client: string;
  executable: string;
  start: UserSnapshot;
  end: UserSnapshot | null;
  running: boolean;
}

export interface GameClientStatus {
  client: OsuClient;
  running: boolean;
  executable: string | null;
  detected_at: string;
}

export interface GameStatusSnapshot { clients: GameClientStatus[]; }

export interface ManiaConversionItem {
  input: string;
  status: "completed" | "skipped" | "failed";
  output: string | null;
  message: string | null;
}

export interface ManiaConversionResult { items: ManiaConversionItem[]; }
