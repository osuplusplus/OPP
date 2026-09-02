import { fireEvent, render, screen } from "@testing-library/react";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

import { Button, Input, Select, Tooltip } from "./ui";

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

  it("Input 与 Select 复用现有控件样式并透传事件", () => {
    const onInput = vi.fn();
    const onSelect = vi.fn();
    render(
      <>
        <Input aria-label="名称" className="custom-input" onChange={onInput} />
        <Select aria-label="模式" className="custom-select" onChange={onSelect}>
          <option value="osu">osu!</option>
          <option value="mania">mania</option>
        </Select>
      </>,
    );

    const input = screen.getByRole("textbox", { name: "名称" });
    const select = screen.getByRole("combobox", { name: "模式" });
    expect(input).toHaveClass("opp-input", "custom-input");
    expect(select).toHaveClass("opp-input", "custom-select");
    fireEvent.change(input, { target: { value: "Daisuke" } });
    fireEvent.change(select, { target: { value: "mania" } });
    expect(onInput).toHaveBeenCalledOnce();
    expect(onSelect).toHaveBeenCalledOnce();
  });

  it("Tooltip 使用 Radix 展示提示内容", async () => {
    render(<Tooltip content="补充说明"><button type="button">详情</button></Tooltip>);

    fireEvent.pointerMove(screen.getByRole("button", { name: "详情" }));
    fireEvent.focus(screen.getByRole("button", { name: "详情" }));
    expect(await screen.findByText("补充说明")).toBeInTheDocument();
  });
});
