import { fireEvent, render, screen } from "@testing-library/react";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

import { Button, Input, Tooltip } from "./ui";

beforeAll(() => {
  // jsdom 不提供 ResizeObserver，Radix Tooltip 定位时需要一个最小实现。
  vi.stubGlobal("ResizeObserver", class {
    observe() {}
    unobserve() {}
    disconnect() {}
  });
});

afterAll(() => vi.unstubAllGlobals());

describe("公共 UI 组件", () => {
  it("保留 Button 的加载与禁用语义", () => {
    render(<Button loading>保存</Button>);

    expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
  });

  it("Input 复用现有控件样式并透传事件", () => {
    const onInput = vi.fn();
    render(
      <Input aria-label="名称" className="custom-input" onChange={onInput} />,
    );

    const input = screen.getByRole("textbox", { name: "名称" });
    expect(input).toHaveClass("opp-input", "custom-input");
    fireEvent.change(input, { target: { value: "Daisuke" } });
    expect(onInput).toHaveBeenCalledOnce();
  });

  it("Tooltip 使用 Radix 展示提示内容", async () => {
    render(<Tooltip content="补充说明"><button type="button">详情</button></Tooltip>);

    fireEvent.pointerMove(screen.getByRole("button", { name: "详情" }));
    fireEvent.focus(screen.getByRole("button", { name: "详情" }));
    expect(await screen.findByText("补充说明")).toBeInTheDocument();
  });
});
