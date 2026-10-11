import { describe, expect, it } from "vitest";
import {
  CURRENT_PAGE_ONBOARDING_VERSION,
  getPageGuide,
  needsPageOnboarding,
} from "./pageTourContent";

const guidedRoutes = [
  "/beatmaphub",
  "/online/beatmaps",
  "/collections",
  "/online/similar",
  "/trainer",
  "/data",
  "/local/maps",
  "/local/skins",
  "/local/media",
  "/local/media/render",
  "/tosu",
  "/tools",
  "/settings",
  "/skill-analysis",
  "/career",
  "/view-trainer",
  "/tools/game",
  "/tools/beatmaps",
  "/tools/system",
  "/tools/live",
  "/community/tournaments",
  "/community/lobbies",
];

describe("page tour content", () => {
  it("defines a complete guide for every feature route", () => {
    for (const route of guidedRoutes) {
      const guide = getPageGuide(route);
      expect(guide!.steps.length).toBeGreaterThanOrEqual(5);
      expect(guide?.steps[0].target).toBe('[data-page-guide-title="true"]');
      expect(guide?.steps.slice(1, -1).every((step) => Boolean(step.example))).toBe(true);
    }
  });

  it("only requests guides with unseen versions", () => {
    const guide = getPageGuide("/tools");
    expect(guide).not.toBeNull();
    expect(needsPageOnboarding(undefined, guide!)).toBe(true);
    expect(needsPageOnboarding(1, guide!)).toBe(true);
    expect(needsPageOnboarding(guide!.version, guide!)).toBe(false);
    expect(getPageGuide("/unknown")).toBeNull();
  });

  it("uses a dedicated version for the redesigned online beatmap guide", () => {
    const guide = getPageGuide("/online/beatmaps");
    expect(guide?.version).toBe(5);
    expect(guide?.steps.map((step) => step.target)).toEqual(expect.arrayContaining([
      '[data-page-guide-online-search="true"]',
      '[data-page-guide-online-filter-trigger="true"]',
      '[data-page-guide-online-results="true"]',
      '[data-page-guide-online-download="true"]',
    ]));
    expect(getPageGuide("/settings")?.version).toBe(CURRENT_PAGE_ONBOARDING_VERSION);
  });

  it("follows the current nested routes after page reorganizations", () => {
    expect(getPageGuide("/view-trainer")?.id).toBe("trainer");
    expect(getPageGuide("/data/overview")?.id).toBe("data-center");
    expect(getPageGuide("/tools/live")?.id).toBe("tools-live");
    expect(getPageGuide("/community/tournaments")?.id).toBe("community-tournaments");
  });
});
