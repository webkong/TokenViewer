# Task: windows-05-limits-parity-fix2

## Goal

完成 Limits phase 5 第二轮复审剩余项：实时 snapshot 持久化、可注入的 15 provider 失败隔离测试，以及完整验证记录。

## Required changes

完整执行 `/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_b112a4fb2b25.md` 中 `Fix 1 Review / Remaining required changes` 1–3。

## Constraints

- 仅修改 phase 5 原文件、测试与 handoff；不修改 Usage、tray、settings/about。
- snapshot 文件测试必须使用临时目录，不访问用户 profile；生产写入必须容错。
- 不在日志、错误或断言信息中输出 token/auth/body。
- 不新增生产依赖。
- 完成后发送 `worker_done`。
