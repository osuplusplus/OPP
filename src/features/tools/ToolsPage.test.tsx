import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { desktopApi } from "../../shared/lib/tauri";
import type { BeatmapPreviewInspection } from "../../shared/types/osu";
import { BeatmapPreviewCard } from "./ToolsPage";

const standardInspection: BeatmapPreviewInspection = {
  bid: 123,
  title: "Preview Map",
  title_unicode: "Preview Map",
  artist: "Artist",
  artist_unicode: "Artist",
  creator: "Mapper",
  difficulty_name: "Insane",
  ruleset: "osu",
  length_ms: 60_000,
  strains: {
    first_object_time_ms: 1_000,
    section_length_ms: 400,
    series: [{ key: "aim", values: [1, 2, 3] }, { key: "speed", values: [2, 1, 4] }],
  },
};

function renderCard(entry = "/tools") {
  return render(<MemoryRouter initialEntries={[entry]}><BeatmapPreviewCard /></MemoryRouter>);
}

describe("BeatmapPreviewCard", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:preview");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
  });

  it("loads a linked BID and initializes a ten-second std range", async () => {
    const inspect = vi.spyOn(desktopApi, "inspectBeatmapPreview").mockResolvedValue(standardInspection);
    renderCard("/tools?preview_bid=123");

    await waitFor(() => expect(inspect).toHaveBeenCalledWith(123));
    expect(await screen.findByText("选择 GIF 区间")).toBeInTheDocument();
    expect(screen.getByLabelText("GIF 开始时间")).toHaveValue("0");
    expect(screen.getByLabelText("GIF 结束时间")).toHaveValue("10");

    fireEvent.change(screen.getByLabelText("GIF 结束时间"), { target: { value: "45" } });
    expect(screen.getByLabelText("GIF 结束时间")).toHaveValue("30");
  });

  it("moves the selected GIF window by dragging it directly on the strain chart", async () => {
    vi.spyOn(desktopApi, "inspectBeatmapPreview").mockResolvedValue(standardInspection);
    renderCard("/tools?preview_bid=123");

    const selectionLayer = await screen.findByLabelText("GIF 图表区间选择");
    const selection = screen.getByLabelText("拖动 GIF 选中区间");
    vi.spyOn(selectionLayer, "getBoundingClientRect").mockReturnValue({
      bottom: 120,
      height: 100,
      left: 0,
      right: 600,
      top: 20,
      width: 600,
      x: 0,
      y: 20,
      toJSON: () => ({}),
    });

    fireEvent.pointerDown(selection, { clientX: 50, pointerId: 1 });
    fireEvent.pointerMove(selectionLayer, { clientX: 150, pointerId: 1 });
    fireEvent.pointerUp(selectionLayer, { clientX: 150, pointerId: 1 });

    expect(screen.getByLabelText("GIF 开始时间")).toHaveValue("10");
    expect(screen.getByLabelText("GIF 结束时间")).toHaveValue("20");
  });

  it("generates and exposes a full PNG for non-std modes", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "inspectBeatmapPreview").mockResolvedValue({ ...standardInspection, ruleset: "mania", strains: null });
    const generate = vi.spyOn(desktopApi, "generateBeatmapPreview").mockResolvedValue({ output_path: "C:/temp/mania.png", file_name: "mania.png", mime_type: "image/png" });
    vi.spyOn(desktopApi, "readBeatmapPreviewOutput").mockResolvedValue(new ArrayBuffer(4));
    const choose = vi.spyOn(desktopApi, "chooseBeatmapPreviewDestination").mockResolvedValue("C:/share/mania.png");
    const save = vi.spyOn(desktopApi, "saveBeatmapPreviewOutput").mockResolvedValue("C:/share/mania.png");

    renderCard();
    await user.type(screen.getByLabelText("Beatmap ID"), "456");
    await user.click(screen.getByRole("button", { name: "读取谱面" }));
    expect(await screen.findByText("该模式将生成包含全部物件的 PNG 长图，无需选择区间。")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "生成 PNG" }));
    await waitFor(() => expect(generate).toHaveBeenCalledWith({ bid: 123, start_seconds: null, end_seconds: null }));
    expect(await screen.findByAltText("谱面预览生成结果")).toHaveAttribute("src", "blob:preview");

    await user.click(screen.getByRole("button", { name: "另存为" }));
    await waitFor(() => expect(choose).toHaveBeenCalledWith("mania.png", "png"));
    expect(save).toHaveBeenCalledWith("C:/temp/mania.png", "C:/share/mania.png");
  });
});
