# Token Viewer (Native)

A lightweight, native macOS menu bar app for tracking AI token usage across 22 coding tools. Built with Rust core + SwiftUI — no Node.js, no WebView.

## Architecture

```
core/       Rust shared library (parsers + SQLite + pricing engine)
macos/      SwiftUI macOS app (menu bar + popover + main window)
```

## Supported AI Tools (22)

Claude Code, Codex CLI, Cursor, Gemini CLI, Kiro, OpenCode, OpenClaw, Every Code, Hermes, GitHub Copilot, Kimi Code, Grok Build, Antigravity, Roo Code, Kilo Code, Kilo CLI, Zed Agent, Goose, oh-my-pi, pi, Craft Agents, CodeBuddy

## Build

### Prerequisites

- Rust toolchain (`rustup`)
- Xcode 16+ with Command Line Tools
- XcodeGen (`brew install xcodegen`)

### Steps

```bash
# 1. Build Rust core
./build-rust.sh

# 2. Generate Xcode project
cd macos && xcodegen generate

# 3. Open and build
open TokenViewer.xcodeproj
# Cmd+R to build & run
```

## Data Storage

All data stored locally at `~/.tokenviewer/data.db` (SQLite).

## Features

- **Menu bar icon** with quick-glance popover (today/week stats)
- **Usage page** — time range selector, token/cost totals, daily trend chart, model breakdown
- **Settings page** — launch at login, sync frequency, data directory
- **22 provider parsers** — auto-detect and parse logs from all supported tools
- **Pricing engine** — built-in model pricing for accurate cost calculation
- **~6MB app bundle** (vs ~100MB with embedded Node.js)
