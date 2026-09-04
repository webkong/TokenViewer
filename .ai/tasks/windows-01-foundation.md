# Task: windows-01-foundation

## Goal

让 Rust core 在 Windows 上从经过验证且可测试的路径发现 Kiro/Kilo CLI/Mimocode/OpenCode/Codex Home，并让 Windows P/Invoke 使用 UTF-8；扩展现有 CI 对 Rust 测试和 Windows 发布产物进行基础校验。

## Current behavior

- `kiro.rs` 把所有非 macOS 平台当成 Linux；`kilocli.rs`、`mimocode.rs`、`opencode.rs` 固定使用 `~/.local/share`。
- `codex_home.rs` 的 known hosts 和广域扫描根仅适合 macOS。
- `windows/TokenViewer/Interop/CoreBridge.cs::tt_init` 使用 `CharSet.Ansi`，非 ASCII 用户路径可能失败。
- `.github/workflows/windows-build.yml` 已存在，但只执行发布脚本，未先运行 Rust 测试，也未断言 DLL 存在。

## Desired behavior

- Windows 路径按设计文档的 Roaming/Local 语义和候选优先级解析；测试可注入临时根目录，不读取真实用户 profile。
- Kiro 的 IDE、settings、legacy CLI DB、profile sessions 分别解析；现有 macOS/Linux 路径不回归。
- Codex Home 保留 `CODEX_HOME`、`~/.codex` 和窄范围扫描规则，并增加 Windows roots。
- 所有 C#→Rust 字符串均显式 UTF-8 marshaling。
- Windows CI 先运行 Rust tests，再构建/发布并断言 `tokenviewer_core.dll`。

## Reuse evidence

- 路径分支模式：`core/src/parsers/utils.rs::vscode_global_storage`、`core/src/parsers/cursor.rs::parse`、`core/src/parsers/goose.rs::parse`。
- 候选发现/去重模式：`core/src/codex_home.rs::add_candidate`、`scan_for_codex_homes`、现有 tests。
- 幂等模式：`core/src/parsers/utils.rs::FileCursor`，不得改变 cursor JSON。
- CI/打包：`.github/workflows/windows-build.yml` 与 `script/windows-release.ps1`，扩展而非另建 workflow。

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `core/src/parsers/utils.rs` | yes | 增加小型、可注入的 Windows base-dir/path candidate helper 及单测 | 改 FileCursor/聚合语义 |
| `core/src/parsers/kiro.rs` | yes | 独立 Windows IDE/settings/CLI candidates | 改 token 估算、模型归一化、去重 |
| `core/src/parsers/kilocli.rs` | yes | Windows LocalAppData candidates | 改数据库解析逻辑 |
| `core/src/parsers/mimocode.rs` | yes | Windows LocalAppData candidates | 改 token total 规则 |
| `core/src/parsers/opencode.rs` | yes | Windows LocalAppData candidates | 改 mark_seen/记录模型 |
| `core/src/codex_home.rs` | yes | Windows known roots/scan roots 与测试 | 扩大为磁盘全盘扫描 |
| `windows/TokenViewer/Interop/CoreBridge.cs` | yes | 所有输入 string 改为 UTF-8 marshalling | 增加本阶段无关查询/UI |
| `.github/workflows/windows-build.yml` | yes | Rust tests、构建/产物断言 | 新建第二个 workflow、自动发布 |
| `script/windows-release.ps1` | yes，仅必要时 | 让 CI 产物路径可被可靠断言 | 改发布文件名/自动推送 |

## Change budget

- 允许修改文件：仅上表 9 个文件。
- 允许新增文件：none。
- 允许新增依赖：none。
- 允许变更公共 FFI、持久化结构：no。
- 允许移动/重命名模块：no。

## Invariants

- macOS/Linux 当前路径和解析结果保持不变。
- `home_dir`/测试注入根仍然是解析器的事实来源；`dirs::*` 不得让测试落到真实目录。
- 已存在的 Windows cursor/zed/goose/vscode 路径不做无关修改。
- CI 保留 PR、main push、workflow_dispatch 和 `core/**` 广义触发。

## Acceptance criteria

- Given Windows Roaming/Local/Profile 临时目录，When 调用路径 helper/parser，Then 首个存在候选被使用且不存在时返回空记录而非报错。
- Given 非 ASCII DB 路径，When .NET 调用 `tt_init`，Then 声明使用 UTF-8，不经过 ANSI codepage。
- Given 原有 macOS/Linux tests，When 执行全量 Rust tests，Then 全部通过且 cursor fixture 不变化。
- Given Windows CI，When workflow 运行，Then Rust tests 在发布前执行，发布目录包含 `tokenviewer_core.dll`。

## Required tests

- `PATH="/opt/homebrew/opt/rustup/bin:$PATH" rtk cargo test --lib --tests`（在 `core/`）。
- `rtk cargo fmt --check`（在 `core/`）。
- `rtk git diff --check`。
- Windows-only build/publish 由 workflow 验证；在 macOS 不声称 WPF 已运行。

## Escalation triggers

- 真实应用路径与设计候选冲突且需要新增不同目录。
- 必须修改 parser registry、数据库 schema 或 cursor 格式。
- UTF-8 marshalling 需要改变 Rust ABI。
- Windows `git2` 构建确实要求引入/更改 OpenSSL 依赖。

## Do not change

- UI、limits、settings、skills、pricing、release 版本及任何无关格式。

