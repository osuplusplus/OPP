import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import { poolImportSession } from "./importSession";
import { PoolImportStatus } from "./PoolImportStatus";
vi.mock("../../shared/lib/tauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/tauri")>();
  return { ...real, desktopApi: { ...real.desktopApi, openTournamentPool: vi.fn() } };
});
afterEach(() => { cleanup(); poolImportSession.dismiss(); });
it("shows real import phases, persists an actionable error and completes after retry", async () => {
  let resolve!: (value: { folder_id: string; existing: boolean }) => void;
  let reject!: (error: unknown) => void;
  vi.mocked(desktopApi.openTournamentPool).mockReturnValueOnce(new Promise((_resolve, fail) => { reject = fail; }))
    .mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  render(<PoolImportStatus />);
  act(() => { poolImportSession.receive({ id: 1, reference: { provider: "opp", url: "https://example.com/pool.json" } }); });
  expect(screen.getByRole("status")).toHaveTextContent("读取图池");
  act(() => { poolImportSession.progress({ request_id: 1, phase: "enriching" }); });
  expect(screen.getByRole("status")).toHaveTextContent("补全资料");
  await act(async () => reject({ message: "网络不可用" }));
  expect(screen.getByRole("alert")).toHaveTextContent("网络不可用");
  await userEvent.click(screen.getByRole("button", { name: "重试导入" }));
  act(() => { poolImportSession.progress({ request_id: 1, phase: "saving" }); });
  expect(screen.getByRole("status")).toHaveTextContent("保存到 OPP");
  await act(async () => resolve({ folder_id: "pool", existing: false }));
  expect(screen.queryByLabelText("图池导入")).not.toBeInTheDocument();
});
