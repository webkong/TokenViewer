# AGENTS.md — TokenViewer

Native macOS menu-bar app that tracks AI token usage & cost across 23 coding tools. Local-first: all data in `~/.tokenviewer/data.db`, no cloud, no account.

- **Stack**: Rust core (parsing + SQLite + pricing) ⟷ C FFI ⟷ SwiftUI macOS app
- **Brand color**: emerald `#059669`
- **License**: see `LICENSE`. Public repo: `webkong/TokenViewer`.

## Build / Run / Test

```bash
# Build Rust core + Swift app, then launch (the everyday loop)
bash run.sh                 # add --skip-sync to launch without auto-sync

# Rust only
cd core && cargo build --release --target aarch64-apple-darwin
cd core && cargo test                       # unit + tests/integration.rs

# Swift only (from macos/)
cd macos && xcodebuild -scheme TokenViewer -configuration Release build
```

- After ANY change, run `bash run.sh` and confirm `BUILD SUCCEEDED`.
- The Xcode project is generated from `macos/project.yml` via `xcodegen generate` (run after adding/removing Swift files). `project.pbxproj` is committed.
- The Swift target `-force_load`s the static lib at `core/target/aarch64-apple-darwin/release/libtokenviewer_core.a` (a preBuildScript also rebuilds it). Always build the Rust target `aarch64-apple-darwin` — plain `cargo build` output is NOT linked by the app.
- Deployment target macOS 14.0, Swift 5.9.

## Layout

```
core/                       Rust workspace (crate: tokenviewer-core)
  src/parsers/              one file per provider; mod.rs registers all_parsers()
    utils.rs                FileCursor, bucketing, jsonl/offset readers, aggregate_records
  src/storage/db.rs         SQLite schema + all queries (usage, sync_cursors)
  src/sync/scheduler.rs     sync_all(): runs parsers in parallel, upserts records + cursors
  src/ffi.rs                C ABI: tt_* functions (see below)
  src/pricing/              model → cost tables
  src/models.rs             UsageRecord, summaries
macos/TokenViewer/
  App/                      TokenViewerApp.swift (AppDelegate, menu-bar setup)
  Bridge/                   CoreBridge.swift + bridging header → calls tt_* FFI
  ViewModels/               UsageViewModel, LimitsViewModel (@MainActor, ObservableObject)
  Views/                    SwiftUI: PopoverView (panel), UsageView (dashboard),
                            LimitsView, SettingsView, TrendChartView, heatmap, etc.
  Services/                 LimitsService (live rate limits), Localization (L10n),
                            UpdateChecker, Preferences
  Resources/                Info.plist, entitlements, brand-logos/*.svg
website/                    React + Vite marketing site → builds into docs/
docs/                       GitHub Pages output (served from main /docs, CNAME tokenviewer.webkong.top)
  releases/vX.Y.Z.md        release notes (used by release.sh) — NOTE: lives inside vite outDir
script/release.sh           build-pkg / build-dmg / build-zip / push-release / push-website
signing/                    self-signed code-signing cert + passwords (gitignored)
```

## Rust ⟷ Swift FFI

`core/src/ffi.rs` exposes (all take/return C strings; free with `tt_free_string`):
`tt_init`, `tt_sync_all`, `tt_get_provider_status`, `tt_query_summary`,
`tt_query_daily`, `tt_query_hourly`, `tt_query_model_breakdown`, `tt_query_heatmap`,
`tt_free_string`, `tt_destroy`.
Swift wraps these in `CoreBridge`; JSON is exchanged across the boundary and decoded with `Codable`. When adding a query, add the Rust query in `db.rs`, expose a `tt_*` fn in `ffi.rs`, declare it in the bridging header, and wrap it in `CoreBridge`.

## Data model & conventions

- **Storage**: table `usage(hour_start, source, model, input/output/cache/... , total_tokens, conversation_count)` aggregated into **30-min buckets**; `sync_cursors(source, cursor_data)` holds per-source `FileCursor` JSON.
- **Timezone (critical)**: `hour_start` is stored in **UTC** (`...Z`). Day/hour grouping for charts & heatmap MUST convert to local time via `strftime('%Y-%m-%d', hour_start, 'localtime')` (see `db.rs`). Range bounds (from/to) are computed in Swift from the local calendar then formatted to UTC. Don't reintroduce `substr(hour_start,1,N)` for day/hour grouping — that buckets by UTC and misattributes local early-morning usage.
- **Idempotent parsing**: `FileCursor` (utils.rs) tracks `offsets` (byte offset for append-only jsonl), `seen_ids` (dedup, capped 50k), `snapshots` (cumulative-total deltas), `mtimes`/`dir_mtimes`/`dir_files` (skip-unchanged + glob cache). Re-running a parser must never double-count. Use `file_changed()` to skip unchanged files, `mark_seen()` to dedup, `delta()` for cumulative sources.
- **Token estimation**: some sources (Kiro CLI) only store char counts → estimate `tokens ≈ chars / 4`.

### Kiro is special (4 data sources, all → source `"kiro"`)
1. `…/Kiro/User/globalStorage/kiro.kiroagent/dev_data/devdata.sqlite` (IDE, stopped writing ~Feb 2026)
2. `…/dev_data/tokens_generated.jsonl` → model `"kiro-agent"` (IDE, no per-model info)
3. `~/Library/Application Support/kiro-cli/data.sqlite3` table `conversations_v2` (CLI history; char-based estimate, dedup by `request_id`)
4. `~/.kiro/sessions/cli/*.json` + `.jsonl` (active CLI sessions)

`normalize_kiro_model` preserves real versions (`claude-sonnet-4.6`, `claude-opus-4.8`, …). `kiro-agent` is the **fallback model bucket** for usage that can't be attributed to a specific model — keep it in the model breakdown, don't filter it out.

## Adding a new provider parser
1. Create `core/src/parsers/<name>.rs` with `pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>>`.
2. Reuse `utils.rs` helpers; produce 30-min-bucketed `UsageRecord`s with `source = "<name>"`.
3. Register in `parsers/mod.rs`: `pub mod <name>;` + add to `all_parsers()`.
4. Add pricing in `core/src/pricing/` if cost is needed.
5. Add a logo `macos/TokenViewer/Resources/brand-logos/<name>.svg` and map it in `Views/ProviderIcon.swift`.
6. `cargo test` + `bash run.sh`.

Reference (read-only) implementation of all providers lives in the sibling project `../TokenTracker/` (JS) — consult `src/lib/rollout.js` & `usage-limits.js` for source paths/formats.

## Code style
- Read existing code before writing; reuse helpers; keep changes minimal and scoped to the request.
- Rust: no new deps without reason (current: rusqlite bundled, serde, serde_json, chrono, glob, rayon).
- Swift: MVVM, `@MainActor` view models, query off-main then publish on main.
- i18n: all user-facing strings go through `L10n` (Services/Localization.swift), zh-CN + en. No hardcoded UI strings.
- Don't auto-generate docs/tests unless asked.

## Release (`script/release.sh`)
Version lives in **two** places — bump both: `macos/project.yml` (`MARKETING_VERSION`) and `script/version.env` (`VERSION`, `BUILD_NUMBER`).
```bash
# 1) write docs/releases/vX.Y.Z.md  2) bump versions  3) commit + tag
# 4) build artifacts (codesign needs keychain unlocked once per session):
set -a; source signing/tokenviewer-internal-codesign.env; set +a
security unlock-keychain -p "$SELF_SIGNED_KEYCHAIN_PASSWORD" "$SELF_SIGNED_KEYCHAIN_PATH"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$SELF_SIGNED_KEYCHAIN_PASSWORD" "$SELF_SIGNED_KEYCHAIN_PATH"
SKIP_RUST_BUILD=1 bash script/release.sh build-dmg   # → zip + dmg
SKIP_RUST_BUILD=1 bash script/release.sh build-pkg   # → pkg (recommended installer; postinstall strips quarantine)
# 5) publish
SKIP_DMG_BUILD=1 RELEASE_TAG="vX.Y.Z" bash script/release.sh push-release
```
- PKG filename includes the version (`TokenViewer-X.Y.Z-Installer.pkg`). Download links in `README.md`, `README.zh-CN.md`, `website/src/main.jsx` use `releases/latest/download/…` with that versioned filename — **update them on every version bump** or `latest/download` 404s.
- Website deploy: `npm run build` in `website/` outputs to `docs/`. ⚠️ Vite empties `docs/` on build, wiping `docs/releases/*.md` — restore them (`git checkout -- docs/releases`) after building. Pages serves from `main` `/docs`.

## Git
- Remotes: `github` = `webkong/TokenViewer` (public, the one that matters for releases/Pages), `origin` = self-hosted Gitea (`main` is branch-protected; force-push is rejected server-side).
- Only commit when asked. Never push to `main` directly unless asked. Tags `v0.1.0`, `v0.1.1` exist; latest = **v0.1.1**.
