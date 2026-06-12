<div align="center">

# TokenViewer

**English** · [简体中文](./README.zh-CN.md)

### Know exactly how many tokens you're spending — across every AI coding tool

A **native macOS menu-bar app** built with SwiftUI + Rust that auto-collects token counts and costs from **24 AI coding tools**, aggregates them locally, and surfaces real usage trends in a beautiful dashboard. No cloud account, no Node.js, no browser — just a tiny native app.

[![Release](https://img.shields.io/github/v/release/webkong/TokenViewer?color=059669&label=Download)](https://github.com/webkong/TokenViewer/releases/latest/download/TokenViewer.dmg)
[![Platform](https://img.shields.io/badge/Platform-macOS%2014%2B-lightgrey?logo=apple&logoColor=white)](https://github.com/webkong/TokenViewer/releases/latest/download/TokenViewer.dmg)
[![License: CC BY-NC 4.0](https://img.shields.io/badge/License-CC%20BY--NC%204.0-green.svg)](https://creativecommons.org/licenses/by-nc/4.0/)
[![GitHub stars](https://img.shields.io/github/stars/webkong/TokenViewer?style=social)](https://github.com/webkong/TokenViewer/stargazers)

<br/>

<img src="https://raw.githubusercontent.com/webkong/TokenViewer/main/website/public/screenshot/t1.png" alt="TokenViewer Dashboard" width="820" />

<br/>

⭐ **If TokenViewer saves you money, please [star it on GitHub](https://github.com/webkong/TokenViewer) — it helps other developers find it.**

</div>

---

## ⚡ Quick Start

> **Requirements**: macOS 14 (Sonoma) or later, Apple Silicon or Intel.

1. [**Download the latest DMG**](https://github.com/webkong/TokenViewer/releases/latest/download/TokenViewer.dmg)
2. Open the DMG and drag **TokenViewer.app** to `/Applications`
3. Launch — the **T** icon appears in your menu bar
4. Click the icon for an instant usage summary
5. Open **Dashboard** for the full view

**What you get immediately:**
- 📊 Today / 7-day / 30-day / Total token counts and cost estimates
- 📈 Daily trend chart with cache hit rate and cost overlay
- 🗓️ 53-week activity heatmap
- 🔒 Live quota bars for 8 providers (Codex, Copilot, Kiro, Cursor, Gemini, Kimi …)
- 🏠 100% local — all data in `~/.tokenviewer/data.db`. No account, no API keys.

> **"TokenViewer.app can't be opened" — first-launch Gatekeeper warning**
>
> The app is ad-hoc signed (not notarized — that requires a paid Apple Developer ID account).
> 1. Go to **System Settings → Privacy & Security**
> 2. Scroll to the **Security** section → click **Open Anyway**
> 3. Confirm **Open** in the follow-up dialog
>
> You only need to do this once. Alternatively, right-click the app → **Open** → **Open**.

---

## ✨ Features

- 🔌 **24 AI tools out of the box** — Claude Code, Codex, Kiro, Cursor, GitHub Copilot, Gemini CLI, Opencode, Roocode, Kilo Code, Zed, Goose, Grok, Kimi, Craft, OpenClaw, Hermes, Antigravity, CodeBuddy, WorkBuddy, OhMyPi, Pi, KiloCLI, EveryCode, MiMoCode
- 🏠 **100% local** — Token data never leaves your machine. No account, no API keys.
- 🚀 **Zero config** — Reads existing log files your tools already produce. Nothing installed into those tools.
- 📊 **Beautiful dashboard** — Usage trends, model breakdown, cost analysis, provider breakdown, daily detail table
- 🖥️ **Native app** — SwiftUI + Rust core. No Electron, no Node.js, no browser. Instant launch.
- 📈 **Smooth trend chart** — Catmull-Rom spline with hover tooltip showing input / output / cache / reasoning / cost
- 🗓️ **Activity heatmap** — 53-week GitHub-style grid in both the dashboard and the menu-bar panel
- 📉 **Rate limit tracking** — Live quota bars with reset countdowns for 8 providers: Codex, GitHub Copilot, Kiro (Free / Pro / Pro+ / Power), Cursor, Gemini, Kimi, Claude, Antigravity
- 💰 **Cost engine** — Per-model pricing for all major providers + Kiro, Cursor, Kimi overrides. Multi-currency display (live exchange rates).
- 🌍 **Chinese / English UI** — Switch in Settings at any time
- ⚡ **Incremental sync** — File offset cursors + mtime skip + rayon parallel parsing keep repeated syncs under 100 ms when nothing changed
- 🔒 **Privacy-first** — Only token counts and timestamps. Never prompts, responses, or file contents.

---

## 🖼️ Screenshots

<table>
<tr>
<td width="50%">

**Menu Bar Panel** — quick summary, limits, trend

<img src="https://raw.githubusercontent.com/webkong/TokenViewer/main/website/public/screenshot/t2.png" alt="Menu Bar Panel" />

</td>
<td width="50%">

**Dashboard** — full usage view

<img src="https://raw.githubusercontent.com/webkong/TokenViewer/main/website/public/screenshot/t3.png" alt="Dashboard" />

</td>
</tr>
</table>

---

## 🔌 Supported AI Tools

| Tool | Data Source | Method |
|------|-------------|--------|
| **Claude Code** | `~/.claude/projects/*.jsonl` | Passive reader |
| **Codex** | `~/.codex/sessions/**/rollout-*.jsonl` | Passive reader |
| **Kiro CLI** | `~/.kiro/sessions/cli/*.json` + `.jsonl` | Session file reader + char-based token estimate |
| **Kiro IDE** | `~/Library/.../kiro.kiroagent/dev_data/` | SQLite + JSONL |
| **GitHub Copilot** | `~/.copilot/otel/*.jsonl` | OpenTelemetry file reader |
| **Cursor** | `cursorDiskModel/usage.json` | Passive reader |
| **Gemini CLI** | `~/.gemini/tmp/*/chats/session-*.json` | Passive reader |
| **Opencode** | `~/.local/share/opencode/opencode.db` | SQLite reader |
| **Roocode** | VSCode globalStorage `tasks/*/ui_messages.json` | Passive reader |
| **Kilo Code** | VSCode globalStorage | Passive reader |
| **Zed** | `~/Library/.../threads.db` | SQLite reader |
| **Goose** | `~/Library/.../sessions.db` | SQLite reader |
| **Grok** | `~/.grok/sessions/**/updates.jsonl` | Passive reader |
| **Kimi** | `~/.kimi/sessions/**/*.jsonl` | Passive reader |
| **Craft** | `~/.craft-agent/**/*.jsonl` | Passive reader |
| **OpenClaw** | `~/.openclaw/agents/**/*.jsonl` | Passive reader |
| **Hermes** | `~/.hermes/state.db` | SQLite reader |
| **Antigravity** | `~/.gemini/antigravity*/brain/**/transcript.jsonl` | Passive reader |
| **CodeBuddy** | `~/.codebuddy/**/*.jsonl` | Passive reader |
| **WorkBuddy** | `~/.antigravity_cockpit/workbuddy_accounts/*.json` | Local quota snapshot reader |
| **OhMyPi** | `~/.omp/agent/sessions/**/*.jsonl` | Passive reader |
| **Pi** | `~/.pi/agent/sessions/**/*.jsonl` | Passive reader |
| **KiloCLI** | `~/.local/share/kilo/kilo.db` | SQLite reader |
| **MiMoCode** | `~/.local/share/mimocode/mimocode.db` | SQLite reader (message table) |

> All integrations are **passive readers** — TokenViewer only reads files your tools already produce. Nothing is installed into those tools.

Missing your tool? [Open an issue](https://github.com/webkong/TokenViewer/issues/new) — adding a new parser is usually one file.

---

## 📊 Rate Limit Tracking

TokenViewer can show live quota usage and reset countdowns for 9 providers:

| Provider | Plan Labels | Data Source |
|----------|-------------|-------------|
| **Claude** | Max / Pro | Anthropic OAuth API (Keychain) |
| **Codex** | Plus / Team / Pro | ChatGPT wham API (`~/.codex/auth.json`) |
| **GitHub Copilot** | Individual / Business | GitHub API (`~/.config/github-copilot/`) |
| **Kiro** | Free / Pro / Pro+ / Power | `kiro-cli /usage` |
| **Cursor** | Pro / Team / Enterprise | `cursor.com/api/usage-summary` (SQLite session token) |
| **Gemini** | — | `cloudcode-pa.googleapis.com` (`~/.gemini/oauth_creds.json`) |
| **Kimi** | Adagio / Andante / Moderato / Allegretto / Allegro | `api.kimi.com/coding/v1/usages` |
| **Antigravity** | — | Uses Gemini quota |
| **WorkBuddy** | Pro / Free / Enterprise | `codebuddy.cn` billing APIs (`workbuddy-desktop.info`) |

Providers not installed show as greyed-out cards (configured providers sort to the top).

---

## 🏗️ How It Works

```
AI Tool Logs  →  23 Rust Parsers  →  SQLite (~/.tokenviewer/data.db)
                                          ↓
                              FFI (tt_sync / tt_query_*)
                                          ↓
                     SwiftUI Views ← CoreBridge ← UsageViewModel
```

- **Rust core** (`core/`) — 23 parsers, SQLite storage, pricing engine. Compiled to `libtokenviewer_core.a`.
- **SwiftUI app** (`macos/`) — Menu-bar status item, popover panel, full dashboard window.
- **Incremental sync** — Each parser stores a file offset cursor in SQLite. On the next sync only new bytes are read. Parsers run in parallel (rayon). Files with unchanged mtime are skipped entirely.

---

## 🛡️ Privacy

| Protection | Detail |
|------------|--------|
| **No content captured** | Only token counts, model names, and timestamps. Never prompts, responses, or file contents. |
| **Local-only** | All data stays in `~/.tokenviewer/data.db`. No network calls except optional exchange rate fetch and update check. |
| **No telemetry** | No analytics, no crash reporting, no phone-home. |
| **Auditable** | Open source. The Rust parsers only extract numeric fields — see `core/src/parsers/`. |

---

## 🏗️ Build from Source

**Requirements:** macOS 14+, Xcode 16+, Rust + aarch64-apple-darwin target, XcodeGen

```bash
git clone https://github.com/webkong/TokenViewer.git
cd TokenViewer/TokenViewerNew

# One-shot build + run
./run.sh
```

Step by step:

```bash
# 1. Install Rust target
rustup target add aarch64-apple-darwin

# 2. Build Rust core
cd core
cargo build --release --target aarch64-apple-darwin

# 3. Generate Xcode project
cd ../macos
xcodegen generate

# 4. Build and run
xcodebuild -scheme TokenViewer -configuration Release build
open ~/Library/Developer/Xcode/DerivedData/TokenViewer-*/Build/Products/Release/TokenViewer.app
```

### Release build (DMG)

```bash
brew install create-dmg gh
./script/release.sh build-dmg     # → dist/release/TokenViewer.dmg
./script/release.sh push-release  # → GitHub Release (requires gh auth login)
./script/release.sh build-website # → website/dist
./script/release.sh push-website  # → gh-pages
```

---

## 🔧 Troubleshooting

<details>
<summary><b>No data showing after first launch</b></summary>

Click the **sync** button (↻) in the menu-bar panel or Dashboard. Data is not synced until you explicitly trigger it or the auto-sync timer fires (default: every 30 min).

Check that your AI tools are on the [supported list](#-supported-ai-tools) and have been used at least once.

</details>

<details>
<summary><b>Kiro token count seems low</b></summary>

Kiro CLI sessions store turns in `.json` files. Token counts (`input_token_count` / `output_token_count`) are `0` by design in kiro-cli — the app estimates tokens from message character counts (÷ 4 chars/token, same as TokenTracker).

Kiro IDE data comes from `tokens_generated.jsonl` and is stored separately under the `kiro-ide` source.

</details>

<details>
<summary><b>Limits page shows "Not configured" for a provider</b></summary>

The rate-limit fetch requires local authentication state:
- **Kiro** — must be logged in via `kiro-cli` (run `kiro-cli chat /usage` to verify)
- **Cursor** — needs `state.vscdb` from the Cursor app
- **Gemini** — needs `~/.gemini/oauth_creds.json` (run `gemini` to authenticate)
- **Kimi** — needs `~/.kimi/credentials/kimi-code.json`

</details>

<details>
<summary><b>Today's data shows zero</b></summary>

TokenViewer converts "today" to UTC using your local timezone. If you're in UTC+8 (CST), "today" starts at UTC 16:00 the previous calendar day. Trigger a manual sync — the data should appear.

</details>

---

## 🤝 Contributing

- **Bugs / feature requests**: [open an issue](https://github.com/webkong/TokenViewer/issues/new)
- **New tool parser**: add a file in `core/src/parsers/`, register it in `mod.rs` and `scheduler.rs`. See existing parsers for reference.
- **Pull requests**: fork → branch → PR. Please describe what you changed and why.

---

## 📜 License

[CC BY-NC 4.0](LICENSE) — Free for personal and non-commercial use.  
**Attribution required** when redistributing or modifying.  
**Commercial use is not permitted.**

---

## ⭐ Star History

<a href="https://star-history.com/#webkong/TokenViewer&Date">
  <img src="https://api.star-history.com/svg?repos=webkong/TokenViewer&type=Date" alt="Star History Chart" width="600" />
</a>

---

## 🗺️ Roadmap

- [ ] **Windows support** — system-tray app with native Win32 UI (planned)
- [ ] Homebrew Cask formula

---

## 🙏 Credits

Built by [webkong](https://github.com/webkong).  
Inspired by [TokenTracker](https://github.com/mm7894215/TokenTracker).

---

<div align="center">

**TokenViewer** — Know exactly what you're spending on AI.

[tokenviewer.webkong.top](https://tokenviewer.webkong.top) · [GitHub](https://github.com/webkong/TokenViewer) · [Releases](https://github.com/webkong/TokenViewer/releases)

</div>
