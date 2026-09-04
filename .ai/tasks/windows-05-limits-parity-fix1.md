# Task: windows-05-limits-parity-fix1

## Goal

完成 Limits 真实 parity、日期选择、响应式、本地化与离线 fixture 验收。

## Required changes

完整执行 `/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_b112a4fb2b25.md` 的 Required changes 1–6。

## Constraints

- WorkBuddy/CodeBuddy 按 macOS 三请求逻辑移植，失败回退 cache；不得在错误中输出 token/auth/body。
- fixture 测试通过纯 parser/可注入 orchestration，不访问真实网络/profile。
- 1240 宽两列、1000 宽单列须有确定性逻辑和测试。
- 不新增生产依赖或通用 SQL FFI。
- 更新 phase-5 handoff，完成后发送 worker_done。
