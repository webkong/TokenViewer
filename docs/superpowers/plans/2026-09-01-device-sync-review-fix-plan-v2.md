# Device Sync Phase 1 Review 修复计划 v2

> 目标：修复 Luna 上一轮修复后仍存在的安全性、状态一致性、历史容量和 Swift 调用边界问题。
>
> 执行对象：Luna。完成后由主 Agent 再次 Review。不要提交、push 或提前实现云端 Provider/UI。

## 执行边界

- 工作目录：`/Users/wangsw/webkong/TokenViewer/tokenviewer`
- 设计基线：`docs/superpowers/specs/2026-08-31-device-sync-design.md`
- 只修改 Device Sync 相关文件及必要测试；不要覆盖用户已有 `.ai/`、`default.profraw` 或其他无关改动。
- 当前工作区包含 parser、session、Git engine、数据库等无关变更，执行时不得格式化或重写这些文件。
- 保持正式 Device Sync Settings、WebDAV、坚果云、S3、OBS、MinIO、自动同步、历史 UI、三方 merge 和增量 blobs 的阶段边界。

## 修复优先级

1. 认证祖先 graph header，防止错误 frontier 和分支丢失。
2. 修复完整历史大小预算，避免合法历史达到 256 MiB 后永久不可用。
3. 修复 `save_state` 失败时 journal 与文件状态不一致。
4. 让 recovery 状态真正可见，并在所有 Device Sync 操作上生效。
5. 正确合并远端 HLC，保持本机设备身份和单调性。
6. 将阻塞 FFI/文件操作移出 MainActor。

## Task 1：认证 snapshot graph header

**涉及文件**

- `core/src/device_sync/crypto.rs`
- `core/src/device_sync/models.rs`
- `core/src/device_sync/engine.rs`
- `core/tests/device_sync.rs`

**问题**

`inspect_snapshot_header()` 返回未认证 header，但 `remote_view()` 使用非 frontier 祖先 header 的 `parent_ids` 计算 frontier。AEAD AAD 只有在完整解密时才验证，当前祖先遍历没有验证 tag。

**实现要求**

- 为 snapshot header 增加可验证的完整性字段，使用 VMK 派生的固定 context（例如 `tokenviewer/snapshot-header/v1`）计算 MAC；MAC 必须覆盖 magic、协议版本、vault、snapshot、parent_ids、payload hash 等关键字段。
- 认证字段不能依赖明文密码、Keychain 内容或未加密配置。
- 新对象发布前必须生成并验证 header MAC；读取旧 v1 对象时保持兼容策略，并明确哪些对象必须完整解密后才能读取 parent。
- `remote_view()` 只有在 header 认证通过后才能将 `parent_ids` 写入 graph；认证失败统一返回 `integrity_failed`，不能继续计算 frontier。
- Head 现有 HMAC 仍保留，不能用 header MAC 替代 Head MAC。

**测试要求**

- 修改非 frontier ancestor header 的 `parent_ids`，但不修改 ciphertext，必须被拒绝。
- 修改 header MAC、snapshot id、vault id、payload hash、parent 数量，均必须被拒绝。
- 验证恶意 header 不能让一个并发 Head 从 frontier 中消失。
- 旧 v1（无 header parent_ids/MAC）fixture 必须按兼容路径完整认证 payload 后再得到 parent。

## Task 2：分离 graph metadata 预算与完整 payload 预算

**涉及文件**

- `core/src/device_sync/engine.rs`
- `core/src/device_sync/store/mod.rs`
- `core/src/device_sync/store/local.rs`
- `core/src/device_sync/models.rs`
- `core/tests/device_sync.rs`

**问题**

header-only 遍历实际只读取前缀，却把完整对象 `meta.size` 累加到 `MAX_GRAPH_BYTES`。两个合法的大 snapshot 就可以使 status/preview 永久失败。

**实现要求**

- 定义独立的 graph metadata 预算，统计实际读取的 prefix/header 字节和节点数。
- 完整 snapshot payload 的 `MAX_ENCRYPTED_SNAPSHOT_BYTES` 只在需要解密 frontier 或执行 Apply 时使用。
- 不能简单删除 `MAX_GRAPH_BYTES`、节点数或深度限制。
- 对恶意循环、超节点数、超 header 读取量、超单对象大小分别返回稳定错误。
- 对尚未有 retention/compact 的阶段一，提供明确可操作错误；不得把合法历史误报为完整性错误。
- 如果仍需限制“可达完整 payload 总量”，必须只对实际下载并认证的 payload 计数，并避免 status/preview 因无关 archive 大小失败。

**测试要求**

- 构造多个接近 `MAX_ENCRYPTED_SNAPSHOT_BYTES` 的 snapshot，确认 header-only status/preview 不因完整对象总大小失败。
- 构造超过 metadata 节点/字节预算的 graph，确认被拒绝。
- 构造超过 payload 下载预算的 frontier Apply，确认被拒绝且不修改本地文件。
- 保留 257 层线性历史测试，并增加大对象历史测试。

## Task 3：修复 Apply 失败时 journal 与文件状态分裂

**涉及文件**

- `core/src/device_sync/engine.rs`
- `core/tests/device_sync.rs`

**问题**

`apply_transaction()` 将 journal 写成 `committed` 后，`save_state()` 可能失败。当前代码即使无法持久化 `applying/prepared` 降级，也继续恢复旧文件；重启时残留的 committed journal 会把状态标记为远端已应用。

**实现要求**

- 在恢复文件前，必须先持久化不可歧义的 rollback intent；intent 写失败时不能继续恢复旧文件。
- 设计明确的 journal phase 转移：`prepared -> applying -> committed`，以及失败路径的 `rollback_requested -> rolled_back`；每次转移原子写入并 fsync。
- 如果状态保存失败但文件已经应用成功，优先保留可恢复的 committed 事实，由启动 recovery 完成 state 收敛；不要产生“文件旧、state 新”的组合。
- rollback intent 持久化失败时返回 `rollback_failed`，保留 recovery/rollback 目录和 operation ID。
- 启动 recovery 对未知或歧义 phase 必须阻塞，不得按 committed 猜测。

**测试要求**

- 注入 `save_state` 失败。
- 注入 journal downgrade 写失败、恢复目录写失败和 rollback 失败的组合。
- 重启后验证 Skills、links、env、state 四者一致。
- 验证任何失败都不会留下 committed journal 指向已恢复旧文件的状态。

## Task 4：统一 recovery 状态与操作门禁

**涉及文件**

- `macos/TokenViewer/App/TokenViewerApp.swift`
- `macos/TokenViewer/Bridge/CoreBridge+DeviceSync.swift`
- `macos/TokenViewer/Services/DeviceSyncApplyCoordinator.swift`
- `macos/TokenViewer/Services/Localization.swift`
- `macos/TokenViewerTests/DeviceSyncTests.swift`

**问题**

恢复失败只写入 coordinator 内部 state，没有当前 UI/通知消费者；同时 preview、push、config、vault 等 CoreBridge 方法可以绕过 coordinator 的 recovery block。

**实现要求**

- 建立统一的 Device Sync session/coordinator gate。恢复未成功前，所有 Device Sync 操作（status 读取除外）必须返回 `recovery_blocked`。
- 不能只依赖 Swift 调用方自觉使用 coordinator；Rust engine 或 bridge 层也要有明确的 blocked 状态/检查。
- 为后续 Settings UI 提供可订阅的 `@Published` recovery 状态；当前没有正式 UI 时至少记录本地化错误状态和恢复路径。
- 启动时 recovery 失败不得被吞掉；普通 TokenViewer 用量页面仍可启动，但 Device Sync 操作必须禁用。
- 增加“重试 recovery/清除已解决 block”的明确生命周期；不能让一次临时 Keychain 错误永久阻塞整个进程。
- 不在错误文本或日志中输出密码、VMK、环境变量值。

**测试要求**

- FFI recovery 失败后，preview/push/pull/config/vault 操作全部被拒绝。
- recovery 成功后 gate 可恢复为可用。
- Keychain 暂时失败后修复凭据并重试，不能永久 blocked。
- 验证 recovery 状态可以被 UI/观察者读取；补齐中英文 L10n parity。

## Task 5：HLC 接收远端时钟

**涉及文件**

- `core/src/device_sync/models.rs`
- `core/src/device_sync/engine.rs`
- `core/src/device_sync/config.rs`
- `core/tests/device_sync.rs`

**实现要求**

- 保持 `DeviceSyncState.clock.device_id` 始终为本机 device ID。
- Pull/Apply 或读取远端 frontier 后，将远端 manifest clock 作为 observed timestamp 合并到本机 watermark。
- 生成下一本机事件时使用标准 HLC receive/update 规则：`max(local_wall, remote_wall, now)`；同一逻辑时间递增 counter；时钟回退不得倒退。
- 构建失败、加密失败或 Apply 失败不得错误消耗本机 HLC；成功观察远端后再原子持久化本机 watermark。
- 保留未修改 record metadata；只有内容变化、关联变化和 tombstone 才生成新 metadata。

**测试要求**

- 远端 clock 领先本机 clock 后，下一次本机 Push 的 HLC 大于远端 clock。
- 本机系统时钟回退、同毫秒连续事件、进程重启均保持单调。
- 验证 state clock 的 device ID 不会变成远端设备 ID。
- 增加旧 snapshot clock 缺失的兼容 fixture。

## Task 6：移出 MainActor 的阻塞工作

**涉及文件**

- `macos/TokenViewer/Services/DeviceSyncApplyCoordinator.swift`
- `macos/TokenViewer/Bridge/CoreBridge+DeviceSync.swift`
- `macos/TokenViewerTests/DeviceSyncTests.swift`

**实现要求**

- 将 apply、recovery、preview、push、pull 等阻塞 FFI 调用封装为 async API，在 `Task.detached` 或既有后台执行器中运行。
- FFI handle 访问仍必须通过 `CoreBridge` 的串行队列；不得把 `OpaquePointer` 跨任务裸传递。
- 所有结果回到 `@MainActor` 更新 `@Published` 状态。
- 捕获 generation/profile ID；用户切换配置、页面关闭或开始新请求后，旧结果不得覆盖当前状态。
- Apply guard 必须跨越后台任务整个生命周期，不能在启动 detached task 后立即释放。
- 启动 recovery 可以阻塞启动流程，但必须避免主线程执行大文件恢复；需要明确启动阶段的 loading/blocked 状态。

**测试要求**

- 主线程执行期间提交大文件/慢 FFI，确认 UI queue 不被同步阻塞。
- 旧 generation 结果不能覆盖新请求。
- 后台 Apply 期间监听器不会触发反向同步。
- 并发请求仍按 CoreBridge/Rust mutex 规则串行或返回 `operation_in_progress`。

## Task 7：回归与工作区检查

**必须补充的场景**

- ancestor header 篡改与 frontier 错误。
- 大对象历史与 metadata-only graph 遍历。
- `save_state`/journal 写入故障组合。
- recovery block 的全入口门禁和恢复后解锁。
- 远端 HLC 领先、本机回退、重启。
- selected Skill A 链接到未选择 Skill B，必须在扫描阶段 warning/skip 或纳入依赖，不能在加密末端才失败。
- `git` content source 不上传 Skill archive。

**验证命令**

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

- Rust 与 Swift 新增测试全部通过，不能只报告已有测试数量。
- 认证 graph header、历史预算和 journal 故障注入均有自动化回归。
- 两个临时 home/source root 完成加密 Push、Preview、Pull、Apply、重复 Apply 和回滚；失败后 Skills、links、env、preferences、state 一致。
- recovery block 能被观察、能阻止所有 Device Sync 写/同步操作，并能在修复后明确解除。
- 不提交、不 push；完成报告列出修改文件、测试结果、剩余风险和 worktree 中未归属的无关改动。
