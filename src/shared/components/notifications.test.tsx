import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { notification, NotificationCard, NotificationViewport } from "./notifications";

describe("NotificationCard", () => {
  it("uses alert semantics for errors and exposes close action", () => {
    const close = vi.fn();
    render(<NotificationCard description="无法连接下载源" onClose={close} title="下载失败" tone="error" />);

    expect(screen.getByRole("alert")).toHaveTextContent("无法连接下载源");
    fireEvent.click(screen.getByRole("button", { name: "关闭通知" }));
    expect(close).toHaveBeenCalledOnce();
  });

  it("uses polite status semantics for successful feedback", () => {
    render(<NotificationCard title="保存完成" tone="success" />);
    expect(screen.getByRole("status")).toHaveTextContent("保存完成");
  });

  it("supports the shared imperative notification API", () => {
    render(<NotificationViewport />);
    let id = "";
    act(() => { id = notification.success("同步完成", "收藏夹已经写回。"); });
    expect(screen.getByRole("status")).toHaveTextContent("收藏夹已经写回。");
    act(() => notification.dismiss(id));
    expect(screen.queryByText("同步完成")).not.toBeInTheDocument();
  });
});
