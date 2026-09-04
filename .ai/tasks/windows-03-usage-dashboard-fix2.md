# Task: windows-03-usage-dashboard-fix2

## Goal

消除 Usage 图表 overlay 测试在 Windows CI 的 WPF apartment 竞态。

## Required changes

1. 仅修改 `windows/TokenViewer.Tests/ChartControlTests.cs`：在显式 `ApartmentState.STA` 线程内创建 WPF Application 和 TrendChartControl、执行 overlay 断言；把异常传回测试线程，并清理 Dispatcher/Application，避免测试进程悬挂。
2. 不新增 NuGet 依赖，不修改生产代码。
3. 更新 `.ai/handoffs/task_c0ce2bcaad18-implementation.md`。
4. 重跑测试项目跨目标 build 和 `git diff --check`。
