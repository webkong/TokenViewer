# Code Review Report — TokenViewerNew

**Date**: 2026-06-01  
**Scope**: Full project (`core/` Rust + `macos/` Swift)  
**Status**: P1 骨架 + P2 功能实现完成，首次全面审查

---

## Summary

| Severity | Rust | Swift | Total |
|----------|------|-------|-------|
| Critical | 3 | 1 | **4** |
| High | 5 | 4 | **9** |
| Medium | 8 | 4 | **12** |
| Low | 7 | 5 | **12** |
| **Total** | **23** | **14** | **37** |

## Fix Status (updated 2026-06-01)

**Fixed**: C1, C2, C3, C4 (all Critical) · H1, H2, H3, H4, H5, H7, H8, H9 · M2, M3, M5, M8, M11(partial), M12 · L1, L2, L7, L9.
**Also fixed a correctness bug not in original review**: cost was never computed (queries returned `total_cost_usd: 0`). Added `aggregate_by_model`/`aggregate_by_day_model` in db, wired `pricing::compute_row_cost` into FFI summary/daily/model_breakdown, and switched pricing to longest-prefix matching with family keys. Verified: 7 models, total $821.08.
**Deferred** (low impact): H6 (pricing HashMap — still linear scan, fine for current table size), M1, M4, M6, M7, M9, M10, L3-L6, L8, L10-L12.

---

## Critical Issues (Must Fix Before Release)

### C1. FFI 空指针未检查 — Undefined Behavior
**Files**: `core/src/ffi.rs` (all FFI functions)  
**Impact**: 如果 Swift 端传入 null handle（如 `tt_init` 失败后仍调用其他函数），直接解引用导致 UB/crash。  
**Fix**: 每个 FFI 函数入口添加 `if handle.is_null() { return std::ptr::null_mut(); }`

### C2. FFI `tt_init` 中 `db_path` 空指针
**File**: `core/src/ffi.rs:20`  
**Impact**: `CStr::from_ptr(null)` 是 UB。  
**Fix**: 添加 `if db_path.is_null() { return std::ptr::null_mut(); }`

### C3. FFI `from`/`to` 参数空指针
**File**: `core/src/ffi.rs:85-112`  
**Impact**: 空字符串作为 SQL 参数导致不可预期的查询结果。  
**Fix**: null 时返回 null 或 error JSON。

### C4. CoreBridge 线程安全 — 多线程并发访问 FFI handle
**File**: `macos/TokenViewer/Bridge/CoreBridge.swift`  
**Impact**: `Task.detached` 后台调用 `syncAll()` 与主线程 `querySummary()` 并发访问同一 handle，数据竞争。  
**Fix**: 使用串行 `DispatchQueue` 保护所有 FFI 调用。

---

## High Issues (Should Fix Soon)

### H1. `UsageViewModel.sync()` 中 self 强引用
**File**: `ViewModels/UsageViewModel.swift:68-74`  
**Impact**: Task 延长 ViewModel 生命周期，可能更新已销毁的 view。  
**Fix**: 使用 `[weak self]` 捕获。

### H2. CoreBridge 单例 deinit 不保证调用
**File**: `Bridge/CoreBridge.swift:12-17`  
**Impact**: Rust 侧 `tt_destroy` 未调用，WAL 数据可能未 flush。  
**Fix**: 在 `applicationWillTerminate` 中显式调用 shutdown。

### H3. PopoverView 和 MainWindowView 各自创建独立 ViewModel
**File**: `Views/PopoverView.swift:4`, `Views/MainWindowView.swift:4`  
**Impact**: 重复 sync、数据不一致。  
**Fix**: 共享单一 ViewModel 实例。

### H4. `epoch_secs_to_bucket` 对无效时间戳静默回退
**File**: `parsers/utils.rs:30`  
**Impact**: 极端时间戳被错误归类到当前时间段。  
**Fix**: 对无效值返回 None。

### H5. Gemini/Cursor 解析器无文件大小限制
**File**: `parsers/gemini.rs:28`, `parsers/cursor.rs:22`  
**Impact**: 超大文件可能导致 OOM。  
**Fix**: 添加文件大小上限检查（如 100MB）。

### H6. `pricing/engine.rs` 线性扫描定价表
**File**: `pricing/engine.rs:8-18`  
**Impact**: 当前 18 条目无影响，但扩展到 2200+ 模型时性能瓶颈。  
**Fix**: 使用 HashMap 做精确匹配。

### H7. `UsageViewModel.refresh()` 在主线程同步调用 FFI
**File**: `ViewModels/UsageViewModel.swift:60-70`  
**Impact**: 大数据库查询阻塞主线程，UI 卡顿。  
**Fix**: 将查询移到后台 Task。

### H8. SettingsView.resetData() 不重置 CoreBridge handle
**File**: `Views/SettingsView.swift:72-74`  
**Impact**: 删除 DB 后 FFI 调用行为未定义。  
**Fix**: 重置后重新初始化或提示重启。

### H9. `sync/scheduler.rs` 中 upsert 失败时 cursor 未更新
**File**: `sync/scheduler.rs:40-44`  
**Impact**: 下次同步重复处理已成功的记录（幂等但浪费）。  
**Fix**: 使用事务或继续处理剩余记录。

---

## Medium Issues

| # | File | Issue | Fix |
|---|------|-------|-----|
| M1 | `storage/db.rs:18` | `migrate` 中 `unwrap_or(0)` 掩盖数据库损坏 | 区分错误类型 |
| M2 | `sync/scheduler.rs:38` | `records_added` u32 可能溢出 | 用 `saturating_add` |
| M3 | `parsers/utils.rs:16` | `bucket_30min` 中 `unwrap()` | 用 `expect()` |
| M4 | `parsers/claude.rs:37-39` | `hour_start` 可能为空字符串 | 回退到 file mtime |
| M5 | `parsers/codex.rs:13` | `.or()` 应为 `.or_else()` | 延迟求值 |
| M6 | `ffi.rs` 全文件 | 缺少 `/// # Safety` 文档 | 添加安全文档 |
| M7 | `sync/scheduler.rs:40-44` | 错误时丢弃已解析记录 | 事务或继续处理 |
| M8 | `pricing/engine.rs:14-16` | 前缀匹配方向可能错误 | 确认语义 |
| M9 | `Views/UsageView.swift:14` | onChange 需 macOS 14+ | 确认 deployment target |
| M10 | `StatusBarController.swift:15` | target weak 引用无注释 | 添加生命周期注释 |
| M11 | `models.rs:88` vs `pricing/engine.rs:22` | compute_cost 逻辑重复 | 统一实现 |
| M12 | `storage/db.rs:107` | `query_heatmap` weeks 未校验 | 添加 `max(1)` |

---

## Low Issues

| # | File | Issue |
|---|------|-------|
| L1 | `Bridging-Header.h` | 缺少 `_Nullable`/`_Nonnull` 标注 |
| L2 | `UsageViewModel.swift:79` | DateFormatter 每次创建 |
| L3 | `PopoverView.swift:155` | SparklineView 空数据保护 |
| L4 | `UsageViewModel.swift:4-10` | snake_case 属性名不符 Swift 规范 |
| L5 | `PopoverView.swift:4` | onOpenMainWindow 可选闭包无默认值 |
| L6 | `lib.rs:6` | `pub mod ffi` 不应公开 |
| L7 | `parsers/mod.rs:56-61` | 错误被静默吞掉 |
| L8 | `parsers/utils.rs:119-131` | `vscode_global_storage` 缺兜底 cfg |
| L9 | `pricing/data.rs` | ModelPricing 应 derive Copy |
| L10 | `Cargo.toml` | crate-type 已正确配置 ✓ |
| L11 | `parsers/mod.rs` | 22 个 parser 无单元测试 |
| L12 | 项目级 | 缺少 CI 配置 |

---

## Recommended Fix Priority

1. **Immediate** (before any user testing):
   - C1-C4: FFI 空指针检查 + CoreBridge 线程安全
   - H1: weak self in Task

2. **Before Beta**:
   - H2-H3: 单例生命周期 + ViewModel 共享
   - H7: 主线程 FFI 调用移到后台
   - M4: Claude parser hour_start 空字符串

3. **Before Release**:
   - H4-H6: 防御性编程
   - M1-M12: 错误处理和代码质量
   - L1-L12: 代码风格和文档

---

## Architecture Notes

**优点**:
- Rust 核心 + Swift UI 分层清晰
- FFI 接口设计简洁（JSON 字符串传递，避免复杂结构体映射）
- Parser 模块化好，每个 provider 独立文件
- SQLite WAL 模式适合读多写少场景

**改进建议**:
- 考虑添加日志系统（`log` crate + `oslog` Swift 端）
- Parser 应有单元测试（用 fixture 数据）
- 考虑 `uniffi` 替代手写 FFI（类型安全、自动生成 Swift 绑定）
