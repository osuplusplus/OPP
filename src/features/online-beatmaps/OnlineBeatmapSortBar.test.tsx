import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { OnlineBeatmapSortBar } from "./OnlineBeatmapSortBar";

describe("OnlineBeatmapSortBar", () => {
  it("selects a sort field with its natural default direction", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<OnlineBeatmapSortBar onChange={onChange} sort="ranked_desc" />);

    const toolbar = screen.getByRole("toolbar", { name: "谱面排序方式" });
    expect(toolbar).toHaveClass("min-h-12", "rounded-[11px]");
    expect(toolbar.children[1]).toHaveClass("flex-nowrap", "overflow-x-auto");
    expect(screen.getByRole("button", { name: "按标题排序" }).firstElementChild).toHaveClass("text-[11px]", "leading-4");

    await user.click(screen.getByRole("button", { name: "按标题排序" }));
    expect(onChange).toHaveBeenCalledWith("title_asc");

    await user.click(screen.getByRole("button", { name: "按难度排序" }));
    expect(onChange).toHaveBeenCalledWith("difficulty_desc");
  });

  it("toggles direction when the active field is clicked again", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<OnlineBeatmapSortBar onChange={onChange} sort="ranked_desc" />);

    await user.click(screen.getByRole("button", { name: "上架时间，当前降序，点击切换" }));
    expect(onChange).toHaveBeenCalledWith("ranked_asc");
  });
});
