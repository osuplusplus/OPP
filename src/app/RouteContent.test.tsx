import { lazy } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Link, MemoryRouter, Route, Routes } from "react-router-dom";
import { expect, it } from "vitest";
import { RouteContent } from "./RouteContent";

it("keeps navigation usable while a feature bundle is still loading", async () => {
  const PendingPage = lazy(() => new Promise<{ default: () => React.ReactNode }>(() => {}));
  render(<MemoryRouter initialEntries={["/slow"]}><Routes>
    <Route element={<><nav><Link to="/ready">切换页面</Link></nav><RouteContent /></>}>
      <Route path="slow" element={<PendingPage />} />
      <Route path="ready" element={<p>页面已切换</p>} />
    </Route>
  </Routes></MemoryRouter>);
  expect(screen.getByRole("status")).toHaveTextContent("正在加载页面");
  await userEvent.setup().click(screen.getByRole("link", { name: "切换页面" }));
  expect(await screen.findByText("页面已切换")).toBeInTheDocument();
  expect(screen.getByRole("navigation")).toBeVisible();
});
