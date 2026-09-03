const WINDOWS_1252_EXTENDED: Record<number, number> = {
  0x20ac: 0x80,
  0x201a: 0x82,
  0x192: 0x83,
  0x201e: 0x84,
  0x2026: 0x85,
  0x2020: 0x86,
  0x2021: 0x87,
  0x2c6: 0x88,
  0x2030: 0x89,
  0x160: 0x8a,
  0x2039: 0x8b,
  0x152: 0x8c,
  0x17d: 0x8e,
  0x2018: 0x91,
  0x2019: 0x92,
  0x201c: 0x93,
  0x201d: 0x94,
  0x2022: 0x95,
  0x2013: 0x96,
  0x2014: 0x97,
  0x2dc: 0x98,
  0x2122: 0x99,
  0x161: 0x9a,
  0x203a: 0x9b,
  0x153: 0x9c,
  0x17e: 0x9e,
  0x178: 0x9f,
};

/** Repairs UTF-8 bytes that were decoded as Windows-1252 before JSON encoding. */
export function repairMedalText(value: unknown): string {
  if (typeof value !== "string") return "";

  const bytes: number[] = [];
  let changed = false;
  for (const character of value) {
    const codePoint = character.codePointAt(0) ?? 0;
    const byte = codePoint <= 0xff ? codePoint : WINDOWS_1252_EXTENDED[codePoint];
    if (byte === undefined) return value;
    bytes.push(byte);
    changed ||= byte !== codePoint;
  }
  // A common UTF-8/Windows-1252 round-trip (for example, "Ã©") only uses
  // code points below 0x100, so it does not set `changed` above.
  if (!changed && !/[ÃÂâ€™œž€›ƒ]|[À-ÿ]{2}/.test(value)) return value;

  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(bytes));
  } catch {
    return value;
  }
}
