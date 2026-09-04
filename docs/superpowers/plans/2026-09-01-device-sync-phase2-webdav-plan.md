# Device Sync Phase 2：WebDAV / 坚果云实施计划

**目标**：在已提交的 Phase 1 foundation 之上，实现可用于标准 WebDAV 和坚果云的 `ObjectStore`，完成连接诊断、Keychain 凭据接入和分阶段 Settings 入口。完成本计划后，Device Sync 才允许在正式 UI 中选择 WebDAV；S3/OBS/MinIO 不在本阶段暴露。

## 范围

- Rust WebDAV `ObjectStore`：HTTPS、Basic Auth、PROPFIND、GET/Range、PUT、DELETE、ETag 条件写。
- 坚果云预设：DAV endpoint、应用密码提示、用户名配置；不得把网页登录密码作为推荐凭据。
- Swift Keychain：复用 Phase 1 的 `DeviceSyncCredentialStore`，凭据只通过内存传入 Rust，不写入 `config.json` 或快照。
- 连接测试和可诊断错误：HTTP 状态、重试建议、provider 信息，不记录 Authorization、密码或响应敏感内容。
- UI 分阶段开放 WebDAV；保持 S3/OBS/MinIO 隐藏或禁用。

## 不在范围内

- S3 SigV4、华为 OBS、MinIO。
- 自动同步、历史 retention、三方 merge、增量 blobs。
- 上传 Git 凭据、对象存储密码、Vault Master Key。
- 修改 frontier、快照格式、Apply/rollback 事务协议，除非为 WebDAV 兼容性发现必须的安全修复。

## 实施步骤

### 1. Provider 与请求层

- 新建 `core/src/device_sync/store/webdav.rs`，实现 `ObjectStore`。
- 使用现有依赖；只有在标准库无法安全完成 XML/HTTP 时才增加最小依赖，并说明原因。
- 所有 URL path segment 使用 RFC 3986 编码，禁止 endpoint 中的 userinfo、控制字符和路径逃逸。
- PROPFIND 使用有限 depth/响应体上限，解析 `<d:response>`、`<d:href>`、`getcontentlength`、`getetag`、`resourcetype`；拒绝 XML 解析失败、重复/越界对象和非预期路径。
- `get_prefix` 优先使用 HTTP Range；服务器忽略 Range 时必须限制读取量并返回明确错误，不能把完整大对象无界载入内存。
- PUT 使用 `If-None-Match: *` 或 `If-Match: <etag>`；服务器返回 412/409 映射为 `RemoteChanged`/`ImmutableObjectConflict`，不能静默覆盖。
- 创建目录只允许在配置 remote prefix 下，拒绝软链接语义、绝对路径和 `..`。

### 2. 配置与 Keychain

- 扩展 provider 配置 Codable 模型，兼容未知字段。
- `config.json` 仅保存 endpoint、remote_prefix、username 和 provider kind；密码不落盘。
- WebDAV 密码按 `<profile-id>:webdav-password` 保存到 `com.tokenviewer.device-sync`。
- 读取凭据失败返回 `CredentialMissing`；切换 profile/provider 时清除旧 provider 的内存凭据引用。

### 3. Swift Bridge / UI

- 在 `CoreBridge+DeviceSync.swift` 增加 provider 凭据注入和连接测试调用，继续经 `DeviceSyncApplyCoordinator` gate 和后台任务执行。
- Settings 增加 WebDAV/坚果云预设、endpoint、用户名、应用密码输入、测试连接按钮和非敏感诊断结果。
- 所有文案进入 `Localization.swift`，提供 zh-CN/en。
- 未通过 Phase 2 gate 前不显示 S3/OBS/MinIO 选项；不创建不可用的空壳入口。

### 4. 测试

- Rust 单元测试：URL 编码、remote prefix 边界、PROPFIND XML 命名空间/截断/重复响应、Range bounded body、ETag 条件写、401/403/404/409/412/429/5xx 映射。
- Rust mock contract：完整覆盖 `head/get/get_prefix/put/list/delete/create_prefix`，验证快照上传顺序和坏 Head 不会发布。
- Swift 测试：Keychain service/account 隔离、坚果云预设、密码不出现在日志/JSON、连接错误展示、recovery gate 不被绕过。
- 运行：`rtk cargo test --manifest-path core/Cargo.toml --lib --tests`、`rtk xcodebuild test ...`、`git diff --check`。

## 安全与兼容要求

- TLS 默认开启；不提供关闭证书校验的开关。
- 日志只允许 provider、HTTP status、非敏感对象后缀或哈希；禁止 Authorization header、密码、XML 响应全文和签名材料。
- 所有下载继续受单对象、metadata、payload、压缩比和 graph 节点上限约束。
- WebDAV 条件写能力不足时必须在写前后重新读取并比较版本，发生变化返回冲突。
- 保持 LocalFolderStore 行为和 Phase 1 测试不变。

## 阶段 Gate

只有全部满足才进入 Phase 3：

- 标准 WebDAV mock contract 全部通过。
- 坚果云 live test：创建目录、上传不可变 snapshot、读取/list、条件更新 head、401/412/断网恢复均有证据。
- 失败和超限场景不会修改本地 Skills、不会删除 rollback、不会发布坏 Head。
- Rust 与 Swift 测试通过，`git diff --check` 通过。
- Review 无 Blocker/High；提交只包含 Phase 2 文件，不 push，等待人工验收。
