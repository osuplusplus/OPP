import { readFileSync } from "node:fs";

// Run from the repository root, including when invoked by the release workflow.
const read = (path) => readFileSync(path, "utf8").replace(/\r\n/g, "\n");

try {
  const version = JSON.parse(read("package.json")).version;
  const tag = process.argv[2] ?? `v${version}`;
  if (!/^\d+\.\d+\.\d+$/.test(version) || tag !== `v${version}`) {
    throw new Error(`Release tag ${tag} must match package.json v${version}.`);
  }

  const cargoPackage = read("src-tauri/Cargo.toml")
    .split(/^\[/m)
    .find((section) => section.startsWith("package]\n"));
  const lockPackage = read("src-tauri/Cargo.lock")
    .split("[[package]]")
    .find((block) => /^name = "opp"$/m.test(block));
  const versions = {
    "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
    "src-tauri/Cargo.toml": cargoPackage?.match(/^version = "([^"]+)"$/m)?.[1],
    "src-tauri/Cargo.lock": lockPackage?.match(/^version = "([^"]+)"$/m)?.[1],
  };
  for (const [path, actual] of Object.entries(versions)) {
    if (actual !== version) {
      throw new Error(`${path}: expected ${version}, found ${actual ?? "no OPP version"}.`);
    }
  }

  const sections = read("docs/版本变更记录.md").split(/^## /m).slice(1);
  const matches = sections.filter((section) => section.split("\n", 1)[0].trim() === tag);
  if (matches.length !== 1) {
    throw new Error(`Expected exactly one changelog heading: ## ${tag}.`);
  }
  const body = matches[0].slice(matches[0].indexOf("\n") + 1).trim();
  if (!body || !/^- /m.test(body)) {
    throw new Error(`Changelog for ${tag} must contain release notes.`);
  }

  // Relative documentation links must still work on the GitHub Release page.
  const notes = body.replace(
    /\]\(\.\/([^\s)]+)\)/g,
    (_, path) => `](https://github.com/osuplusplus/OPP/blob/${tag}/docs/${path})`,
  );
  process.stdout.write(`${notes}\n`);
} catch (error) {
  process.stderr.write(`Release preparation failed: ${error.message}\n`);
  process.exitCode = 1;
}
