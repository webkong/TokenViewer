# Task: windows-06-settings-about-final-fix3

## Goal

修复最终复审剩余的 visibility reset/legacy toggle、popover 卡片信息与测试清理。

## Required changes

完整执行 `/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_78c3a93efca5.md` 中 `Fix 2 Review / Remaining required changes` 1–4。

## Constraints

- 仅修改 `AppSettings/SettingsViewModel/PopoverWindow/SettingsTests/handoff` 中必要文件。
- 复用现有 countdown/L10n/shared Limits，不新增服务、timer、网络或依赖。
- 不提交、不推送；完成后发送 `worker_done`。
