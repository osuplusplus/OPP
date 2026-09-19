import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ManiaDifficultyVector, ManiaPatternView } from "../../shared/types/osu";
import { MmaPatternPanel } from "./MmaPatternPanel";
import { MmaCoverageTooltip, SimilarityRadar } from "./SimilarityRadar";
import { mmaRadarValues } from "./mmaRadar";

const view = {
  category: "Shield",
  mode_tag: "Mix",
  coverage: [0.52, 0.31, 0.12, 0.66, 0.48, 0.09],
  bars: [
    { pattern: "Coordination", amount: 158_000, relative: 0.66, specific_types: [["Shield", 0.092], ["Chordjack", 0.041]] },
    { pattern: "Wildcard", amount: 239_000, relative: 1, specific_types: [] },
  ],
  subtypes: [["Shield", 0.092]],
  ln_note_ratio: 0.234,
  intensity: [6.4, 12.1, 5.2, 18.5],
  temporal: [0.42, 0.18, 0.12],
  duration_seconds: 120,
  sv_amount: 0,
} satisfies ManiaPatternView;

const difficulty: ManiaDifficultyVector = { speed: 1, hand_stream: 1, jack: 1, chordjack: 1, technical: 1, stamina: 1, long_note: 1, course: 1 };

describe("MMA native presentation", () => {
  it("keeps the analyser classification independent of the longest bar", () => {
    render(<MmaPatternPanel view={view} />);
    expect(screen.getByText("Mix · Shield")).toBeInTheDocument();
    expect(screen.getByText("RC 76.6%")).toBeInTheDocument();
    expect(screen.getByText("LN 23.4%")).toBeInTheDocument();
    expect(screen.getByText("79.2s")).toBeInTheDocument();
    expect(screen.getByText("Shield (9.2%), Chordjack (4.1%)")).toBeInTheDocument();
    expect(screen.getByRole("meter", { name: "Wildcard 相对量" })).toHaveAttribute("aria-valuenow", "100");
    expect(screen.getByRole("meter", { name: "Coordination 相对量" })).toHaveAttribute("aria-valuenow", "66");
    expect(screen.queryByText(/分类依据|相对键型量|模式可以重叠|难度/)).not.toBeInTheDocument();
  });

  it("renders the stable empty state instead of an empty bar list", () => {
    render(<MmaPatternPanel view={{ ...view, bars: [] }} />);
    expect(screen.getByText("未识别到稳定模式")).toBeInTheDocument();
  });

  it("uses the six-axis coverage radar when key-pattern data exists and keeps the legacy radar otherwise", () => {
    const { rerender } = render(<SimilarityRadar patternView={view} patternViewComparison={view} target={difficulty} />);
    expect(screen.getByRole("img", { name: "MMA 六维覆盖率雷达图" })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "Mania 八维难度雷达图" })).not.toBeInTheDocument();
    rerender(<SimilarityRadar target={difficulty} />);
    expect(screen.getByRole("img", { name: "Mania 八维难度雷达图" })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "MMA 六维覆盖率雷达图" })).not.toBeInTheDocument();
  });

  it("describes each axis with real coverage, covered seconds and main subtypes", () => {
    const values = mmaRadarValues(view);
    const coordination = values.findIndex((value) => value.dimension === "Coordination");
    render(<MmaCoverageTooltip label="Coordination" series={[{ name: "参考谱面", value: values[coordination] }, { name: "候选谱面", value: values[0] }]} />);
    expect(screen.getByText("Coordination")).toBeInTheDocument();
    expect(screen.getByText("参考谱面 · 66.0% · 79.2s")).toBeInTheDocument();
    expect(screen.getByText("Shield · Chordjack")).toBeInTheDocument();
    expect(screen.getByText("候选谱面 · 52.0% · 62.4s")).toBeInTheDocument();
  });
});
