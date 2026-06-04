<div align="center">

# TokenViewer

[English](./README.md) · **简体中文**

### 清楚知道你在每个 AI 编程工具上花了多少 Token

一款用 **SwiftUI + Rust** 构建的原生 macOS 菜单栏应用，自动收集 **22 个 AI 编程工具**的 Token 用量和费用，在本地聚合后呈现在精美的仪表盘中。无需云端账号、无需 Node.js、无需浏览器——只是一个小巧的原生应用。

[![Release](https://img.shields.io/github/v/release/webkong/TokenViewer?color=059669&label=下载)](https://github.com/webkong/TokenViewer/releases/latest/download/TokenViewer-Installer.pkg)
[![Platform](https://img.shields.io/badge/平台-macOS%2014%2B-lightgrey?logo=apple&logoColor=white)](https://github.com/webkong/TokenViewer/releases/latest/download/TokenViewer-Installer.pkg)
[![License: CC BY-NC 4.0](https://img.shields.io/badge/协议-CC%20BY--NC%204.0-green.svg)](https://creativecommons.org/licenses/by-nc/4.0/)
[![GitHub stars](https://img.shields.io/github/stars/webkong/TokenViewer?style=social)](https://github.com/webkong/TokenViewer/stargazers)

<br/>

<img src="https://raw.githubusercontent.com/webkong/TokenViewer/main/website/public/screenshot/t1.png" alt="TokenViewer 仪表盘" width="820" />

<br/>

⭐ **如果 TokenViewer 帮你省了钱，请 [在 GitHub 给它 star](https://github.com/webkong/TokenViewer) — 这能帮助其他开发者发现它。**

</div>

---

## ⚡ 快速开始

> **环境要求**：macOS 14（Sonoma）或更高版本，Apple Silicon 或 Intel。

1. [**下载最新 DMG**](https://github.com/webkong/TokenViewer/releases/latest/download/TokenViewer-Installer.pkg)
2. 打开 DMG，将 **TokenViewer.app** 拖入 `/Applications`
3. 启动——菜单栏出现 **T** 图标
4. 点击图标即可查看用量摘要
5. 点击 Dashboard 打开完整视图

**你会立即得到：**
- 📊 今天 / 7 天 / 30 天 / 总计的 Token 数量和费用估算
- 📈 每日趋势图，含缓存命中率和费用叠加线
- 🗓️ 53 周活跃度热力图
- 🔒 8 个工具的实时配额进度条（Codex、Copilot、Kiro、Cursor、Gemini、Kimi……）
- 🏠 100% 本地——所有数据存储在 `~/.tokenviewer/data.db`，无账号，无 API Key

> **初次打开提示「无法打开」？**
>
> 应用使用 ad-hoc 签名（未通过 Apple 公证，公证需要付费开发者账号）。Gatekeeper 会在首次启动时拦截。
> 1. 前往**系统设置 → 隐私与安全性**
> 2. 在**安全性**部分找到「TokenViewer 已被阻止」→ 点击**仍要打开**
> 3. 在弹窗中确认**打开**
>
> 只需操作一次。或者在 Finder 中右键点击应用 → **打开** → **打开**。

---

## ✨ 功能特性

- 🔌 **开箱支持 22 个工具** — Claude Code、Codex、Kiro、Cursor、GitHub Copilot、Gemini CLI、Opencode、Roocode、Kilo Code、Zed、Goose、Grok、Kimi、Craft、OpenClaw、Hermes、Antigravity、CodeBuddy、OhMyPi、Pi、KiloCLI、EveryCode
- 🏠 **100% 本地** — Token 数据绝不离开你的设备，无账号，无 API Key
- 🚀 **零配置** — 读取工具已有的日志文件，无需在这些工具中安装任何东西
- 📊 **精美仪表盘** — 用量趋势、模型分布、费用分析、工具分布、每日明细表
- 🖥️ **原生应用** — SwiftUI + Rust 内核，无 Electron，无 Node.js，无浏览器，极速启动
- 📈 **平滑趋势图** — Catmull-Rom 样条曲线，hover 显示输入 / 输出 / 缓存 / 推理 / 费用
- 🗓️ **活跃度热力图** — 仪表盘和菜单栏面板均有 53 周 GitHub 风格日历格
- 📉 **限额监控** — 8 个工具的实时配额进度条，含重置倒计时
- 💰 **费用引擎** — 主流工具按模型精确定价，支持多币种显示（实时汇率）
- 🌍 **中文 / 英文界面** — 在设置中随时切换
- ⚡ **增量同步** — 文件偏移量游标 + mtime 跳过 + rayon 并行解析，无变化时重复同步 <100ms
- 🔒 **隐私优先** — 只采集 Token 数量和时间戳，永远不会读取提示词、响应内容或文件内容

---

## 🖼️ 截图展示

<table>
<tr>
<td width="50%">

**菜单栏面板** — 快速摘要、限额、趋势

<img src="https://raw.githubusercontent.com/webkong/TokenViewer/main/website/public/screenshot/t2.png" alt="菜单栏面板" />

</td>
<td width="50%">

**完整仪表盘** — 详细用量视图

<img src="https://raw.githubusercontent.com/webkong/TokenViewer/main/website/public/screenshot/t3.png" alt="仪表盘" />

</td>
</tr>
</table>

---

## 🔌 支持的工具（22 个）

| 工具 | 数据来源 | 方式 |
|------|---------|------|
| **Claude Code** | `~/.claude/projects/*.jsonl` | 被动读取 |
| **Codex** | `~/.codex/sessions/**/rollout-*.jsonl` | 被动读取 |
| **Kiro CLI** | `~/.kiro/sessions/cli/*.json` + `.jsonl` | Session 文件 + 字符估算 |
| **Kiro IDE** | `~/Library/.../kiro.kiroagent/dev_data/` | SQLite + JSONL |
| **GitHub Copilot** | `~/.copilot/otel/*.jsonl` | OpenTelemetry 文件读取 |
| **Cursor** | `cursorDiskModel/usage.json` | 被动读取 |
| **Gemini CLI** | `~/.gemini/tmp/*/chats/session-*.json` | 被动读取 |
| **Opencode** | `~/.local/share/opencode/opencode.db` | SQLite 读取 |
| **Roocode** | VSCode globalStorage `tasks/*/ui_messages.json` | 被动读取 |
| **Kilo Code** | VSCode globalStorage | 被动读取 |
| **Zed** | `~/Library/.../threads.db` | SQLite 读取 |
| **Goose** | `~/Library/.../sessions.db` | SQLite 读取 |
| **Grok** | `~/.grok/sessions/**/updates.jsonl` | 被动读取 |
| **Kimi** | `~/.kimi/sessions/**/*.jsonl` | 被动读取 |
| **Craft** | `~/.craft-agent/**/*.jsonl` | 被动读取 |
| **OpenClaw** | `~/.openclaw/agents/**/*.jsonl` | 被动读取 |
| **Hermes** | `~/.hermes/state.db` | SQLite 读取 |
| **Antigravity** | `~/.gemini/antigravity*/brain/**/transcript.jsonl` | 被动读取 |
| **CodeBuddy** | `~/.codebuddy/**/*.jsonl` | 被动读取 |
| **OhMyPi** | `~/.omp/agent/sessions/**/*.jsonl` | 被动读取 |
| **Pi** | `~/.pi/agent/sessions/**/*.jsonl` | 被动读取 |
| **KiloCLI** | `~/.local/share/kilo/kilo.db` | SQLite 读取 |

> 所有集成均为**被动读取**——TokenViewer 只读取这些工具已有的文件，不向工具安装任何东西。

没有你的工具？[提 issue](https://github.com/webkong/TokenViewer/issues/new)——增加一个新的解析器通常只需一个文件。

---

## 📊 限额监控

TokenViewer 可以显示 8 个工具的实时配额用量和重置倒计时：

| 工具 | 档位显示 | 数据来源 |
|------|---------|---------|
| **Claude** | Max / Pro | Anthropic OAuth API（Keychain） |
| **Codex** | Plus / Team / Pro | ChatGPT wham API（`~/.codex/auth.json`） |
| **GitHub Copilot** | Individual / Business | GitHub API（`~/.config/github-copilot/`） |
| **Kiro** | Free / Pro / Pro+ / Power | `kiro-cli /usage` |
| **Cursor** | Pro / Team / Enterprise | `cursor.com/api/usage-summary` |
| **Gemini** | — | `cloudcode-pa.googleapis.com` |
| **Kimi** | Adagio / Andante / Moderato / Allegretto / Allegro | `api.kimi.com/coding/v1/usages` |
| **Antigravity** | — | 使用 Gemini 配额 |

未安装的工具显示为置灰卡片（已配置工具排在前面）。

---

## 🏗️ 工作原理

```
AI 工具日志  →  22 个 Rust 解析器  →  SQLite (~/.tokenviewer/data.db)
                                              ↓
                                 FFI (tt_sync / tt_query_*)
                                              ↓
                        SwiftUI 视图 ← CoreBridge ← UsageViewModel
```

- **Rust 核心** (`core/`) — 22 个解析器、SQLite 存储、定价引擎，编译为 `libtokenviewer_core.a`
- **SwiftUI 应用** (`macos/`) — 菜单栏状态图标、弹出面板、完整仪表盘窗口
- **增量同步** — 每个解析器在 SQLite 中存储文件偏移量游标。下次同步只读新增字节。解析器并行运行（rayon）。mtime 未变的文件完全跳过。

---

## 🛡️ 隐私说明

| 保护 | 说明 |
|------|------|
| **不采集内容** | 只采集 Token 数量、模型名称和时间戳，永远不读取提示词、响应内容或文件内容 |
| **完全本地** | 所有数据保存在 `~/.tokenviewer/data.db`，除可选汇率获取和更新检查外无网络请求 |
| **无遥测** | 无分析统计，无崩溃上报，无后台联网 |
| **可审计** | 完全开源，Rust 解析器只提取数值字段，见 `core/src/parsers/` |

---

## 🏗️ 从源码构建

**环境要求**：macOS 14+、Xcode 16+、Rust（aarch64-apple-darwin 目标）、XcodeGen

```bash
git clone https://github.com/webkong/TokenViewer.git
cd TokenViewer/TokenViewerNew

# 一键构建并运行
./run.sh
```

分步执行：

```bash
# 1. 安装 Rust 目标
rustup target add aarch64-apple-darwin

# 2. 构建 Rust 核心
cd core
cargo build --release --target aarch64-apple-darwin

# 3. 生成 Xcode 项目
cd ../macos
xcodegen generate

# 4. 构建并运行
xcodebuild -scheme TokenViewer -configuration Release build
open ~/Library/Developer/Xcode/DerivedData/TokenViewer-*/Build/Products/Release/TokenViewer.app
```

### 发布构建（DMG）

```bash
brew install create-dmg gh
./script/release.sh build-dmg     # → dist/release/TokenViewer.dmg
./script/release.sh push-release  # → GitHub Release（需要 gh auth login）
./script/release.sh build-website # → website/dist
./script/release.sh push-website  # → gh-pages
```

---

## 🔧 常见问题

<details>
<summary><b>首次启动没有数据</b></summary>

点击菜单栏面板或仪表盘右上角的 **↻** 同步按钮。数据在手动触发或自动同步定时器触发（默认每 30 分钟）之前不会同步。

确认你使用的工具在[支持列表](#-支持的工具22-个)中，并且已经使用过至少一次。

</details>

<details>
<summary><b>Kiro Token 数量偏少</b></summary>

Kiro CLI 的 session `.json` 文件中 `input_token_count` / `output_token_count` 字段通常为 `0`——这是 kiro-cli 的设计决定，不存储真实 Token 数。TokenViewer 通过消息字符数 ÷ 4 估算（与 TokenTracker 相同的方法）。

Kiro IDE 的数据来自 `tokens_generated.jsonl`，单独存储在 `kiro-ide` 数据源下。

</details>

<details>
<summary><b>限额页面显示「未配置」</b></summary>

限额获取需要本地身份认证状态：
- **Kiro** — 需要通过 `kiro-cli` 登录（运行 `kiro-cli chat /usage` 验证）
- **Cursor** — 需要 Cursor App 生成的 `state.vscdb`
- **Gemini** — 需要 `~/.gemini/oauth_creds.json`（运行 `gemini` 完成身份验证）
- **Kimi** — 需要 `~/.kimi/credentials/kimi-code.json`

</details>

<details>
<summary><b>今天的数据显示为零</b></summary>

TokenViewer 使用本地时区将「今天」转换为 UTC 范围。如果你在 UTC+8（北京时间），「今天」从上一个自然日的 UTC 16:00 开始。触发一次手动同步，数据应该会出现。

</details>

---

## 🤝 参与贡献

- **Bug / 功能建议**：[提 issue](https://github.com/webkong/TokenViewer/issues/new)
- **新工具解析器**：在 `core/src/parsers/` 添加一个文件，在 `mod.rs` 和 `scheduler.rs` 中注册，参考现有解析器的写法
- **Pull Request**：fork → 新建分支 → PR，请描述改动内容和原因

---

## 📜 开源协议

[CC BY-NC 4.0](LICENSE) — 个人和非商业用途免费使用。  
**再分发或修改时须注明出处。**  
**不允许商业用途。**

---

## ⭐ Star History

<a href="https://star-history.com/#webkong/TokenViewer&Date">
  <img src="https://api.star-history.com/svg?repos=webkong/TokenViewer&type=Date" alt="Star History Chart" width="600" />
</a>

---

## 🗺️ 路线图

- [ ] **Windows 支持** — 系统托盘应用，原生 Win32 界面（计划中）
- [ ] Homebrew Cask 公式

---

## 🙏 致谢

由 [webkong](https://github.com/webkong) 构建。  
灵感来自 [TokenTracker](https://github.com/mm7894215/TokenTracker)。

---

<div align="center">

**TokenViewer** — 清楚知道你在 AI 上的花费。

[tokenviewer.webkong.top](https://tokenviewer.webkong.top) · [GitHub](https://github.com/webkong/TokenViewer) · [发布页](https://github.com/webkong/TokenViewer/releases)

</div>
