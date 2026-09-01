# Device Sync 设计与实施方案

**日期**：2026-08-31
**状态**：待实现，方案可交付 Luna
**范围**：在 TokenViewer 中增加 Skills 与相关配置的跨设备加密同步，不同步用量数据库、Agent 会话或云存储凭据。

## 1. 目标

Device Sync 用于把一台设备上的 Skill 工作环境安全地带到另一台设备，覆盖：

- TokenViewer 管理的 Skills 内容；
- Skill 与 Agent 的启用、关联关系；
- Skill 所需环境变量；
- 与 Skill 有关且可跨设备迁移的偏好；
- WebDAV、AWS S3、华为云 OBS、MinIO 等 S3 兼容存储；
- 手动预览、上传、拉取、历史恢复，以及后续的自动同步与冲突处理。

验收结果不是“若干文件已上传”，而是新设备加入同一个 Vault 后，可以预览远端状态，恢复 Skills，按本机 Agent 注册表重新生成链接，并恢复环境变量；任何覆盖本地内容的动作都可预览、确认和回滚。

## 2. 非目标

以下内容不在本功能范围内：

- 不同步 `~/.tokenviewer/data.db`、用量记录、限额、会话、日志或缓存；
- 不同步 Git Token、WebDAV 密码、S3 Access Key/Secret、Session Token 或 Keychain 原始内容；
- 不同步 `.zshrc`、`.bashrc`，只在本机沿用现有 `SkillEnvironmentManager` 配置 source block；
- 不同步真实软链接，也不把一台设备的绝对 Agent 路径直接写到另一台设备；
- 不做实时协同编辑，不承诺多个设备同时离线编辑时无冲突自动收敛；
- 第一至第三阶段不做内容寻址增量上传，先保证协议、安全、恢复和多后端一致；
- 不把现有 Skills Git Sync 替换成云同步。Git 仍是可选的 Skill 内容来源。

## 3. 命名与边界

代码、配置键和 FFI 统一使用 `device_sync` / `DeviceSync`。不要使用裸 `sync`，避免与用量解析器的 `tt_sync_all`、Skills Git Sync 和 UI 中的刷新动作混淆。

职责边界：

```text
SwiftUI Views
    -> DeviceSyncViewModel (@MainActor)
        -> DeviceSyncCredentialStore (Keychain)
        -> CoreBridge+DeviceSync (JSON FFI, Task.detached)
            -> Rust device_sync::engine
                -> snapshot / crypto / merge
                -> ObjectStore
                    -> LocalFolder / WebDAV / S3
```

- Rust Core 负责扫描、规范化、归档、加密、远端协议、预览、合并、事务应用和回滚。
- Swift 负责设置表单、Keychain、异步状态、预览/冲突/历史界面和本地化。
- FFI 保持同步函数；Swift 在后台任务调用。不要把 Tokio runtime 或回调生命周期暴露给 Swift。
- 同一个 `CoreHandle` 上的 Device Sync 操作必须由 Rust 互斥锁串行化，第二个请求返回 `operation_in_progress`，不能并发修改 Skills。

## 4. 当前状态与真实数据源

TokenViewer 当前相关状态如下：

| 数据 | 当前来源 | Device Sync 表示 |
|---|---|---|
| Skill 内容 | `SkillsCore.source_root`，默认位于 `~/.tokenviewer/skills` | 规范化文件树或外部 Git 引用模式 |
| Agent 关联 | `~/.tokenviewer/skills-manager/linked_skills.json` | `agent_id -> skill_ids` 逻辑映射 |
| Agent 覆盖 | `agent_overrides.json` | 仅同步可移植字段，不同步绝对路径 |
| Agent 可见性 | UserDefaults `skillsEnabledProviders` | 排序后的 Agent ID 集合 |
| 环境变量 | `~/.tokenviewer/skill-env.sh` | 解析后的 `name -> value`，远端始终加密 |
| Shell 接入 | `.zshrc` / `.bashrc` 中 source block | 不同步；恢复后调用现有管理器确保存在 |
| Skill Git 设置 | `skills_config.json` + Keychain Token | 不进入 Device Sync 快照 |
| 真实 Agent 链接 | 各 Agent 的 Skills 目录 | 不打包；恢复时调用 `SymlinkManager` 重建 |

不要直接复制 `linked_skills.json`、`skill-env.sh` 或 UserDefaults plist。Snapshot Builder 应从现有 API 读取并转换为版本化领域模型；Apply 再通过现有管理器写回。这样可以隔离本机路径、旧字段和未来迁移。

环境变量同步必须是显式 opt-in。Snapshot Builder 先从 Skill manifest/扫描结果得到 `skill_id -> variable_names`，再读取现有 managed values；用户可按 Skill 勾选并进一步取消单个变量。未被任何已选 Skill 声明的变量、没有值的变量和用户未勾选的变量都不进入快照。一个变量被多个 Skill 共享时只存一条 value 记录，并保存非敏感的 `referenced_by_skill_ids` 用于 Preview；取消一个 Skill 不得误删仍被其他已选 Skill 引用的变量。

## 5. Skill 内容来源规则

用户必须选择一种 Skill 内容来源：

1. `cloud`：Device Sync 快照包含 Skills 文件和配置；远端可在没有 Git 的设备上完整恢复。
2. `git`：现有 Skills Git Sync 管理 Skills 内容；Device Sync 只保存仓库身份提示、Agent 关联、环境变量和偏好，不上传 Skill 文件。

约束：

- Git 与 Cloud 不得同时自动写 Skills 内容；保存设置时校验并阻止双写。
- 从 `cloud` 切到 `git` 前先显示影响预览；未提交本地内容不能被删除。
- `git` 模式不上传 Git Token，也不把远端 URL 中的 userinfo 写入快照。
- `git` 模式恢复后，如果目标设备尚未配置仓库，只保留 `pending_content_source` 并提示用户完成 Git 配置；不能创建空链接。

## 6. 同步数据分类

### 6.1 必须同步

`cloud` 模式：

- source root 中属于同步范围内 Skill 的普通文件和目录；
- Skill ID、相对路径、文件哈希、规范化权限；
- Agent 与 Skill 的关联集合；
- `skillsEnabledProviders`；
- 用户明确选中的环境变量名称和值；
- 可移植设置，例如内容来源模式和用户选定的同步组件。

`git` 模式：

- Agent 与 Skill 的关联集合；
- `skillsEnabledProviders`；
- 用户明确选中的环境变量名称和值；
- 经过清理的 Git 仓库提示：provider、branch、无凭据 URL、可选 commit OID；
- 可移植设置。

### 6.2 必须排除

- 任意路径组件名为 `.git` 的目录或文件；
- `.DS_Store`、`Thumbs.db`、`*.swp`、`*.swo`、`*~`；
- Unix socket、device、FIFO；
- 指向 source root 外部的软链接；
- `skills_config.json`、Git 凭据、对象存储凭据和 Keychain 数据；
- `data.db*`、崩溃文件、profraw、会话、日志、下载缓存；
- Agent 的绝对路径覆盖和安装检测缓存。

不要像参考项目一样排除所有隐藏文件。Skill 可以合法包含 `.claude`、`.config` 等内容；只按明确 denylist 和安全规则过滤。

### 6.3 软链接规则

- source root 内部指向内部普通文件的软链接可以在归档中保留为“受限相对链接”，Apply 时重新校验目标仍在 root 内。
- 指向 root 外部、绝对路径、循环链接或悬空外部链接均不上传，并在 Preview 中给出 warning。
- Agent 目录中的链接不进入快照。恢复关联关系后调用现有 `SymlinkManager`。
- 如果目标位置是用户真实目录、普通文件或指向别处的链接，Apply 必须停止该项并返回 `link_target_occupied`，不得删除或接管。
- 本机未安装某 Agent 时保留 `pending` 关联；安装后可重试重建。

## 7. 本地状态

Device Sync 自有状态放在：

```text
~/.tokenviewer/device-sync/
  config.json               # 非敏感配置，0600，原子写
  device.json               # 随机 device_id、显示名、创建时间
  state.json                # 已应用/已见 heads、HLC、上次结果
  staging/                  # 临时下载/解密/预览，操作结束清理
  rollback/<operation-id>/  # 非 Skills 配置的 Apply 前备份
```

当 source root 位于其他 volume 时，Skills 的 staging/rollback 不能固定放在上述目录；Engine 必须在 source root 的父目录创建仅当前用户可访问的同文件系统临时目录，验证 device/volume 后才使用原子 rename。`~/.tokenviewer/device-sync/rollback` 只保存同一 volume 上可安全恢复的配置状态和事务 journal。

Keychain 项使用新的 service，不复用 Git Token service：

```text
service = com.tokenviewer.device-sync
accounts:
  <profile-id>:webdav-password
  <profile-id>:s3-secret-key
  <profile-id>:s3-session-token
  <vault-id>:master-key
```

同步密码只在创建/加入 Vault 时进入内存；默认不持久化。Vault Master Key 以 `kSecAttrAccessibleWhenUnlocked` 保存。日志只允许 profile ID、provider、HTTP status、对象 key 的哈希或非敏感后缀，不得记录密码、Authorization header、签名 canonical request、环境变量值或 Vault Key。

`config.json` 示例：

```json
{
  "schema_version": 1,
  "enabled": true,
  "profile_id": "01...",
  "vault_id": "9f...",
  "provider": {
    "kind": "webdav",
    "endpoint": "https://dav.jianguoyun.com/dav/",
    "remote_prefix": "tokenviewer-sync",
    "username": "user@example.com"
  },
  "content_source": "cloud",
  "components": ["skills", "agent_links", "skill_env", "preferences"],
  "skill_scope": { "mode": "selected", "skill_ids": ["example-skill"] },
  "environment_scope": { "mode": "selected", "names": ["EXAMPLE_API_KEY"] },
  "auto_sync": false
}
```

配置读取必须兼容未知字段；协议版本不兼容时只允许读取元数据并返回 `protocol_unsupported`，不能尝试恢复。

## 8. 远端对象协议

### 8.1 布局

所有 provider 使用相同 key 空间：

```text
<remote-prefix>/<vault-id>/
  vault.json
  devices/<device-id>/head.json
  snapshots/<snapshot-id>.tvsync
  blobs/<sha256-prefix>/<sha256>       # 第四阶段启用
```

- Snapshot 是不可变对象；同一个 key 再次 PUT 必须得到相同内容，否则返回 `immutable_object_conflict`。
- 每台设备只更新自己的 Head，不需要跨供应商分布式锁。
- 写入顺序必须是 snapshot/blobs 成功后再写 head。失败会留下 orphan，但不会发布不完整快照。
- Head 更新优先使用条件写；若后端条件写能力不足，则在上传前后重新读取并比较上一版本，发现变化返回冲突。
- `list` 从第一阶段即进入 `ObjectStore`，因为 frontier 依赖设备 Head 列表；Local Store 先实现，WebDAV/S3 在各自阶段实现。`delete` 也保留在 trait 中，但正式 retention 到第四阶段才开放。

### 8.2 多设备 Head 与 frontier

Pull/Push 不能按 `updated_at` 选一个“最新 Head”。Engine 必须列出并验证全部设备 Head，加载它们的 parent graph，去掉已被其他 Head 包含的祖先，得到当前 frontier：

- frontier 为空：Vault 尚无 snapshot；
- frontier 只有一个 snapshot：它是当前基线；
- frontier 多于一个：设备已分叉。第一至第三阶段返回 `conflict_requires_resolution`，不允许继续 Push 或覆盖式 Pull；第四阶段以共同祖先三方合并，成功后写包含两个或更多 frontier parent 的 merge snapshot；
- 本地 Push 只有在本地已应用 snapshot 等于当前唯一 frontier 时才允许；否则必须先 Pull/merge；
- 设备第一次加入并 Apply 远端 snapshot 后记录本地基线，但不伪造远端 Head。该设备首次 Push 时以当前 frontier 为 parent 并创建自己的 Head；
- 恢复旧历史不是把 Head 指回旧对象，而是创建一个以当前 frontier 为 parent、内容来自旧 snapshot 的新 snapshot，保留可审计历史。

`updated_at` 只用于 UI。祖先判断、冲突和 prune 都以 parent graph 为准。为避免恶意或损坏远端构造无限图，遍历必须有节点数、深度、对象总字节和循环检测上限。

### 8.3 `vault.json`

```json
{
  "format": "tokenviewer-vault",
  "protocol_version": 1,
  "vault_id": "uuid",
  "created_at": "RFC3339 UTC",
  "kdf": {
    "name": "argon2id",
    "version": 19,
    "salt_b64": "...",
    "memory_kib": 65536,
    "iterations": 3,
    "parallelism": 1
  },
  "key_wrap": {
    "algorithm": "xchacha20-poly1305",
    "nonce_b64": "...",
    "ciphertext_b64": "..."
  }
}
```

参数属于协议字段，不能依赖 crate 默认值。实现时在目标最低配置 Mac 上测量 Argon2id 耗时，再在不降低 64 MiB 内存的前提下调整迭代次数。创建 Vault 生成随机 256-bit VMK；密码经 Argon2id 生成 wrapping key，只用于包装 VMK。

### 8.4 Head

```json
{
  "format": "tokenviewer-head",
  "protocol_version": 1,
  "vault_id": "uuid",
  "device_id": "uuid",
  "snapshot_id": "uuid",
  "parent_ids": ["uuid"],
  "updated_at": "RFC3339 UTC",
  "snapshot_sha256": "hex",
  "sequence": 42,
  "mac_b64": "..."
}
```

Head 的 `mac_b64` 是除该字段外 canonical JSON 的 HMAC-SHA256，key 由 VMK 通过 HKDF-SHA256 和固定 context `tokenviewer/head/v1` 派生。Canonical JSON 采用 RFC 8785/JCS 规则并加入跨语言 test vector；协议模型禁用浮点数，不能依赖普通 `HashMap` 的迭代顺序。读取时先验证 vault/device，再验 MAC。`state.json` 记录每设备最高 sequence 和已见 snapshot；远端回退时返回 `remote_rollback_detected`，只能由明确的历史恢复流程继续。

### 8.5 Snapshot manifest

解密后的 payload 包含 canonical manifest 和可选文件归档：

```json
{
  "format": "tokenviewer-snapshot",
  "protocol_version": 1,
  "snapshot_id": "uuid",
  "vault_id": "uuid",
  "device_id": "uuid",
  "parent_ids": ["uuid"],
  "created_at": "RFC3339 UTC",
  "clock": { "wall_ms": 0, "counter": 0, "device_id": "uuid" },
  "content_source": "cloud",
  "components": {
    "skills": { "sha256": "hex", "bytes": 0, "entries": 0 },
    "agent_links": { "sha256": "hex", "records": 0 },
    "skill_env": { "sha256": "hex", "records": 0 },
    "preferences": { "sha256": "hex", "records": 0 }
  },
  "tombstones": [],
  "limits": { "archive_bytes": 0, "expanded_bytes": 0 }
}
```

每条逻辑记录还要保存 `record_id`、HLC、最后修改设备和 tombstone。第一阶段即写入这些字段，即使高级三方合并到第四阶段才开放，避免以后破坏 wire format。

### 8.6 `.tvsync` 加密封装

格式：固定 magic/version + header length + canonical JSON header + 24-byte nonce + ciphertext/tag。Payload 先构造成确定性归档，再 zstd 压缩，最后用 XChaCha20-Poly1305 加密。

- VMK 通过 HKDF 派生 snapshot key，context 包含 `vault_id/snapshot_id/protocol_version`；
- AAD 包含 magic、协议版本、vault ID、对象类型、snapshot ID；
- nonce 必须来自 CSPRNG，不能从 snapshot ID 或时间推导；
- 解密后先校验 payload hash、manifest 和所有 component hash，再解包；
- 使用 `zeroize` 清理密码、wrapping key、VMK 临时副本和解密后的环境变量 buffer；
- 错误密码与被篡改对象统一返回认证失败类错误，不泄漏内部差异。

远端对象名、Skill 名和环境变量名仍可能形成侧信道。第一版接受对象数量/时间元数据可见，但 snapshot payload 内的文件名、Agent ID 和变量名均加密；风险需写入帮助文本。

## 9. Archive 与资源限制

使用一个经过测试的归档 crate，不手写 ZIP/TAR 解析器。若选 `zip`，固定文件排序、时间戳、权限和压缩参数，禁止绝对路径、`..`、NUL、重复规范化路径和大小写碰撞。若选 `tar`，同样归零 mtime/uid/gid/uname/gname 并限制链接类型。Luna 在第一阶段只能选择其中一种并写 ADR 注释，不能并存两套格式。

默认上限（做成协议和本地配置中的常量）：

| 项目 | 默认上限 |
|---|---:|
| 单文件 | 32 MiB |
| 加密 snapshot 下载 | 256 MiB |
| 解压后总大小 | 512 MiB |
| 文件条目数 | 20,000 |
| 相对路径 UTF-8 长度 | 1,024 bytes |
| 压缩比 | 100:1 |
| 环境变量数 | 1,000 |
| 单个环境变量值 | 64 KiB |

扫描和解包都必须流式计数，不能先把不受限响应或全部解压内容放入内存。任何超限先停止，staging 清理，不修改现有目录。

## 10. 领域模型与合并

### 10.1 记录

- Skill 内容记录以规范化相对路径/Skill ID 标识，整个 Skill 目录作为用户冲突单元。
- 环境变量以变量名为 ID；值不进入日志和 Preview 文本，只显示“已设置/将更新”。
- Agent 关联以 `(agent_id, skill_id)` 为集合元素。
- 偏好以稳定 key 为 ID，并使用 allowlist。
- 删除产生 tombstone，不能通过“当前不存在”推断删除；tombstone 至少保留至所有已知 device head 都有该祖先，之后才能 prune。

### 10.2 版本与父关系

- 每个 snapshot 保存 0、1 或 2 个 parent；正常 push 为一个 parent，解决冲突后的 merge snapshot 为两个 parent。
- 使用 HLC `{wall_ms, counter, device_id}` 生成确定性版本标记；系统时钟回退时 counter 递增。
- Head 的设备时间只用于展示，不能作为唯一冲突裁决依据。

### 10.3 三方合并

基于共同祖先比较 local、remote：

| 情况 | 结果 |
|---|---|
| 只有一侧修改 | 自动采用修改侧 |
| 不同 Skill 分别修改 | 自动合并 |
| 同一 Skill 两侧修改 | `skill_content_conflict`，用户选本地/远端/保留两份 |
| 同一环境变量两侧修改 | `environment_conflict`，用户选本地/远端 |
| 关联集合不同元素变化 | OR-set 合并，tombstone 防止删除复活 |
| 同一元素删除与修改 | 显式冲突，不静默 LWW |
| Agent 未安装 | 保存 pending intent，不算冲突 |
| 目标被真实目录/外部链接占用 | 阻塞该项并要求用户处理 |

第一至第三阶段如果检测到远端 Head 已不再是 Preview 的基线，返回 `stale_preview` 并要求重新 Preview；不做覆盖式 last-write-wins。第四阶段再启用自动三方合并和 Conflict Sheet。

## 11. Preview、Apply 与回滚事务

任何 Pull、Restore 或可能覆盖本地数据的操作必须遵循：

```text
download -> bounded verify -> decrypt -> stage -> diff/preview
    -> user confirmation with preview_token
    -> revalidate remote head + local fingerprint
    -> create rollback backup
    -> apply skills
    -> persist logical config/env
    -> rebuild agent links
    -> verify postconditions
    -> commit local state / cleanup rollback by retention
```

Preview 返回一次性 `preview_token`、remote head fingerprint、local fingerprint 和过期时间。Apply 只接收 token 与用户的冲突选择，不接受 Swift 重传任意文件路径。任何基线变化返回 `stale_preview`。

应用策略：

- 所有 staging 和 rollback 目录必须位于与 source root 同一文件系统，优先使用原子 rename；
- 替换前保存 Skills、`linked_skills.json`、环境变量逻辑值和受影响链接状态；
- 任一步失败按逆序恢复；回滚本身失败返回 `rollback_failed` 并保留目录路径供用户恢复；
- 成功后重新扫描 Skills，验证关联目标都指向 source root，环境文件权限为 `0600`；
- 自动同步抑制 guard 覆盖整个 Apply，避免文件监听器把刚拉取的变化立刻推回。

### 11.1 Rust/Swift 跨层事务

Rust 不能直接修改 Swift UserDefaults，也不能调用 `SkillEnvironmentManager`。实现不能假装一个 FFI 调用天然覆盖两个 runtime。采用可恢复的两段应用协议：

1. `prepare_apply(preview_token, resolutions)`：Rust 重新验证基线，建立 journal/rollback，暂存 Skills、links、env 与 portable preferences，返回 `transaction_id` 和不含 secret 的 Swift preference mutation；此时不发布最终状态。
2. Swift 保存旧 `skillsEnabledProviders`，写入新值并同步读取确认；环境变量值不经过 Swift，仍由 Rust 直接按现有 `skill-env.sh` 格式原子写入。
3. `commit_apply(transaction_id)`：Rust 原子切换 Skills，写逻辑配置/env，调用 `SymlinkManager` 重建并验证，然后标记 journal committed。
4. Swift 调用现有 shell profile helper 确保 source block 存在；如果失败，提示“值已恢复但新 Shell 尚未接入”，不修改用户已有 profile 内容。
5. 任一步失败调用 `rollback_apply(transaction_id)`，Rust 恢复文件/links/env，Swift 恢复旧 UserDefaults。应用启动时先调用 `recover_pending_apply`，根据 journal 阶段完成回滚，消除崩溃窗口。

为避免两份 env 解析规则长期漂移，新增 Rust `skill_env.rs` 必须兼容当前 `SkillEnvironmentManager` 的 managed header、base64 value metadata、shell quoting 和 `0600` 权限，并使用共享 golden fixtures；Swift manager 后续仍可读写同一格式。不得直接把远端 JSON 值拼进 shell 脚本。

## 12. ObjectStore 接口

Transport-independent trait 建议如下，具体类型可调整但语义不能缩减：

```rust
trait ObjectStore: Send + Sync {
    fn capabilities(&self) -> StoreCapabilities;
    fn test_connection(&self) -> Result<ConnectionReport, DeviceSyncError>;
    fn head(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, DeviceSyncError>;
    fn get_bounded(&self, key: &ObjectKey, max_bytes: u64, sink: &mut dyn Write)
        -> Result<ObjectMeta, DeviceSyncError>;
    fn put(&self, key: &ObjectKey, source: &mut dyn Read, len: u64, condition: PutCondition)
        -> Result<ObjectMeta, DeviceSyncError>;
    fn list(&self, prefix: &ObjectPrefix, cursor: Option<&str>)
        -> Result<ObjectPage, DeviceSyncError>;
    fn delete(&self, key: &ObjectKey, condition: DeleteCondition)
        -> Result<(), DeviceSyncError>;
    fn create_prefix(&self, prefix: &ObjectPrefix) -> Result<(), DeviceSyncError>;
}
```

要求：

- `ObjectKey` 只能由经过编码的相对段构造，不能接受完整 URL 或 `../`；
- provider 错误映射到统一错误码，并附带已脱敏、可本地化的参数；
- `get_bounded` 流式写 staging 文件并执行 Content-Length 与实际字节双重限制；
- PUT snapshot 使用 `If-None-Match: *` 或等价语义；Head 使用 `If-Match` / 版本条件；
- provider 不支持某能力时显式返回 capability，Engine 选择保守算法，不能假装原子。

第一阶段提供 `LocalFolderStore`，只用于协议、故障注入和自动化测试，不在正式 UI 作为云存储选项。

## 13. WebDAV（第二阶段）

支持坚果云及标准 WebDAV：

- `OPTIONS`/轻量 GET 测连；
- 逐级 `MKCOL`，把 405“已存在”当成功，其他状态保留结构化信息；
- `HEAD`、`GET`、`PUT`、`DELETE`；
- `PROPFIND Depth: 1` 列出历史，使用 XML parser，不用正则解析 XML；
- 正确编码每个路径 segment，禁止 endpoint query/fragment 混入对象 key；
- 使用 ETag 做条件更新；弱 ETag、缺 ETag 时走读取-比较-写入-复核的保守路径；
- 处理 401/403、404、409、412、423、429、5xx 和超时；
- 坚果云 UI 显示应用密码提示，不把网页登录密码描述为推荐凭据。

可参考 `cc-switch` 的 `webdav.rs`、`webdav_sync.rs`、`archive.rs`：MKCOL、bounded download、artifact-first/manifest-last、回滚和错误脱敏。不得照搬其中缺失 E2EE、不可变多设备 Head 和三方合并的单一备份协议。

`cockpit-tools` 只参考远端备份列表、删除、保留天数清理及“单个清理失败不影响其余项”的产品行为；代码按许可证规则独立实现。

## 14. S3 / OBS（第三阶段）

一个 S3 transport，配置显式区分 preset：

| Preset | Endpoint | Region | Addressing |
|---|---|---|---|
| AWS S3 | AWS 标准 endpoint，可自动构造 | 必填 | virtual-hosted，特殊 bucket/HTTPS 情况回退需明确 |
| Huawei OBS | 用户选择 region 或输入 OBS endpoint | 必填 | 按 OBS 兼容性实测决定，配置中显式保存 |
| MinIO / Custom | 用户输入 | 必填或默认 `us-east-1` | 默认 path-style，可切换 |

实现要求：

- AWS Signature Version 4，canonical URI/query/header 必须有官方 test vector；
- 支持临时凭据 `x-amz-security-token`；
- 支持 `HeadObject`、`GetObject`、`PutObject`、`ListObjectsV2`、`DeleteObject`；
- 分页处理 continuation token，XML 使用 parser；
- snapshot 使用 `If-None-Match: *`；Head 条件写按服务能力验证，失败回到保守冲突检测；
- endpoint 只允许 HTTPS，开发/局域网 MinIO 的 HTTP 必须单独开启 insecure 开关并显示警告；
- region/host/path-style 参与签名时必须与最终请求完全一致；
- 网络错误中不得输出 Secret、Session Token、Authorization 或完整 canonical request。

可参考 `cc-switch` 的 `s3.rs`、`s3_sync.rs`：SigV4 结构、AWS virtual-hosted 与自定义 path-style、错误脱敏和 transport-independent protocol。不能把其“UI 有 OBS 选项”视为 OBS 已验证；TokenViewer 必须对 AWS、MinIO、华为 OBS 各跑一次独立 live test。

## 15. 自动同步、历史与增量（第四阶段）

### 15.1 自动同步

- watcher 事件进入容量 1 的 dirty queue；多次变化折叠；
- 1 秒 debounce，持续写入最多等 10 秒；
- 全局 operation mutex 保证手动/自动操作串行；
- Pull/Apply 使用 suppression guard，结束后重新扫描一次确认是否存在用户并发修改；
- 启动、唤醒和网络恢复只触发“检查远端”，检测到冲突时通知用户，不自动覆盖；
- 指数退避并加 jitter；401/403 等凭据错误停止自动重试，网络错误可重试；
- 菜单栏生命周期内不得启动无法取消的常驻线程。

可参考 `cc-switch` 的 `webdav_auto_sync.rs`、`s3_auto_sync.rs` 的 dirty queue、debounce、suppression guard 和全局锁；重新实现以适配 TokenViewer CoreHandle 生命周期。

### 15.2 历史与 retention

- 默认保留最近 30 个 snapshot、至少 30 天，并保留所有当前 Head 可达祖先；
- prune 先计算引用图并 Preview，再删除；Head、vault.json、未知新协议对象永不自动删除；
- 单对象删除失败记录结果并继续，最后返回 partial failure；
- orphan snapshot 至少宽限 7 天再清理，避免刚上传但 Head 尚未发布的合法操作被删除。

### 15.3 增量 blobs

- 文件内容以 SHA-256 寻址并独立 AEAD 加密；manifest 只引用 blob；
- 加密 key 从 VMK + blob hash 派生，仍使用随机 nonce；相同明文是否泄露由 Vault 内去重策略明确说明；
- 上传缺失 blob 后才发布 snapshot；prune 使用所有保留 snapshot 的 mark-and-sweep；
- 第四阶段迁移必须继续读取 v1 整包 snapshot，新写格式通过 feature/capability version 区分。

## 16. Rust 文件与依赖计划

建议新增：

```text
core/src/device_sync/
  mod.rs
  models.rs
  config.rs
  skill_env.rs
  snapshot.rs
  archive.rs
  crypto.rs
  merge.rs
  engine.rs
  store/
    mod.rs
    local.rs
    webdav.rs
    s3.rs
```

修改：

- `core/src/lib.rs`：导出 `device_sync`；
- `core/src/ffi.rs`：CoreHandle 增加 engine/mutex，添加 FFI；
- `core/Cargo.toml`：加入经审计的 HTTP、AEAD、Argon2id、HKDF/HMAC/SHA-256、zeroize、归档和 XML 依赖；
- `core/tests/`：按 transport/crypto/merge/rollback 拆分集成测试。

依赖选择原则：

- 优先 RustCrypto 生态；禁用不需要的默认 feature；
- HTTP client 选择可打进 macOS staticlib、无需用户安装 OpenSSL 的 TLS 方案；
- 不重复引入已有 `zstd`、`serde`、`uuid` 能力；
- 增加依赖前记录许可证、维护状态、最低 Rust 版本、二进制体积和跨平台影响；
- `cargo deny` 若项目尚未引入，不为本功能强制增加工具链，但 PR 中要给出 `cargo tree` 和 license review。

## 17. FFI 契约

建议函数：

```text
tt_device_sync_get_config
tt_device_sync_set_config
tt_device_sync_test_connection
tt_device_sync_create_vault
tt_device_sync_join_vault
tt_device_sync_preview_push
tt_device_sync_push
tt_device_sync_preview_pull
tt_device_sync_prepare_apply
tt_device_sync_commit_apply
tt_device_sync_rollback_apply
tt_device_sync_recover_pending_apply
tt_device_sync_list_snapshots
tt_device_sync_preview_restore
tt_device_sync_prune
tt_device_sync_get_status
```

每个函数接收一个 JSON request，返回统一 envelope：

```json
{
  "ok": false,
  "data": null,
  "error": {
    "code": "stale_preview",
    "message_key": "deviceSync.error.stalePreview",
    "arguments": { "device_name": "MacBook" },
    "retryable": true,
    "operation_id": "uuid"
  }
}
```

- Swift 只根据 `code`/`message_key` 本地化，禁止解析自由文本决定流程。
- request 内的凭据只用于该次调用；配置保存请求不得包含密码字段。
- 所有返回 C string 继续由 `tt_free_string` 释放。
- FFI 对 panic 使用现有边界策略转换为 `internal_error`；不能 unwind 进入 Swift。
- Preview 模型至少返回 add/update/delete/conflict/skipped/warning 数量和逐项信息，但环境变量不返回 value。
- Push Preview 也必须显示将上传的文件数、总大小、排除项和内容来源，便于发现误打包。

核心错误码至少包括：`invalid_config`、`credential_missing`、`authentication_failed`、`network_unreachable`、`rate_limited`、`protocol_unsupported`、`vault_not_found`、`vault_auth_failed`、`object_too_large`、`archive_unsafe`、`integrity_failed`、`operation_in_progress`、`stale_preview`、`remote_changed`、`conflict_requires_resolution`、`link_target_occupied`、`apply_failed`、`rollback_failed`、`partial_failure`。

## 18. Swift 文件与状态模型

建议新增：

```text
macos/TokenViewer/
  Models/DeviceSyncModels.swift
  ViewModels/DeviceSyncViewModel.swift
  Services/DeviceSyncCredentialStore.swift
  Bridge/CoreBridge+DeviceSync.swift
  Views/DeviceSync/DeviceSyncSettingsView.swift
  Views/DeviceSync/DeviceSyncPreviewSheet.swift
  Views/DeviceSync/DeviceSyncConflictSheet.swift
  Views/DeviceSync/DeviceSyncHistorySheet.swift
```

修改：

- `Views/SettingsView.swift`：sidebar 增加 `deviceSync`，SF Symbol 建议 `arrow.triangle.2.circlepath.icloud`；
- `Bridge/TokenViewer-Bridging-Header.h`：声明 FFI；
- `Services/KeychainManager.swift`：保持现有环境变量文件兼容，并把 shell source block 的 ensure 操作提取为可测试的内部 API；
- `Services/Localization.swift`：中英文完整键；
- `macos/project.yml`：源文件由现有目录 glob 收录；新增文件后执行 `xcodegen generate` 并提交生成的 `project.pbxproj`；
- `TokenViewerTests/`：Keychain、ViewModel generation guard、JSON contract、L10n parity 测试。

ViewModel 状态必须是互斥枚举，不用多个可能矛盾的 Bool：

```text
idle
loadingConfig
testingConnection
creatingVault
joiningVault
previewing(direction)
awaitingConfirmation(preview)
syncing(direction, progress)
resolving(conflicts)
succeeded(summary)
failed(error)
```

异步请求捕获 generation/profile ID；用户切 provider、关闭页面或开始新请求后，旧结果不得覆盖当前状态。所有阻塞 FFI 通过 `Task.detached` 执行，UI 发布回到 `@MainActor`。

## 19. 设置与交互设计

在 Settings sidebar 新增独立“设备同步”页面，保持现有 `SettingsCard`、按钮和间距系统，不嵌套卡片。

页面结构：

1. 状态：Vault、当前设备、上次同步、远端状态、错误或冲突；
2. 存储：WebDAV / S3 segmented control；
3. S3 preset：AWS S3 / Huawei OBS / MinIO or Custom；
4. 凭据：固定 label、SecureField、Reveal icon、内联校验；
5. 内容：Cloud / Git 单选，以及 Skills、Agent links、Environment、Preferences 组件；
6. 操作：测试连接、保存、创建/加入 Vault、立即同步；
7. 历史：第四阶段提供历史与清理入口。

交互要求：

- 测试连接、保存和同步运行时禁用重复提交，并显示 `ProgressView`；
- 错误紧邻字段或动作，不能只用 toast；
- Pull/Restore 必须先展示按 Skills、Agent links、Environment、Preferences 分组的 Preview；
- 删除、覆盖、Force 类动作使用明确确认文案；不提供一个含糊的“解决全部”按钮；
- Conflict Sheet 逐项选择，支持“本地”“远端”“保留两份”（仅 Skill），默认不预选破坏性答案；
- 环境变量只显示名称和状态，不显示值；
- 环境变量同步默认关闭；开启后先按 Skill 选择，再允许取消单个变量，不能用一个开关静默上传全部 managed values；
- 颜色不是唯一信号，状态同时使用 SF Symbol 和文本；
- 所有可见字符串通过 `L10n`，中英文 key 数量一致；
- 支持 VoiceOver label、键盘焦点和 Dynamic Type；布局必须容纳中英文最长文本；
- Reduced Motion 下不使用无限旋转自定义动画，使用系统静态/标准 `ProgressView`。

## 20. 参考项目与许可证边界

### 20.1 cc-switch

`/Users/wangsw/webkong/cc-switch` 为 MIT。可以移植确有价值的代码，但必须：

- 在新增源码或 `THIRD_PARTY_NOTICES` 中保留 Jason Young 2025 的 MIT notice；
- 记录移植文件和本地修改；
- 适配 TokenViewer 的 Rust Core/FFI、E2EE 和多设备协议，不能整体复制 Tauri command/UI。

重点参考：

| 模块 | 可借鉴 | 必须补齐 |
|---|---|---|
| `sync_protocol.rs` | manifest/version/hash/size/mutex/rollback | E2EE、不可变 heads、parents/tombstones |
| `webdav.rs` | MKCOL、GET/PUT/HEAD、ETag、bounded response | PROPFIND parser、条件写能力、统一 ObjectStore |
| `webdav_sync.rs` | artifact-first/manifest-last | snapshot-first/head-last、多设备冲突 |
| `archive.rs` | deterministic archive、zip-slip/zip bomb | TokenViewer 数据模型和加密封装 |
| `s3.rs` | SigV4、virtual/path-style、脱敏 | session token、list/delete、OBS 实测 |
| `s3_sync.rs` | transport-independent protocol | Vault/Head 协议 |
| `*_auto_sync.rs` | dirty queue、debounce、suppression、mutex | CoreHandle 生命周期和冲突通知 |

### 20.2 cockpit-tools

`/Users/wangsw/webkong/TokenViewer/cockpit-tools` 的 crate 声明为 `CC-BY-NC-SA-4.0`。TokenViewer 当前许可并不自动消除 ShareAlike 等义务。除非取得作者单独书面授权：

- 禁止直接复制、改写或逐行翻译其实现；
- 只记录公开可观察的产品行为和高层设计；
- Luna 不得把它作为 copy source；实现必须基于本方案、标准协议文档和 TokenViewer 自有代码；
- 可借鉴的行为限于远端备份列表、删除/按天保留、migration preview、rollback、局部清理失败继续。

Review 时通过相似代码片段、注释风格和错误文本抽查来源；发现实质性复制则退回重写。

## 21. 分阶段实施任务

每阶段单独 PR/提交组、独立测试和手动验收。未达到阶段 gate 不进入下一阶段。

### 阶段一：Foundation、Local Store、E2EE 与手动恢复

- [ ] 写 models/config/protocol，固定 v1 canonical serialization 和资源上限。
- [ ] 实现 Skill/links/env/preferences 的 Snapshot Builder 和排除规则。
- [ ] 实现 deterministic archive、安全解包和测试。
- [ ] 实现 Argon2id key wrap、HKDF、XChaCha20-Poly1305、zeroize。
- [ ] 实现 `LocalFolderStore` 与故障注入测试。
- [ ] 实现 create/join vault、push/pull Preview、Apply、rollback。
- [ ] 接入现有 `SymlinkManager` 与 `SkillEnvironmentManager`，验证 pending links。
- [ ] 添加统一 FFI envelope 和 Swift typed bridge。
- [ ] 添加设置页、Keychain、Preview Sheet 和完整 L10n。
- [ ] 完成 Phase 1 自动化与手动双设备目录模拟。

Gate：在两个临时 home/source root 间完成加密导出/预览/恢复；错误密码、篡改、危险归档、目标占用和注入式 Apply 失败都不改变原数据；回滚测试通过。

阶段一属于内部 foundation：正式版 Settings 不显示一个无法连接云端的空壳入口。Local Store 只通过测试 harness 或 Debug-only 本地目录入口验证。阶段二 WebDAV 达到 Gate 后再向用户开放 Device Sync 页面；S3/OBS 选项到阶段三通过各自 Gate 后才显示。

### 阶段二：WebDAV / 坚果云

- [ ] 实现 WebDAV ObjectStore、XML PROPFIND、路径编码和条件写。
- [ ] UI 增加坚果云预设、应用密码说明和连接诊断。
- [ ] 增加 mock server 合约测试：状态码、弱/无 ETag、截断、超限、并发变化。
- [ ] 使用测试账户运行 ignored live test；凭据只从环境变量/Keychain 注入。
- [ ] 手动完成 Mac A push -> Mac B preview/apply -> B push -> A conflict/stale preview。

Gate：坚果云 live test 和标准 WebDAV mock contract 均通过；断网/401/412/超限不会损坏本地或发布坏 Head。

### 阶段三：AWS S3、Huawei OBS、MinIO

- [ ] 实现 SigV4、session token、addressing style 和 S3 ObjectStore。
- [ ] 实现 ListObjectsV2 分页与 DeleteObject。
- [ ] UI 增加三个 preset 及 endpoint/region/bucket/prefix 校验。
- [ ] 运行 AWS 官方 SigV4 vector 与本地 MinIO contract tests。
- [ ] 分别运行 AWS S3、华为 OBS、MinIO ignored live tests。
- [ ] 验证 unicode/space path、临时凭据、403、region redirect、clock skew 和条件写。

Gate：三个真实后端均完成 create/join/push/pull/list/delete；OBS 必须有独立测试证据，不能由“S3 兼容”推断通过。

### 阶段四：自动同步、历史、三方合并与增量

- [ ] 引入 watcher dirty queue、debounce、suppression 和退避。
- [ ] 完成 parent graph、共同祖先搜索、HLC、tombstone/OR-set merge。
- [ ] 完成 Conflict Sheet 和 merge snapshot。
- [ ] 完成历史列表、Restore Preview、retention、orphan cleanup。
- [ ] 增加 content-addressed blob 协议及 v1 整包兼容读取。
- [ ] 加入 wake/network recovery、取消和 app termination 测试。
- [ ] 做 3 设备并发、离线、时钟回退、删除复活、prune 引用图测试。

Gate：三设备随机操作模型测试无静默丢失；任何 unresolved conflict 都不会自动 push；历史恢复可回滚；prune 不删除 Head 可达数据。

## 22. 自动化测试矩阵

### Rust 单元/集成测试

- deterministic archive，同输入得到相同明文归档 hash；
- `.git` 精确排除但合法隐藏文件保留；
- symlink 不越界、循环、绝对路径和大小写碰撞；
- zip-slip/tar traversal、zip bomb、重复路径、条目/大小/压缩比上限；
- encrypt/decrypt、wrong password、nonce 唯一、tamper、AAD mismatch、zero-length；
- vault/protocol 向前拒绝和未知字段兼容；
- snapshot-first/head-last，所有步骤故障注入；
- Preview token 过期、remote/local fingerprint 改变；
- Prepare/Commit 每一步失败和启动时 pending journal recovery 都恢复原 Skills、links、env、preferences；
- HLC 时钟回退、tombstone、OR-set、共同祖先、同 Skill/env 冲突；
- WebDAV URL/path/XML/ETag/status/bounded body；
- SigV4 官方 vector、virtual-host/path-style/session token/ListObjectsV2；
- 凭据、环境变量值和 Authorization 不出现在 error/debug 输出。

### Swift 测试

- Keychain save/read/delete、profile 隔离和空值；
- 表单按 provider/preset 的校验；
- FFI envelope、Preview/Conflict/History decoding；
- generation guard，旧异步结果不能覆盖新 profile；
- 运行中禁用重复动作；
- destructive confirmation 与 stale preview 流程；
- 凭据从不进入 UserDefaults/config JSON；
- 中英文 key parity；
- 环境变量 Preview 不展示 value。

### Live tests

live tests 默认 `#[ignore]`，从环境变量读取专用测试 bucket/path 和短期凭据，测试后只清理随机 test prefix。至少定义：

```text
TOKENVIEWER_TEST_WEBDAV_*
TOKENVIEWER_TEST_AWS_S3_*
TOKENVIEWER_TEST_OBS_*
TOKENVIEWER_TEST_MINIO_*
```

CI 不保存个人坚果云密码或长期云密钥。若未来加入 CI，使用 OIDC/短期凭据和隔离 bucket。

## 23. 构建与验收命令

每个 Rust/Swift 阶段完成后执行：

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" rtk cargo test --lib --tests

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
rtk xcodebuild test -project macos/TokenViewer.xcodeproj \
  -scheme TokenViewer -destination 'platform=macOS'

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
rtk bash run.sh
```

Device Sync 修改 Rust Core，所以必须运行完整 `run.sh`，不能使用 `--skip-sync`。新增 Swift 文件后先运行：

```bash
rtk xcodegen generate --spec macos/project.yml
```

验收必须记录 `BUILD SUCCEEDED`、测试数量、ignored live test 的 provider 和结果。除非用户明确要求，不构建 dmg/pkg、不发布、不 push。

## 24. 手动验收场景

至少使用两个独立临时 home/profile 或两台 Mac：

1. A 创建 Vault，包含普通 Skill、隐藏文件、内部链接、环境变量和两个 Agent 关联；
2. B 用错误密码加入，确认无本地改动；再用正确密码 Preview；
3. B 的一个 Agent 未安装，Apply 后 Skill/env 成功、链接显示 pending；
4. 安装/模拟该 Agent 后重建链接，确认目标正确；
5. B 修改不同 Skill 并 Push，A Pull 自动合并/应用；
6. A/B 修改同一个 Skill 与同一环境变量，确认冲突而非覆盖；
7. Preview 后远端变化，Apply 返回 stale；
8. 目标路径预先放真实目录，确认阻塞且不删除；
9. Apply 中途注入失败，确认 Skills、links、env 全部恢复；
10. 篡改 snapshot/head，确认认证失败；
11. Git 模式只同步关系/env/preferences，远端 snapshot 不含 Skill 文件；
12. 检查日志、config、UserDefaults、远端明文均无凭据和环境变量值。

## 25. 迁移、回滚与发布风险

- 现有用户默认关闭 Device Sync，不自动创建 Vault，不扫描或上传任何数据。
- 首次启用只读生成 Push Preview，用户确认后才上传。
- 本地 config 使用 schema version 和原子写；迁移前备份，失败继续以旧配置启动。
- wire protocol 只追加兼容字段；破坏性变化使用新 protocol version 和显式迁移。
- 功能开关按 provider 分阶段开放；Phase 2 不因 Phase 3 未完成而暴露假 S3/OBS 入口。
- 禁用功能只停止自动操作，不删除远端 Vault；删除远端数据是独立、二次确认动作。
- 忘记同步密码且所有设备 Keychain 都丢失时，Vault 不可恢复。创建时必须明确提示，不提供弱安全后门。
- 对象存储服务可回滚/删除对象，E2EE 能检测篡改但无法完全阻止服务端回放或可用性攻击；本地 sequence 检测只能在保留 state 时发现回退。
- 新 HTTP/crypto/archive 依赖可能增加 staticlib 和 App 体积，阶段一记录前后体积。
- 自动同步默认关闭，第四阶段完成压力测试后才能作为 opt-in 发布。

## 26. Luna 执行契约

交给 Luna 时必须附带本节：

1. 按阶段工作，一次只实现一个阶段；阶段内按小任务提交 diff，不跨阶段预埋未测试 UI。
2. 开始前阅读根目录 `CLAUDE.md`/`AGENTS.md`、本方案及相关现有实现；永远不直接编辑 `AGENTS.md`。
3. 保持改动最小，不重构无关 Skills、Git Sync、用量同步或 Settings 页面。
4. 不修改、不删除、不提交现有未跟踪 `.ai/` 和 `default.profraw`。
5. 所有密码、Token、Secret、环境变量值和 VMK 禁止进入 fixture、snapshot、日志和提交历史；测试使用假值。
6. cockpit-tools 只能作为行为参考，禁止复制代码；cc-switch 代码若移植必须列出来源并保留 MIT notice。
7. 先写失败测试，再实现协议、安全边界和回滚；网络 happy path 不能替代故障注入。
8. 不弱化本方案中的预览、基线复核、资源上限、E2EE、冲突和回滚要求来缩短实现。
9. 新增 Swift 文件后运行 XcodeGen；修改 Rust 后运行 Rust tests、Swift tests 和完整 `run.sh`。
10. 不提交或 push，除非用户另行明确要求。交付时提供文件清单、依赖/许可证、测试命令与结果、未完成项和已知风险。

每阶段交付模板：

```text
Phase:
Implemented:
Protocol/schema changes:
Security decisions:
Files changed:
Dependencies and licenses:
Automated tests and results:
Live/manual verification:
Known gaps:
Worktree status:
```

## 27. 最终 Codex Review 清单

Luna 完成后由 Codex 按严重级别 Review，至少检查：

- [ ] 范围：没有同步 data.db、会话、凭据、绝对路径或真实 Agent 链接。
- [ ] 来源：Git/Cloud 内容写入互斥，cockpit-tools 无实质复制，cc-switch notice 完整。
- [ ] Crypto：CSPRNG、Argon2 参数、nonce、AAD、HKDF context、zeroize、错误和日志无泄漏。
- [ ] Archive：路径规范化、链接、大小/条目/压缩比限制在扫描和解包两侧都有效。
- [ ] Protocol：snapshot 不可变、先对象后 Head、Head 认证、parents/HLC/tombstone 从 v1 存在。
- [ ] Concurrency：互斥、stale preview、条件写、watch suppression、取消与 app lifecycle 正确。
- [ ] Apply：Preview token 绑定基线，完整 rollback，目标占用不被删除，pending link 可恢复。
- [ ] Providers：WebDAV XML/ETag，SigV4/session token/addressing/list/delete，OBS 有独立证据。
- [ ] UI：状态机无竞态、字段内联错误、破坏性确认、无凭据持久化、L10n/VoiceOver/Reduced Motion。
- [ ] Tests：错误密码、篡改、超限、并发、三方冲突、删除复活、故障注入和三个 S3 类后端证据。
- [ ] Build：Rust/Swift tests、完整 `run.sh` 和 `BUILD SUCCEEDED`；无无关 diff。

Review 输出先列 finding（严重级别、文件和行号、复现/影响），再列问题和测试缺口；只有全部 blocker/high 修复并重新验证后，才建议进入下一阶段或发布。
