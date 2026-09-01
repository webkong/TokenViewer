# Device Sync Phase 1 Review 修复计划

> 目标：修复阶段一 Review 中阻塞安全性、可恢复性和重复 Apply 的问题，使 Local Store foundation 达到进入阶段二前的可验收状态。
>
> 执行对象：Luna。按任务顺序执行，每个任务完成后运行对应测试；不要提交、push 或扩展到云端 Provider/UI。

## 执行约束

- 工作目录：`/Users/wangsw/webkong/TokenViewer/tokenviewer`
- 以 `docs/superpowers/specs/2026-08-31-device-sync-design.md` 和本计划为准。
- 保留用户已有的 `.ai/`、根目录 `default.profraw` 等未跟踪内容；不要用 reset/checkout 覆盖它们。
- 不实现 WebDAV、坚果云、AWS S3、华为 OBS、MinIO、自动同步、历史 UI、三方 merge 或增量 blobs。
- 不新增凭据进入 Rust 配置、日志、fixture 或 snapshot 明文。
- Swift 可继续保持正式 Device Sync Settings 入口隐藏，但 Rust/Swift Apply 协议必须完整、可恢复、可测试。
- 只编辑 `CLAUDE.md`（本计划不需要修改规范文件）；不创建独立的 `AGENTS.md`。

## Review 问题与修复顺序

1. 加密端与解密端压缩比规则不一致，可能发布不可读快照。
2. Swift preference mutation 没有消费，跨层 Apply 事务不完整，崩溃后无法恢复 UserDefaults。
3. SingleFile 已有 TokenViewer 生成文件时被误判为用户占用，重复 Pull 被阻断。
4. 归档扫描与打开文件之间存在 symlink TOCTOU。
5. HLC 没有持久化递增，未修改记录每次被重新赋予版本。
6. parent graph 每次下载解密全部历史，固定深度/字节上限最终阻断正常使用。
7. 启动恢复错误被 `try?` 静默吞掉。

## Task 1：统一快照大小与压缩比策略

**涉及文件**

- `core/src/device_sync/crypto.rs`
- `core/src/device_sync/models.rs`（仅在需要统一常量时修改）
- `core/tests/device_sync.rs`

**实现要求**

- 明确 `MAX_EXPANDED_BYTES`、`MAX_MANIFEST_BYTES`、`MAX_COMPRESSION_RATIO` 的协议含义。
- `encrypt_snapshot()` 在 zstd 压缩后执行与 `decrypt_snapshot()` 等价的压缩比校验；失败不得写入或发布对象。
- 解密侧继续进行流式展开，先校验总展开上限，再校验压缩比，避免一次性分配攻击。
- 错误码保持 `object_too_large`，不要暴露明文内容或密码信息。
- 处理 `compressed_len == 0`、整数溢出和 `saturating_mul` 边界。

**测试要求**

- 增加高度重复内容的 payload：`encrypt_snapshot()` 必须拒绝，且 Local Store 中不存在可发布对象。
- 增加接近但未超过 100:1 的 payload：加密、解密必须成功。
- 保留现有篡改、错误密钥、总大小和 archive bomb 测试。

## Task 2：完成 Rust/Swift 两段 Apply 事务

**涉及文件**

- `core/src/device_sync/engine.rs`
- `core/src/device_sync/models.rs`
- `macos/TokenViewer/Bridge/CoreBridge+DeviceSync.swift`
- 新建 `macos/TokenViewer/Services/DeviceSyncApplyCoordinator.swift`
- 必要时修改 `macos/TokenViewer/Services/Localization.swift`
- `macos/TokenViewerTests/DeviceSyncTests.swift`

**协议要求**

1. Rust `prepare_apply` 重新验证 preview token、远端 head 和本地 fingerprint，创建 journal/rollback，并返回不含 secret 的 preference mutation。
2. Swift coordinator 在 prepare 成功后读取并保存旧 `skillsEnabledProviders`，校验 mutation key 为 allowlist 中的稳定 key，再写入新值并读回确认。
3. Swift 调用 Rust `commit_apply(transaction_id)`；成功后才清理 Swift 侧 transaction 状态。
4. 任一步失败都调用 Rust `rollback_apply(transaction_id)`，并恢复旧 UserDefaults；rollback 失败必须保留 transaction ID 和恢复目录信息。
5. 为 Swift 侧旧偏好建立可恢复 journal（文件权限 `0600`，不保存 secret），使进程在 prepare/偏好写入/commit 任意阶段崩溃后，启动恢复可以决定回滚或完成清理。
6. Rust journal 必须记录足以关联 Swift transaction 的 operation/transaction ID，但不能假设 Rust 能直接修改 UserDefaults。
7. Apply guard 覆盖整个 Swift/Rust 流程，防止文件监听器立即触发反向同步。

**错误处理要求**

- `CoreBridge+DeviceSync.swift` 不得用 `try?` 吞掉 Apply、rollback 或 recovery 错误。
- 错误需要保留 `DeviceSyncErrorPayload.operationId`，并映射到可本地化的错误状态。
- 没有正式 Settings UI 时，coordinator 仍应能被测试直接调用；不要为了隐藏 UI 删除事务能力。

**测试要求**

- 成功 Apply 后 UserDefaults、Skills、env、links 一致。
- 偏好写入失败、commit 失败、Rust rollback 失败分别验证：旧 UserDefaults 是否恢复、Rust rollback 是否被调用、恢复目录是否保留。
- 模拟每个 journal phase 的启动恢复，确认不会出现“文件已回滚但偏好仍为远端值”或相反状态。
- 验证未知 preference key 被拒绝，远端不能注入任意 UserDefaults key。

## Task 3：修复 SingleFile 目标 ownership 判定

**涉及文件**

- `core/src/skills/symlink.rs`
- `core/src/device_sync/engine.rs`
- `core/tests/device_sync.rs`

**实现要求**

- 继续拒绝真实用户文件、目录和外部 symlink。
- 对 TokenViewer 自己生成的 SingleFile 文件提供可验证的 ownership 机制，优先复用已有 registry/linked skill 状态；必要时增加不可执行的 managed marker 或 sidecar。
- 重复 Apply、恢复和 `rebuild_single_file()` 必须允许覆盖自身生成的目标，并保持原子替换。
- 不得通过“文件内容看起来像生成文件”作为唯一 ownership 依据。
- 多个 SingleFile skill 指向同一目标时只执行一次 preflight，避免重复检查产生不一致结果。

**测试要求**

- 空目标创建成功。
- 已有 TokenViewer 生成目标可重复 Apply/rebuild。
- 已有普通文件、目录、外部 symlink 仍返回 `link_target_occupied` 且原数据不变。
- Apply 中途失败后 SingleFile 目标恢复为原始生成文件。

## Task 4：消除归档 symlink TOCTOU

**涉及文件**

- `core/src/device_sync/archive.rs`
- `core/tests/device_sync.rs`

**实现要求**

- 扫描阶段和读取阶段都必须验证路径位于 canonical source root 内。
- Unix 使用 no-follow 方式打开普通文件，或在打开 fd 后检查 fd metadata 与扫描结果一致；不要让 `File::open` 静默跟随替换后的 symlink。
- 对文件在扫描后被替换、大小增长超过限制、类型变化等情况返回 `archive_unsafe` 或 `object_too_large`，不得继续归档。
- 保持现有绝对 symlink、外部 symlink、dangling symlink warning 语义。

**测试要求**

- 增加并发替换测试或可控故障注入：扫描后把文件替换为 `/etc/passwd` 等外部 symlink，归档必须失败且输出不含外部内容。
- 增加文件增长和类型变化测试。
- 保留 deterministic archive 测试，确保修复不改变稳定输出。

## Task 5：实现可持久化 HLC 与 metadata 保留

**涉及文件**

- `core/src/device_sync/models.rs`
- `core/src/device_sync/config.rs`
- `core/src/device_sync/engine.rs`
- `core/src/device_sync/snapshot.rs`
- `core/tests/device_sync.rs`

**实现要求**

- `DeviceSyncState.clock` 作为每设备 HLC watermark 持久化；生成新 snapshot 前读取并原子递增。
- 正常时 `wall_ms` 取当前时间；时间回退或同毫秒时递增 `counter`；跨设备排序使用 `(wall_ms, counter, device_id)`，不要只比较 wall clock。
- 构建 snapshot 成功后再保存新的 clock，构建失败不得消耗或破坏旧状态。
- 对 baseline 中内容未变化的 record 保留原 `record_id/hlc/last_modified_by`；只有内容、关联或显式 tombstone 变化时才生成新 metadata。
- 保持 v1 wire format 兼容；旧 snapshot 缺少新字段时按协议默认值读取，但不能伪造 secret。

**测试要求**

- 时间回退、同毫秒连续生成、进程重启后三种场景验证 HLC 单调性。
- 未修改 Skill/env/link/preference 的连续 snapshot 保留 metadata。
- 修改、删除、恢复分别生成新 metadata 和 tombstone。
- 增加旧 v1 fixture 读取测试。

## Task 6：降低 remote graph 的历史读取风险

**涉及文件**

- `core/src/device_sync/engine.rs`
- `core/src/device_sync/models.rs`
- `core/tests/device_sync.rs`

**实现要求**

- 将“验证 Head/parent graph”与“加载完整 snapshot payload”分离；能只读取 manifest/header 时不得下载并解密全部 archive。
- 保留单对象大小、总图大小、节点数和深度限制，避免通过删除安全上限解决问题。
- 在达到深度或总量阈值时返回可操作错误，指出需要 retention/compact，而不是泛化为 integrity failure。
- 在阶段一没有 history UI 的前提下，至少保证超过 256 次线性 push 时 status/preview 不会因无关历史 payload 直接失效；若协议暂时无法做到，必须显式记录为阶段二前 blocker 并提供 compact 测试工具。

**测试要求**

- 构造超过 256 层线性 parent 的远端目录，验证 status/preview 行为符合新的读取策略。
- 多设备分叉、共享祖先和重复 parent 不重复累计下载字节。
- 恶意循环、超节点数、超字节数仍被拒绝。

## Task 7：启动恢复与可观察性

**涉及文件**

- `macos/TokenViewer/App/TokenViewerApp.swift`
- `macos/TokenViewer/Bridge/CoreBridge+DeviceSync.swift`
- `macos/TokenViewer/Services/DeviceSyncApplyCoordinator.swift`
- `macos/TokenViewerTests/DeviceSyncTests.swift`
- 必要时修改 `macos/TokenViewer/Services/Localization.swift`

**实现要求**

- 启动顺序保持：初始化 Core -> 恢复 master key -> 恢复 pending Apply -> 再加载可受影响的 Agent/Skill 状态。
- master key 缺失可以是正常首次启动，但 Keychain 读取、FFI restore、pending recovery 的非预期错误必须进入可观察状态。
- recovery 失败时阻止新的 Device Sync 操作，保留 journal/recovery path，并显示本地化的恢复提示；不能继续假装处于 clean 状态。
- 成功恢复后清理 journal；清理失败要记录 operation ID 和路径，不得删除仍可能需要恢复的目录。
- 不把密码、VMK、环境变量值写入日志或 UI 错误文本。

**测试要求**

- FFI recovery 返回错误时，App 启动状态为 recovery-blocked。
- Keychain 缺失、错误长度 master key、journal 损坏、journal 路径不匹配分别验证。
- 恢复成功和清理失败均有明确状态，且不影响普通 TokenViewer 用量查看。

## Task 8：补充交叉场景与工作区清理检查

**新增/修改测试**

- selected Skill A 内部链接到未选中的 Skill B：要么在扫描阶段产生明确 warning 并跳过，要么将 B 作为必要依赖纳入 scope；不得等到加密阶段才出现“target missing”。
- `git` content source 不上传 Skill archive，仍只记录 repository hint、links、env、preferences。
- 目标被真实目录/外部链接占用时，preview/apply 不改变原数据。
- 所有错误 envelope 包含稳定 code，secret 不出现在错误、日志、fixture 或 snapshot。

**工作区检查**

- 不删除或覆盖用户已有 `.ai/` 和根目录 `default.profraw`。
- 检查 `macos/default.profraw` 是否只是构建副产物；提交前由用户决定是否恢复，Luna 不得顺手提交。
- 运行 `git diff --check`，确认没有空白错误或意外生成文件。

## 验收命令

```bash
cd /Users/wangsw/webkong/TokenViewer/tokenviewer

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
rtk cargo test --manifest-path core/Cargo.toml --lib --tests

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
rtk xcodegen generate --spec macos/project.yml

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
rtk xcodebuild test \
  -project macos/TokenViewer.xcodeproj \
  -scheme TokenViewer \
  -destination 'platform=macOS'

git diff --check
git status --short
```

## 完成门槛

- Task 1 至 Task 7 的新增回归测试全部通过；不能只依赖已有 180 个 Rust 测试。
- 两个临时 home/source root 完成 cloud content source 的加密 Push、Preview、Pull、Apply、重复 Apply 和故障回滚。
- 错误密码、篡改、压缩 bomb、危险 symlink、TOCTOU、目标占用、偏好写入失败和启动恢复失败都不会静默覆盖或丢失本地数据。
- Rust 和 Swift 构建/测试成功，正式 Settings 仍保持阶段边界要求的隐藏状态。
- Luna 完成后只报告修改文件、测试结果、未解决风险和 worktree 状态，不提交、不 push。
