import { useEffect, useState, type CSSProperties } from "react";

export type StageArtworkStyle = CSSProperties & Record<`--${string}`, string | number>;

// Quantized chromatic pixels keep a white sky or dark border from dominating the accent.
export function extractStageHue(pixels: ArrayLike<number>): number | null {
  const buckets = new Map<number, number>();
  for (let index = 0; index < pixels.length; index += 4) {
    if (pixels[index + 3] < 128) continue;
    const [red, green, blue] = [pixels[index], pixels[index + 1], pixels[index + 2]].map((value) => value / 255);
    const high = Math.max(red, green, blue);
    const low = Math.min(red, green, blue);
    const delta = high - low;
    if (delta < 0.12 || high < 0.16 || low > 0.88) continue;
    const hue = ((high === red ? (green - blue) / delta : high === green ? (blue - red) / delta + 2 : (red - green) / delta + 4) * 60 + 360) % 360;
    const bucket = Math.floor(hue / 15) * 15;
    buckets.set(bucket, (buckets.get(bucket) ?? 0) + delta);
  }
  return [...buckets].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
}

export function useArtworkPalette(source: string | null, suspended = false) {
  const [artwork, setArtwork] = useState<{ source: string | null; hue: number | null }>({ source: null, hue: null });
  useEffect(() => {
    if (suspended) return;
    let active = true;
    const commit = (nextSource: string | null, hue: number | null) => {
      if (active) setArtwork({ source: nextSource, hue });
    };
    if (!source) {
      void Promise.resolve().then(() => commit(null, null));
      return () => { active = false; };
    }
    const image = new Image();
    image.onload = async () => {
      try { await image.decode(); } catch { /* onload already established a usable image */ }
      if (!active) return;
      let hue: number | null = null;
      try {
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 32;
        const context = canvas.getContext("2d", { willReadFrequently: true });
        if (context) {
          context.drawImage(image, 0, 0, 32, 32);
          hue = extractStageHue(context.getImageData(0, 0, 32, 32).data);
        }
      } catch { /* neutral colors remain usable when pixel sampling is unavailable */ }
      commit(source, hue);
    };
    image.onerror = () => commit(source, null);
    image.src = source;
    return () => {
      active = false;
      image.onload = null;
      image.onerror = null;
    };
  }, [source, suspended]);

  const renderedSource = suspended ? artwork.source : source;
  const hue = artwork.source === renderedSource ? artwork.hue : null;
  const style: StageArtworkStyle = hue === null ? {} : {
    "--stage-hue": hue,
    "--stage-accent-dark": `hsl(${hue} 70% 78%)`,
    "--stage-accent-light": `hsl(${hue} 65% 25%)`,
  };
  return { source: renderedSource, hue, style };
}
