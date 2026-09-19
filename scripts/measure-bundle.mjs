import { readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { gzipSync } from "node:zlib";

const directory = resolve(process.argv[2] ?? "dist");
const manifest = JSON.parse(readFileSync(resolve(directory, ".vite/manifest.json"), "utf8"));
for (const entry of ["index.html", "mini-player.html"]) {
  const visited = new Set();
  const files = new Set();
  function visit(key) {
    if (visited.has(key)) return;
    visited.add(key);
    const chunk = manifest[key];
    if (!chunk) throw new Error(`Missing manifest entry: ${key}`);
    files.add(chunk.file);
    for (const css of chunk.css ?? []) files.add(css);
    for (const dependency of chunk.imports ?? []) visit(dependency);
  }
  visit(entry);
  let bytes = 0;
  let gzipBytes = 0;
  for (const file of files) {
    const path = resolve(directory, file);
    bytes += statSync(path).size;
    gzipBytes += gzipSync(readFileSync(path)).length;
  }
  console.log(JSON.stringify({ entry, files: [...files].sort(), bytes, gzip_bytes: gzipBytes }));
}
