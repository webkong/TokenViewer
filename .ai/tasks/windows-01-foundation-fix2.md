# Task: windows-01-foundation-fix2

## Goal

纠正实施 handoff 中与实际验证结果不一致的测试数量，保持交接证据准确。

## References

- 原任务：`task_4e97135b15c5`
- Fix1：`task_918b3ab56efc`
- Handoff：`.ai/handoffs/task_4e97135b15c5-implementation.md`

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `.ai/handoffs/task_4e97135b15c5-implementation.md` | yes | 把 Tests 中 `113 passed` 改为实际的 `114 passed` | 其他内容和任何源码 |

## Change budget

- 只允许修改一个 handoff 文件；不允许修改源码、测试、依赖或其他文档。

## Acceptance criteria

- Handoff 与 Worker/Reviewer 实际运行结果一致，写明 114 passed。
- `git diff --check` 通过。

## Required tests

- `rtk git diff --check`。

## Escalation triggers

- 发现除测试数量外的事实不一致。

## Do not change

- 所有源码和其他交接内容。

