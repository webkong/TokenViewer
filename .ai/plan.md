# Windows UI Port 实施计划

## 目标

依据 `/Users/wangsw/webkong/TokenViewer/tokenviewer/docs/superpowers/specs/2026-08-13-windows-ui-port-design.md`，把现有 Windows WPF 骨架扩展为可构建、可测试、具备 macOS 主要功能面的原生 Windows 应用。

## 基线与交付方式

- 实施基线：`436905710783bef6a31db55d7b4686d46162fab6`。
- 主工作树中的设计文档有未提交修订；Implementer 只读绝对路径中的最新版，不覆盖或复制主工作树改动。
- 实施 Agent：Claude Code 会话 `de9673c3-ae4b-44d0-9eec-be1c0bf4da43`，在 Orca 独立子 worktree 中顺序执行。
- 每个任务完成后发送一次 `worker_done`，由 GPT 对真实 diff 和测试结果 Review；通过后再派发下一任务。
- 不新增 NuGet/Rust 依赖，除非任务升级并获得批准。

## 任务 DAG

1. `windows-01-foundation`：Rust Windows 数据路径、UTF-8 P/Invoke 基础与现有 Windows CI 加固。
2. `windows-02-data-layer`：查询模型、AppTime、Localization、SyncCoordinator、UsageViewModel 与自动测试骨架。
3. `windows-03-usage-dashboard`：完整 Usage Dashboard、自绘趋势图和热力图。
4. `windows-04-resources-tray`：品牌资源、托盘 ICO、PopoverWindow 和托盘交互。
5. `windows-05-limits-parity`：15 个 canonical limits 集成与 Limits UI；移除外部 sqlite3.exe 依赖。
6. `windows-06-settings-about-final`：Settings/About 对齐、重建/重置动作、最终 CI/打包断言和综合验收。

依赖关系严格为 `01 → 02 → 03 → 04 → 05 → 06`，避免同一文件并发写入。每个阶段只允许修改其任务契约列出的文件。

## 全局不变量

- Rust 30 分钟桶、UTC 存储、本地时区查询、FileCursor 幂等格式和既有 FFI JSON 契约不变。
- Windows 内部 source ID 与 macOS AgentRegistry 一致；用户界面称 Agent，不把 source 改名为 provider。
- 所有用户可见文字必须经过 EN/ZH Localization；不得硬编码新 UI 文案。
- Skills UI、WebView2、云同步、账号系统和自动发布不在范围内。
- 主工作树不自动合并；最终 PASS 后报告候选 commit 和安全合并条件。

