/** Repeat image references, never downloads, to fill even a small library. */
export function mosaicTiles(urls: string[]) {
  const unique = [...new Set(urls)].slice(0, 24);
  return unique.length ? Array.from({ length: 24 }, (_, i) => unique[i % unique.length]) : [];
}
