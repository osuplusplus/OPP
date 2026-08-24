import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import { DataCenterPage } from "./DataCenterPage";

vi.mock("../../app/ModeContext", () => ({
  useMode: () => ({ ruleset: "osu" }),
}));

vi.mock("./api", () => ({
  useOwnProfile: () => ({
    data: {
      data: {
        id: 42,
        username: "Pestl",
        avatar_url: "",
        country_code: "CN",
        is_active: true,
        is_online: false,
        is_supporter: true,
        cover_url: "https://assets.ppy.sh/user-profile-covers/42/example.jpeg",
      },
    },
    isLoading: false,
  }),
}));

describe("DataCenterPage", () => {
  it("presents the account as player information with profile navigation", () => {
    render(
      <MemoryRouter initialEntries={["/data/overview"]}>
        <Routes>
          <Route element={<DataCenterPage />} path="/data">
            <Route element={<p>概览内容</p>} path="overview" />
          </Route>
        </Routes>
      </MemoryRouter>,
    );

    expect(screen.getByRole("heading", { name: "Pestl" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "玩家信息页面" })).toBeInTheDocument();
    expect(screen.getByText("osu! · osu! 官方玩家资料")).toBeInTheDocument();
  });
});
