// Patch Tauri AppImages so they render on modern Wayland hosts (issue #46).
//
// Root cause: linuxdeploy copies `libwayland-client.so.0` from the build base
// (Ubuntu 22.04 in CI) into the AppImage. At runtime the host provides libEGL
// (Mesa); Mesa's EGL display creation needs a libwayland-client that matches
// the host's libEGL, but WebKit hands it a wl_display created through the
// bundled Ubuntu copy. The duplicated client makes
// `eglGetPlatformDisplay(EGL_PLATFORM_WAYLAND_KHR, ...)` fail and WebKit
// aborts with "Could not create default EGL display: EGL_BAD_PARAMETER"
// before rendering anything.
//
// Deleting the bundled copy lets ld.so fall back to the host's
// libwayland-client, which always matches the host's libEGL because they ship
// together. Every distro with a Wayland session provides it in the default
// library path (the deb bundle links the host stack and never had the issue).
//
// The linuxdeploy-plugin-gtk hook may also force `export GDK_BACKEND=x11`,
// which overrides an explicit user choice. We keep x11 as the default (the
// behavior every client of this AppImage has been verified against) but stop
// overriding a user-set GDK_BACKEND.
//
// Usage:
//   node scripts/patch-appimage.mjs --dir src-tauri/target/release/bundle/appimage
//   node scripts/patch-appimage.mjs path/to/OPP_x.y.z_linux_amd64.AppImage

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { constants, existsSync, readdirSync } from 'node:fs';
import { chmod, mkdir, open, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const DEFAULT_BUNDLE_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri', 'target', 'release', 'bundle', 'appimage');

// Fixed tool + checksum so the repack cannot be influenced by a moving target.
const APPIMAGETOOL_VERSION = '1.9.1';
const APPIMAGETOOL_SHA256 = 'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0';
const APPIMAGETOOL_URL = `https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-x86_64.AppImage`;

const WAYLAND_CLIENT_LIBRARY_PATTERN = /^libwayland-client\.so(?:\.|$)/;
// The opener plugin resolves `xdg-open` through PATH. AppImage's generated
// launcher prepends `usr/bin`, so a bundled copy would shadow the host's
// desktop integration and can fail to launch the user's browser (issue #47).
const BUNDLED_XDG_OPEN_PATH = 'usr/bin/xdg-open';
const FORCED_GDK_BACKEND_PATTERN =
  /^[ \t]*(?:export[ \t]+)?GDK_BACKEND[ \t]*=[ \t]*(?:"x11"|'x11'|x11)[ \t]*(?:#.*)?$/gm;
// Same effective backend as before, but a user-provided GDK_BACKEND wins.
const RESPECTED_GDK_BACKEND = 'if [ -z "${GDK_BACKEND+x}" ]; then export GDK_BACKEND=x11; fi';

function log(message) {
  console.log(`[patch-appimage] ${message}`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const output = [result.stdout, result.stderr]
      .filter(Boolean)
      .map((chunk) => chunk.toString())
      .join('\n')
      .trimEnd();
    if (output) console.error(output);
    throw new Error(`${command} ${args.join(' ')} exited with ${result.status}`);
  }
  return result;
}

// Like run(), but swallows the expected verbose extraction output and only
// shows it when extraction fails.
function runQuiet(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: ['ignore', 'pipe', 'pipe'], ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const output = [result.stdout, result.stderr]
      .filter(Boolean)
      .map((chunk) => chunk.toString())
      .join('\n')
      .trimEnd();
    if (output) console.error(output);
    throw new Error(`${command} ${args.join(' ')} exited with ${result.status}`);
  }
}

function findAppImage(dirPath) {
  const candidates = readdirSync(dirPath).filter((name) => name.toLowerCase().endsWith('.appimage'));
  if (candidates.length === 0) throw new Error(`no .AppImage found in ${dirPath}`);
  if (candidates.length > 1) {
    throw new Error(`multiple .AppImage files in ${dirPath}, pass one explicitly:\n  ${candidates.join('\n  ')}`);
  }
  return join(dirPath, candidates[0]);
}

async function stripBundledWaylandClient(extractDir) {
  const removed = [];
  const pending = [extractDir];
  while (pending.length > 0) {
    const directory = pending.pop();
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        pending.push(path);
      } else if (WAYLAND_CLIENT_LIBRARY_PATTERN.test(entry.name)) {
        await rm(path);
        removed.push(path);
      }
    }
  }
  return removed;
}

async function stripBundledXdgOpen(extractDir) {
  const path = join(extractDir, BUNDLED_XDG_OPEN_PATH);
  if (!existsSync(path)) return false;
  await rm(path);
  log(`removed bundled ${BUNDLED_XDG_OPEN_PATH}`);
  return true;
}

function rewriteForcedGdkBackend(hookText) {
  FORCED_GDK_BACKEND_PATTERN.lastIndex = 0;
  if (!FORCED_GDK_BACKEND_PATTERN.test(hookText)) {
    return { text: hookText, changed: false };
  }
  FORCED_GDK_BACKEND_PATTERN.lastIndex = 0;
  return { text: hookText.replace(FORCED_GDK_BACKEND_PATTERN, RESPECTED_GDK_BACKEND), changed: true };
}

async function ensureAppimagetool(cacheDir = join('/tmp', 'opp-appimagetool')) {
  await mkdir(cacheDir, { recursive: true });
  const target = join(cacheDir, `appimagetool-${APPIMAGETOOL_VERSION}`);
  const verify = async (path) => {
    const handle = await open(path, constants.O_RDONLY);
    try {
      const bytes = await handle.readFile();
      const actual = createHash('sha256').update(bytes).digest('hex');
      if (actual !== APPIMAGETOOL_SHA256) {
        throw new Error(`appimagetool checksum mismatch: expected ${APPIMAGETOOL_SHA256}, got ${actual}`);
      }
    } finally {
      await handle.close();
    }
    return path;
  };

  if (existsSync(target)) {
    return verify(target);
  }
  log(`downloading pinned appimagetool ${APPIMAGETOOL_VERSION}`);
  const response = await fetch(APPIMAGETOOL_URL, { redirect: 'follow' });
  if (!response.ok) throw new Error(`failed to download appimagetool: ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const downloadPath = `${target}.${process.pid}.download`;
  await writeFile(downloadPath, bytes);
  await chmod(downloadPath, 0o755);
  await rename(downloadPath, target);
  return verify(target);
}

async function patchAppImage(targetPath) {
  if (!existsSync(targetPath)) throw new Error(`not found: ${targetPath}`);
  log(`patching ${targetPath}`);

  const workDir = dirname(targetPath);
  const extractDir = join(workDir, 'squashfs-root');
  await rm(extractDir, { recursive: true, force: true });

  await chmod(targetPath, 0o755);
  runQuiet(targetPath, ['--appimage-extract'], { cwd: workDir });
  if (!existsSync(extractDir)) throw new Error(`extraction did not produce ${extractDir}`);

  try {
    const stripped = await stripBundledWaylandClient(extractDir);
    const removedBundledXdgOpen = await stripBundledXdgOpen(extractDir);
    for (const path of stripped) {
      log(`removed bundled ${path.slice(extractDir.length + 1)}`);
    }

    let hookChanged = false;
    const gtkHookPath = join(extractDir, 'apprun-hooks', 'linuxdeploy-plugin-gtk.sh');
    if (existsSync(gtkHookPath)) {
      const hook = await readFile(gtkHookPath, 'utf8');
      const rewritten = rewriteForcedGdkBackend(hook);
      if (rewritten.changed) {
        await writeFile(gtkHookPath, rewritten.text);
        hookChanged = true;
        log('GTK hook no longer overrides a user-provided GDK_BACKEND');
      } else {
        log('GTK hook has no forced GDK_BACKEND to rewrite');
      }
    } else {
      log('no linuxdeploy-plugin-gtk hook found, skipping GDK_BACKEND rewrite');
    }

    if (stripped.length === 0 && !removedBundledXdgOpen && !hookChanged) {
      log('nothing to patch — leaving the AppImage in place');
      await rm(extractDir, { recursive: true, force: true });
      return;
    }

    const appimagetool = await ensureAppimagetool();
    const patchedPath = `${targetPath}.patched`;
    await rm(patchedPath, { force: true });
    // APPIMAGE_EXTRACT_AND_RUN lets appimagetool run without FUSE (CI sandboxes).
    run(appimagetool, [extractDir, patchedPath], {
      cwd: workDir,
      env: { ...process.env, ARCH: 'x86_64', APPIMAGE_EXTRACT_AND_RUN: '1' },
    });

    await rm(extractDir, { recursive: true, force: true });
    await rename(patchedPath, targetPath);
    await chmod(targetPath, 0o755);
    log(`done: ${targetPath}`);
  } catch (error) {
    await rm(extractDir, { recursive: true, force: true });
    throw error;
  }
}

const args = process.argv.slice(2);
let target;
if (args[0] === '--dir') {
  target = findAppImage(resolve(args[1] ?? DEFAULT_BUNDLE_DIR));
} else if (args[0]) {
  target = resolve(args[0]);
} else {
  target = findAppImage(resolve(DEFAULT_BUNDLE_DIR));
}

patchAppImage(target).catch((error) => {
  console.error(`[patch-appimage] FAILED: ${error.message}`);
  process.exit(1);
});
