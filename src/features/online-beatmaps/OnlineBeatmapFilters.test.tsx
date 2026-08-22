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

    const { container } = render(
      <OnlineBeatmapFilters
        loading={false}
        onChange={onChange}
        onReset={vi.fn()}
        onSubmit={onSubmit}
        query={query}
      />,
    );

    const moreFilters = screen.getByRole("button", { name: /更多筛选/ });
    expect(moreFilters.querySelector("svg")).toHaveClass("lucide-chevron-down");
    expect(screen.getByRole("group", { name: "常规筛选" })).toBeVisible();
    expect(screen.getByRole("group", { name: "模式筛选" })).toBeVisible();
    expect(screen.getByRole("group", { name: "分类筛选" })).toHaveClass("flex-nowrap", "overflow-x-auto");
    expect(screen.getByRole("group", { name: "不良内容筛选" })).toBeVisible();
    const coreFilterLabels = Array.from(container.querySelectorAll("[data-page-guide-online-core-filters] [role='group']"), (element) => element.getAttribute("aria-label"));
    expect(coreFilterLabels).toEqual(["常规筛选", "模式筛选", "分类筛选", "不良内容筛选"]);
    expect(screen.queryByRole("group", { name: "流派筛选" })).not.toBeInTheDocument();
    const qualified = screen.getByRole("button", { name: "Qualified" });
    expect(qualified).toHaveClass("shrink-0", "whitespace-nowrap");
    expect(qualified.firstElementChild).toHaveClass("text-[11px]", "leading-4");

    await user.click(moreFilters);
    const collapseFilters = screen.getByRole("button", { name: "收起筛选" });
    expect(collapseFilters.querySelector("svg")).toHaveClass("lucide-chevron-up");
    expect(container.querySelector("[data-page-guide-online-advanced]")?.lastElementChild).toBe(collapseFilters);
    expect(screen.queryByText(/离散筛选|文本与日期|数值范围/)).not.toBeInTheDocument();
    expect(screen.getByRole("group", { name: "流派筛选" })).toBeVisible();
    expect(screen.getAllByRole("group", { name: "常规筛选" })).toHaveLength(1);
    expect(screen.getAllByRole("group", { name: "不良内容筛选" })).toHaveLength(1);

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

    await user.click(screen.getByRole("button", { name: /更多筛选/ }));
    const minimumStars = screen.getAllByPlaceholderText("最小")[0];
    fireEvent.change(minimumStars, { target: { value: "4.5" } });
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ stars_min: 4.5 }));
    expect(onSubmit).not.toHaveBeenCalled();

    minimumStars.focus();
    await user.keyboard("{Enter}");
    expect(onSubmit).toHaveBeenCalledOnce();
  });
});
