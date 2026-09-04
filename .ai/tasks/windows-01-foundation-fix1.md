# Task: windows-01-foundation-fix1

## Goal

修正第一阶段 Review 发现的 Kiro Windows 单路径问题，恢复最小 diff，并补齐实施 handoff。

## References

- 原任务：`task_4e97135b15c5` / `.ai/tasks/windows-01-foundation.md`
- Review：`/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_4e97135b15c5.md`
- 最新设计：`/Users/wangsw/webkong/TokenViewer/tokenviewer/docs/superpowers/specs/2026-08-13-windows-ui-port-design.md`

## Desired behavior

- Kiro IDE dev_data、Kiro settings、Kiro CLI legacy DB 在 Windows 上都通过有序候选选择首个存在路径。
- 首选仍为 `%APPDATA%/Kiro/...`（IDE/settings）和 `%LOCALAPPDATA%/kiro-cli/data.sqlite3`（CLI DB），并有显式 compatibility fallback。
- macOS/Linux 路径、Kiro session profile 路径、解析/幂等/token 语义不变。

## Reuse evidence

- `core/src/parsers/utils.rs::first_existing/local_data_candidates/resolve_local_data_path`。
- 已实施的 `core/src/parsers/kiro.rs` platform branches。

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `core/src/parsers/utils.rs` | yes，仅必要时 | 可复用的小型 candidate resolver/test | FileCursor/其他 parser 行为 |
| `core/src/parsers/kiro.rs` | yes | 三类有序候选、测试、撤销无关格式 diff | token/model/cursor 语义 |
| `.ai/handoffs/task_4e97135b15c5-implementation.md` | add | 完整实施 handoff | 省略失败/限制 |

## Change budget

- 只允许上表文件；无新依赖、无公共 API/schema 改动、无移动/重命名。

## Invariants

- 保留第一轮已通过 Review 的其他修改，不重写或扩展。
- 不读取真实用户 profile 进行测试。
- 不猜测更多未证实路径；fallback 必须是设计或既有兼容路径。

## Acceptance criteria

- 三类 Kiro Windows path 均为 ordered candidates，而不是单一路径。
- 首个存在候选被使用；候选均不存在时 parser 安全返回空数据。
- 无关的 normalize model 格式 diff 消失。
- handoff 包含文件、实现、测试、限制、风险和 Review 重点。

## Required tests

- `OPENSSL_DIR=/opt/homebrew/opt/openssl@3 PATH="/opt/homebrew/opt/rustup/bin:$PATH" rtk cargo test --lib --tests --target aarch64-apple-darwin`（`core/`）。
- `rtk git diff --check`。

## Escalation triggers

- 必须修改列表外文件或引入依赖。
- compatibility fallback 无法从现有设计/代码得出。

## Do not change

- 第一轮其他已通过内容、UI、CI、FFI、其他 parsers 和无关格式。

