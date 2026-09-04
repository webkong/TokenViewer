# Task: windows-06-settings-about-final-fix1

## Goal

完成 Settings/About/final 阶段真实行为闭环、自动测试与发布包断言。

## Required changes

完整执行 `/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_78c3a93efca5.md` 的 Required changes 1–8。

## Constraints

- 严格遵守 review 的 Scope；不新增依赖，不修改 Rust/schema/版本/网站/macOS。
- tests 不访问真实用户 profile、网络或 startup 注册表。
- 主 Limits 页面仍显示全部 15 个 canonical agents；visibility 仅作用于 tray popover。
- 不提交、不推送；完成后发送 `worker_done`。
