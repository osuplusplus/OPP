import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { OnboardingTour } from "./OnboardingTour";
import { needsOnboarding, onboardingSteps } from "./tourContent";

describe("OnboardingTour", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn(),
    });
  });

  it("moves through steps and completes the tour", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<OnboardingTour onClose={onClose} reduceMotion />);

    expect(screen.queryByText(/交流群|加入 QQ 群/)).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "欢迎使用 OPP" })).toBeInTheDocument();
    for (let index = 1; index < onboardingSteps.length; index += 1) {
      await user.click(screen.getByRole("button", { name: "下一步" }));
      expect(screen.getByRole("heading", { name: onboardingSteps[index].title })).toBeInTheDocument();
    }
    expect(screen.queryByText(/交流群|加入 QQ 群/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "完成" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("supports going back and closing with Escape", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<OnboardingTour onClose={onClose} reduceMotion={false} />);

    await user.click(screen.getByRole("button", { name: "下一步" }));
    await user.click(screen.getByRole("button", { name: "上一步" }));
    expect(screen.getByRole("heading", { name: "欢迎使用 OPP" })).toBeInTheDocument();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("traps focus and restores the previous focus when removed", async () => {
    const launcher = document.createElement("button");
    document.body.appendChild(launcher);
    launcher.focus();
    const { unmount } = render(<OnboardingTour onClose={vi.fn()} reduceMotion />);

    await waitFor(() => expect(screen.getByRole("dialog")).toHaveFocus());
    unmount();
    expect(launcher).toHaveFocus();
    launcher.remove();
  });

  it("temporarily expands a collapsed module for its guide step", async () => {
    const details = document.createElement("details");
    const summary = document.createElement("summary");
    summary.textContent = "数值范围";
    details.appendChild(summary);
    document.body.appendChild(details);

    const { unmount } = render(
      <OnboardingTour
        onClose={vi.fn()}
        reduceMotion
        steps={[{
          title: "数值范围",
          description: "限制搜索数值。",
          example: "设置 5–6 星。",
          targetText: "数值范围",
          expandTarget: true,
        }]}
      />,
    );

    await waitFor(() => expect(details.open).toBe(true));
    expect(screen.getByText("设置 5–6 星。")).toBeInTheDocument();
    unmount();
    expect(details.open).toBe(false);
    details.remove();
  });
});

describe("needsOnboarding", () => {
  it("treats missing and older versions as unseen", () => {
    expect(needsOnboarding(undefined)).toBe(true);
    expect(needsOnboarding(0)).toBe(true);
    expect(needsOnboarding(1)).toBe(false);
  });
});
