<div align="center">

# TokenViewer

**English** · [简体中文](./README.zh-CN.md)

### Track your AI token usage across 22 tools — right from the menu bar

A native macOS menu-bar app that quietly collects token counts and costs from every AI coding tool you use, and shows you the full picture in a beautiful dashboard. No cloud, no account, no Node.js — just a tiny native app.

[![License: CC BY-NC 4.0](https://img.shields.io/badge/License-CC%20BY--NC%204.0-green.svg)](https://creativecommons.org/licenses/by-nc/4.0/)
[![Platform: macOS 14+](https://img.shields.io/badge/Platform-macOS%2014%2B-lightgrey?logo=apple&logoColor=white)](https://github.com/webkong/TokenViewer/releases/latest)
[![GitHub stars](https://img.shields.io/github/stars/webkong/TokenViewer?style=social)](https://github.com/webkong/TokenViewer/stargazers)

<br/>

<img src="website/public/screenshot/t1.png" alt="TokenViewer Dashboard" width="820" />

</div>

---

## ✨ Features

- 📊 **Dashboard** — Today / 7-day / 30-day / Total token counts and costs
- 💰 **Cost tracking** — Per-model pricing, multi-currency display
- 📈 **Usage trends** — Smooth daily/hourly charts with cache hit rate
- 🗓️ **Activity heatmap** — 53-week GitHub-style grid
- 🔒 **Limits** — Live quota bars for Codex, Copilot, Kiro, Cursor, Gemini, Kimi
- 🌍 **Chinese / English UI**
- 🏠 **100% local** — All data in `~/.tokenviewer/data.db` (SQLite). No cloud, no account.

## ⚡ Quick Start

1. [Download the latest DMG](https://github.com/webkong/TokenViewer/releases/latest)
2. Open the DMG and drag **TokenViewer.app** to `/Applications`
3. Launch — the T icon appears in your menu bar
4. Click the icon to see your usage summary
5. Data syncs automatically in the background

## 🛠️ Supported Tools (22)

| Tool | Data Source |
|------|------------|
| Claude Code | `~/.claude/projects/*.jsonl` |
| Codex | `~/.codex/sessions/**/rollout-*.jsonl` |
| Kiro CLI | `~/.kiro/sessions/cli/*.json` |
| Kiro IDE | `~/Library/.../kiro.kiroagent/dev_data/` |
| GitHub Copilot | `~/.copilot/otel/*.jsonl` |
| Cursor | `cursorDiskModel/usage.json` |
| Gemini CLI | `~/.gemini/tmp/*/chats/*.json` |
| Opencode | `~/.local/share/opencode/opencode.db` |
| Roocode | VSCode globalStorage `ui_messages.json` |
| Kilo Code | VSCode globalStorage |
| Zed | `~/Library/.../threads.db` |
| Goose | `~/Library/.../sessions.db` |
| Grok | `~/.grok/sessions/**/updates.jsonl` |
| Kimi | `~/.kimi/sessions/**/*.jsonl` |
| Craft | `~/.craft-agent/**/*.jsonl` |
| OpenClaw | `~/.openclaw/agents/**/*.jsonl` |
| Hermes | `~/.hermes/state.db` |
| Antigravity | `~/.gemini/antigravity*/` |
| CodeBuddy | `~/.codebuddy/**/*.jsonl` |
| OhMyPi | `~/.omp/agent/sessions/**/*.jsonl` |
| Pi | `~/.pi/agent/sessions/**/*.jsonl` |
| KiloCLI | `~/.local/share/kilo/kilo.db` |

## 🏗️ Build from Source

**Requirements:** macOS 14+, Xcode 16+, Rust (aarch64-apple-darwin), XcodeGen

```bash
git clone https://github.com/webkong/TokenViewer.git
cd TokenViewer/TokenViewerNew

# Build and run
./run.sh
```

Or step by step:
```bash
# 1. Build Rust core
cd core && cargo build --release --target aarch64-apple-darwin

# 2. Generate Xcode project
cd ../macos && xcodegen generate

# 3. Build app
xcodebuild -scheme TokenViewer -configuration Release build
```

## 🌐 Website

[tokenviewer.webkong.top](https://tokenviewer.webkong.top)

## 📜 License

[CC BY-NC 4.0](LICENSE) — Free for personal and non-commercial use.
Attribution required when redistributing or modifying.
Commercial use is not permitted.

## 🙏 Credits

Built by [webkong](https://github.com/webkong).
Inspired by [TokenTracker](https://github.com/mm7894215/TokenTracker).
