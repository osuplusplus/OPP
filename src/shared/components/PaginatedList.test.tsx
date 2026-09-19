import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { PaginatedList } from "./PaginatedList";

it("clamps the current page when removing the last items", async () => {
  const user = userEvent.setup();
  const view = (items: number[]) => <PaginatedList items={items} pageSize={2} label="测试">{(page) => <ul>{page.map((n) => <li key={n}>项目 {n}</li>)}</ul>}</PaginatedList>;
  const { rerender } = render(view([1, 2, 3]));
  await user.click(screen.getByRole("button", { name: "测试下一页" }));
  expect(screen.getAllByRole("listitem")).toHaveLength(1);
  expect(screen.getByText("项目 3")).toBeInTheDocument();
  rerender(view([1, 2]));
  expect(screen.getAllByRole("listitem")).toHaveLength(2);
  expect(screen.getByText("项目 1")).toBeInTheDocument();
  expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
});
