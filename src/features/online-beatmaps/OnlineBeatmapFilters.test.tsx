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
    expect(screen.getByPlaceholderText("输入关键字...")).toBeVisible();
    expect(screen.getByRole("button", { name: "隐藏" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "显示" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByRole("button", { name: "有视频" })).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "流派筛选" })).not.toBeInTheDocument();
    const qualified = screen.getByRole("button", { name: "过审 (Qualified)" });
    expect(qualified).toHaveClass("shrink-0", "whitespace-nowrap");
    expect(qualified.firstElementChild).toHaveClass("text-[clamp(11px,calc(8px+0.3vw),14px)]", "leading-[1.25]");
    expect(screen.getByText("分类")).toHaveClass("text-[clamp(11px,calc(8px+0.3vw),14px)]", "whitespace-nowrap");

    await user.click(moreFilters);
    const collapseFilters = screen.getByRole("button", { name: "收起筛选" });
    expect(collapseFilters.querySelector("svg")).toHaveClass("lucide-chevron-up");
    expect(container.querySelector("[data-page-guide-online-advanced]")?.lastElementChild).toBe(collapseFilters);
    expect(screen.queryByText(/离散筛选|文本与日期|数值范围/)).not.toBeInTheDocument();
    expect(screen.getByRole("group", { name: "流派筛选" })).toBeVisible();
    expect(screen.getByRole("group", { name: "语言筛选" })).toBeVisible();
    expect(screen.getByRole("group", { name: "其他筛选" })).toBeVisible();
    expect(screen.getByRole("group", { name: "玩过筛选" })).toBeVisible();
    expect(screen.getByRole("button", { name: "电子游戏" })).toBeVisible();
    expect(screen.getByRole("button", { name: "汉语" })).toBeVisible();
    expect(screen.getByRole("button", { name: "有视频" })).toBeVisible();
    expect(screen.getAllByRole("group", { name: "常规筛选" })).toHaveLength(1);
    expect(screen.getAllByRole("group", { name: "不良内容筛选" })).toHaveLength(1);
    const advancedDiscrete = container.querySelector("[data-online-advanced-discrete]");
    expect(advancedDiscrete).toHaveClass("border-b");
    expect(advancedDiscrete).not.toHaveClass("border-t", "border-y");
    expect(screen.getByRole("group", { name: "常规筛选" }).parentElement).toHaveClass("min-h-10", "py-1");
    expect(screen.getByLabelText("艺术家")).toHaveClass("opp-filter-compact-input");
    expect(screen.getAllByPlaceholderText("最小")[0]).toHaveClass("opp-filter-compact-input");

    await user.click(screen.getByRole("button", { name: "社区喜爱 (Loved)" }));
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
