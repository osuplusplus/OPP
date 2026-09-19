import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { OnlineSourceCredit } from "./OnlineSourceCredit";

const openExternal = vi.hoisted(() => vi.fn());
vi.mock("../../../shared/lib/tauri", () => ({
  desktopApi: { openExternal },
}));

it("credits Sayobot and opens its official website", async () => {
  render(<OnlineSourceCredit />);
  expect(screen.getByText(/在线背景与“小夜”镜像下载来源/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "打开小夜官网" }));
  expect(openExternal).toHaveBeenCalledWith("https://osu.sayobot.cn");
});
