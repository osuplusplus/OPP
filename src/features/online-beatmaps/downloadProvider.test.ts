import { describe, expect, it } from "vitest";

import { resolveDefaultDownloadProvider } from "./downloadProvider";

describe("resolveDefaultDownloadProvider", () => {
  it.each(["sayobot", "osudirect", "nekoha", "hinai", "catboy", "nerinyan"] as const)(
    "keeps the configured %s provider",
    (provider) => {
      expect(resolveDefaultDownloadProvider({
        default_beatmap_download_provider: provider,
      })).toBe(provider);
    },
  );

  it("falls back to Sayobot when settings are unavailable", () => {
    expect(resolveDefaultDownloadProvider(undefined)).toBe("sayobot");
  });
});
