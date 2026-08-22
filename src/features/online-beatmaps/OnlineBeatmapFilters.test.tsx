import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { createDefaultSearchQuery } from "./filters";
import { OnlineBeatmapFilters } from "./OnlineBeatmapFilters";

describe("OnlineBeatmapFilters", () => {
  it("uses down/up disclosure affordance and applies discrete filters immediately", async () => {
    const user = userEvent.setup();
    const query = createDefaultSearchQuery("osu");
    const onChange = vi.fn();
    const onSubmit = vi.fn();

    render(
      <OnlineBeatmapFilters
        loading={false}
        onChange={onChange}
        onReset={vi.fn()}
        onSubmit={onSubmit}
        query={query}
      />,
    );

    const summary = screen.getByText(/更多筛选/).closest("summary");
    const arrow = summary?.querySelector("svg");
    expect(arrow).toHaveClass("lucide-chevron-down", "group-open:rotate-180");
    expect(screen.getByRole("group", { name: "状态筛选" })).toHaveClass("flex-nowrap", "overflow-x-auto");
    const qualified = screen.getByRole("button", { name: "Qualified" });
    expect(qualified).toHaveClass("shrink-0", "whitespace-nowrap");
    expect(qualified.firstElementChild).toHaveClass("text-[11px]", "leading-4");

    await user.click(summary!);
    expect(summary?.parentElement).toHaveAttribute("open");

    await user.click(screen.getByRole("button", { name: "Loved" }));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ status: "loved" }));
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ status: "loved" }));
  });

  it("keeps numeric ranges pending until the form is submitted", async () => {
    const user = userEvent.setup();
    const query = createDefaultSearchQuery("osu");
    const onChange = vi.fn();
    const onSubmit = vi.fn();

    render(
      <OnlineBeatmapFilters
        loading={false}
        onChange={onChange}
        onReset={vi.fn()}
        onSubmit={onSubmit}
        query={query}
      />,
    );

    await user.click(screen.getByText(/更多筛选/).closest("summary")!);
    const minimumStars = screen.getAllByPlaceholderText("最小")[0];
    fireEvent.change(minimumStars, { target: { value: "4.5" } });
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ stars_min: 4.5 }));
    expect(onSubmit).not.toHaveBeenCalled();

    minimumStars.focus();
    await user.keyboard("{Enter}");
    expect(onSubmit).toHaveBeenCalledOnce();
  });
});
