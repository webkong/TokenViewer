# Task: windows-05-limits-parity-fix3

## Goal

完成 Limits phase 5 最后一项：cockpit snapshot 原子写入。

## Required changes

完整执行 `/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_b112a4fb2b25.md` 中 `Fix 2 Review / Remaining required change`。

## Constraints

- 仅修改 `LimitsService.cs`、对应测试与 phase-5 handoff。
- 不新增依赖，不访问用户 profile，不记录 snapshot/token/body 内容。
- 完成后发送 `worker_done`。
