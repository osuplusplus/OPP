# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Project Overview

OPP is a cross-platform osu! desktop toolkit built with Tauri 2, React 19, and Rust. It provides features including OAuth login, beatmap downloads, collection management, similarity matching, PP calculation, skin workshop, replay rendering, and live streaming integration. The project supports Windows x64 and Linux.

**Important**: This is an independent community project. Do not include Client Secrets, Tokens, credential export files, or app data in commits.

## Development Commands

### Frontend (React + Vite)
```bash
pnpm install              # Install dependencies
pnpm tauri dev            # Start development environment (includes Rust backend)
pnpm dev                  # Vite dev server only (without Tauri)
pnpm lint                 # ESLint with max-warnings 0
pnpm test                 # Run Vitest tests
pnpm test:watch           # Run tests in watch mode
pnpm build                # TypeScript compilation + Vite build
pnpm preview              # Preview production build
```

### Backend (Rust)
```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
```

### Release Build
```bash
pnpm tauri build          # Build production artifacts
# Windows: src-tauri/target/release/bundle/nsis/
# Linux: src-tauri/target/release/opp
```

## Architecture

### Data Flow
```text
React pages & components
  → features/*/api.ts (TanStack Query state boundary)
  → shared/lib/tauri.ts (typed command adapter layer)
  → Tauri command
  → Rust domain modules
  → osu! API / local files / system credentials / external apps
```

Frontend never directly accesses local files or credentials. Rust commands handle input validation, permission boundaries, and error conversion. Remote responses and local index data are passed through serialization contracts in `shared/types/osu.ts`.

### Frontend Structure
- `src/app/` — Application shell, routing, sidebar, title bar, global client/mode context
- `src/features/` — Business domain pages, query adapters, local components, pure logic
  - Each feature should not import internal state from another feature; use public APIs or small shared events
- `src/shared/components/` — Reusable components without business state
- `src/shared/lib/` — Formatting, style merging, Tauri command adapters
- `src/shared/types/` — Shared data contracts between frontend and backend

Large pages should place pure filtering/transformation logic in model files in the same directory, and split independent visual areas into components. Side effects should only be used for external synchronization, subscriptions, and resource cleanup, not for mirroring derivable state.

### Backend Structure

Rust backend organized by `domain / features / infrastructure`:

```text
src-tauri/src/
├── domain/                 # Shared, semantically stable OPP models
├── features/               # User-facing business capabilities
│   ├── account/           # OAuth, credentials, account caching
│   ├── collections/       # Collection sync, sharing, missing beatmap補齐
│   ├── danser/            # Local replay render queue
│   ├── local_analysis/    # Path detection, scanning, cache, resource analysis
│   ├── online_beatmaps/   # Online beatmap queries and downloads
│   ├── similarity/        # Similarity dataset, query, recommendations
│   ├── skin_workshop/     # Skin component merging and atomic writes
│   ├── live_render/       # Replay live preview (wgpu-based, Windows native window + other platforms canvas)
│   └── ...
├── infrastructure/        # osu! API, platform detection, storage, portable updates
├── tools/                 # Single-purpose tools without complex state
├── state.rs               # Dependency assembly and shared runtime state
├── commands.rs            # Tauri command registry
├── error.rs               # Unified error contract
├── lib.rs                 # App startup & lifecycle
└── main.rs                # Binary entry point
```

**Complex feature module structure**:
```text
<module>/
├── models.rs      # OPP domain data semantics
├── ports.rs       # Capability definitions (trait interfaces)
├── adapters/      # External system adapters (files, network, platform APIs)
├── service.rs     # Business flow orchestration
├── commands.rs    # Tauri command exposure
└── mod.rs         # Module declaration & export control
```

**Dependency direction**:
```text
commands → features → domain + ports ← adapters / infrastructure
```

- `domain/` does not depend on features or infrastructure
- Features can depend on domain and use infrastructure through explicit interfaces
- Cross-feature calls must use explicit `crate::features::<name>` paths
- Infrastructure handles external structure conversion, not business flows

### Module Organization Principles
1. **Models** unify data semantics (convert external structures like Stable, lazer, Realm, HTTP in adapters)
2. **Ports** define capabilities, not specific implementations (use traits only when multiple implementations, testing, or clear external differences exist)
3. **Adapters** isolate external world (Stable, lazer, API, tosu, platform differences)
4. **Service** handles business logic and flow composition
5. **Commands** only expose service to frontend (thin layer: deserialization, state access, service call, error conversion)
6. **mod.rs** controls module boundaries (internal implementations default private)
7. **Keep simple features simple** — don't add architecture for architecture's sake
8. **Split by responsibility**, not mechanically by file line count
9. **Refactoring must not change existing Tauri command names and serialization contracts** unless frontend migration is completed simultaneously

### Similarity Dataset
- **Standard**: Requires Analyzer v4 (`five-dimension-slider-rosu-reading-v4`) with dimensions: Aim, Speed, Reading, Slider, Overlap
  - Root directory must contain: `metadata.sqlite`, `features-v*.bin`, `indexes/difficulty-main.hnsw`, `normalizers/v*.bin`
- **Mania**: Requires Analyzer v1 (commit `1fa21fa6`) supporting native 4K/6K/7K with NM/DT/HT
  - Root directory must contain: `mania-metadata.sqlite`, `mania-features-v1.bin`, `normalizers/mania-v1.bin`, `indexes/mania-v1.buckets(.sha256)`
  - DT/HT mixed pools additionally require `mania-mod-features-v1.bin` and `beatmaps/<BeatmapID>.osu`

See `docs/similarity-dataset.md` for details.

### Platform Boundaries
- **Windows**: Registry + user directory detection, Credential Manager, native subwindows for live render
- **Linux**: XDG data directories, PATH commands (`osu-wine`, `osu-lazer`), Secret Service (D-Bus), `/proc` for process detection, `pkexec` for tosu Wine process reading
- **Danser integration**: Windows-only (requires `danser-cli.exe`, `ffmpeg.exe`, Explorer)
- File association and display gamma: Windows-only (via Windows API)

Platform-specific business should go into `infrastructure/platform.rs` or capability switches, avoiding scattered OS checks in pages and domain modules.

## Quality Gates

Before submitting, run on target platform:
```bash
pnpm lint
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
```

Windows and Linux builds must be separately verified on corresponding systems. Cross-platform compilation success does not mean runtime functionality works.

## Logging System

OPP uses a structured logging system for debugging and diagnostics. All logs are stored as JSONL files in `<app_data_dir>/logs/` with automatic rotation (keeps last 5 files) and sensitive data sanitization.

### When to Log

**Must log**:
- All Tauri command entry points
- File system operations (read, write, delete, move)
- Network requests (HTTP, WebSocket)
- External process calls
- Database operations
- Credential operations (login, token refresh)
- Error paths
- Critical business operations start and end

### How to Log

Use `LogSpan` to track operations with automatic timing and structured fields:

```rust
use crate::infrastructure::logging::global;

pub async fn download_beatmap(id: i32) -> CommandResult<PathBuf> {
    // Create span - automatically logs operation start
    let mut span = global().map(|logger| logger.operation("beatmap_downloader", "download_beatmap"));

    // Log intermediate steps
    if let Some(ref s) = span {
        s.info(format!("开始下载谱面 ID: {}", id), Some(serde_json::json!({ "beatmap_id": id })));
    }

    // Log IO operations
    let result = fetch_from_api(id).await;
    if let Some(ref s) = span {
        s.io("fetch_api", &result);
    }
    let data = result?;

    // Log file operations
    let write_result = fs::write(&path, &data);
    if let Some(ref s) = span {
        s.fs_op("write", &path, &write_result);
    }
    write_result?;

    // Finish span - logs operation complete with duration
    if let Some(ref mut s) = span {
        s.finish_ok(Some(serde_json::json!({ "path": path.to_string_lossy() })));
    }

    Ok(path)
}
```

**Simplified with helper**:
```rust
use crate::infrastructure::logging::{global, finish_span};

pub fn simple_operation() -> CommandResult<String> {
    let span = global().map(|logger| logger.operation("module", "operation"));
    let result = perform_work();
    finish_span(span, result)
}
```

**Available span methods**:
- `span.info(msg, fields)` - Log info step
- `span.warn(msg, fields)` - Log warning
- `span.io(action, &result)` - Log IO operation result
- `span.fs_op(op, path, &result)` - Log file system operation
- `span.http_request(method, url, status)` - Log HTTP request
- `span.finish_ok(fields)` - Complete successfully
- `span.finish_error(&error)` - Complete with error

**Simple logging macros**:
```rust
log_info!("module", "Application started");
log_warn!("module", "Config missing, using defaults");
log_error!("module", "Failed to connect: {}", error);
log_debug!("module", "State: {:?}", state);
```

See `docs/日志系统使用指南.md` for comprehensive examples and `src-tauri/src/infrastructure/logging_examples.rs` for refactoring patterns.

### Viewing Logs

Logs are JSONL format. Use `jq` for analysis:
```bash
# View all logs
cat opp-*.jsonl | jq .

# Filter errors
cat opp-*.jsonl | jq 'select(.level == "ERROR")'

# Track specific request
cat opp-*.jsonl | jq 'select(.request_id == "abc-123")'

# Find slowest operations
cat opp-*.jsonl | jq 'select(.fields.duration_ms != null) | {op: .message, duration: .fields.duration_ms}' | jq -s 'sort_by(.duration) | reverse | .[0:10]'
```

From frontend:
```typescript
const logDir = await invoke<string>('get_log_directory');
const files = await invoke<LogFileInfo[]>('list_log_files');
await invoke('open_log_directory');
```

## Version Updates
Version numbers must be updated simultaneously in:
- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`

Frontend "About" page reads `package.json` at build time. User-visible changes should be documented in `docs/版本变更记录.md`.

## Windows Portable Update Flow

Windows clients check `https://github.com/osuplusplus/OPP/releases/latest/download/latest.json` on startup and in settings. The manifest is published as a same-named asset for each official GitHub Release:

```json
{
  "version": "0.4.5",
  "url": "https://github.com/osuplusplus/OPP/releases/download/v0.4.5/OPP_0.4.5_windows_x64_portable.exe"
}
```

When a higher remote version is detected, the client downloads the new EXE from the manifest's HTTPS URL. A temporary copy of the current EXE waits for the old process to exit, places the new EXE at the original path, and restarts. If the new version exits abnormally during early startup, the old EXE is restored. Keep Releases as Draft until all assets are uploaded to avoid the fixed address switching to a half-finished product.

## Additional Resources
- [架构与开发 (Architecture & Development)](./docs/架构与开发.md)
- [后端代码组织方式 (Backend Code Organization)](./docs/后端代码组织方式.md)
- [相似谱面数据集 (Similarity Dataset)](./docs/similarity-dataset.md)
- [Linux 使用与构建 (Linux Usage & Build)](./docs/Linux.md)
- [版本变更记录 (Version Changelog)](./docs/版本变更记录.md)

## PR Contribution
Direct PRs to the `dev` branch. Issues are welcome for discussion.
