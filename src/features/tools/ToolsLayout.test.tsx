import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { ToolsLayout } from "./ToolsLayout";

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname}</output>;
}

describe("ToolsLayout", () => {
  it("closes directly to the online beatmaps page", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter initialEntries={["/tools/game"]}>
        <Routes>
          <Route element={<ToolsLayout />} path="/tools">
            <Route element={<div />} path="game" />
          </Route>
          <Route element={<LocationProbe />} path="/online/beatmaps" />
        </Routes>
      </MemoryRouter>,
    );

    await user.click(screen.getByRole("button", { name: "关闭工具" }));

    expect(screen.getByTestId("location")).toHaveTextContent("/online/beatmaps");
  });
});
