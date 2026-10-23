# JobLens

A private desktop app that finds remote jobs you can actually apply to, ranks them against your resume, and helps you write honest applications. Everything stays on your computer.

- Scope: [`docs/Feature Spec.md`](docs/Feature%20Spec.md)
- Build plan: [`docs/PLAN.md`](docs/PLAN.md)
- UI reference: `docs/prototype/`

## Stack

Tauri v2 (Rust) + React + TypeScript + Vite. SQLite (bundled via `rusqlite`). Targets macOS and Linux.

## Prerequisites

- Node 24+ and pnpm 10
- Rust stable (`rustup`)
- macOS: Xcode command line tools
- Linux: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libxdo-dev libssl-dev build-essential`

## Commands

| Command | What it does |
|---|---|
| `pnpm install` | Install JS deps |
| `pnpm tauri dev` | Run the app with hot reload |
| `pnpm tauri build` | Release build + installers |
| `pnpm check` | Everything CI runs: lint, typecheck, JS + Rust tests, clippy |
| `pnpm lint:fix` | Auto-fix formatting and lint (Biome) |
| `pnpm test:rust` | Rust tests; also regenerates `src/bindings/*.ts` |

## Layout

```
src/                 React UI
  bindings/          TS types generated from Rust (ts-rs) — don't edit, run `pnpm test:rust`
  components/        Shared UI (Sidebar, Icon, Toast, EmptyState)
  views/             One file per top-level screen
  lib/ipc.ts         Typed wrappers for every Tauri command
  styles/tokens.css  Design tokens from the prototype
src-tauri/
  src/commands.rs    Tauri commands exposed to the UI
  src/db/            SQLite open + migrations (migrations/NNNN_*.sql, append-only)
  src/state.rs       App state (data dir, DB connection)
```

## Data

Stored in the OS app data dir:
- macOS: `~/Library/Application Support/app.joblens.desktop/joblens.db`
- Linux: `~/.local/share/app.joblens.desktop/joblens.db`

The schema version lives in SQLite `user_version`. To change the schema, add a new `src-tauri/src/db/migrations/NNNN_name.sql` and register it in `MIGRATIONS`. Never edit a shipped migration.

## Privacy rules

- No telemetry. The CSP in `tauri.conf.json` blocks all network calls from the UI. Any network access goes through Rust commands.
- Fonts and icons are bundled. No CDN.
