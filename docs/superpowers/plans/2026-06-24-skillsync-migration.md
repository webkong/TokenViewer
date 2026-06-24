# SkillSync Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate all SkillSync functionality into TokenViewer as a first-class Skill Manager module, then stop maintaining the standalone SkillSync app.

**Architecture:** TokenViewer remains one native macOS menu-bar app with one Rust static library. SkillSync's Rust capability moves into `core/src/skills/`, exported through namespaced `tt_skills_*` FFI functions and consumed by new Swift `SkillManager` view models/views inside the main TokenViewer window. The standalone SkillSync project is treated as read-only migration source, not a shared runtime dependency.

**Tech Stack:** SwiftUI macOS 14, Swift 5, Rust staticlib, C FFI with JSON strings, SQLite via rusqlite, git2-rs, notify-rs macOS kqueue, XcodeGen, TokenViewer `L10n` localization.

---

## Execution Context

Run this plan from:

```bash
cd /Users/Joy/JoyWorkSpace/TokenViewer/tokenviewer
```

Use SkillSync only as a source reference:

```text
/Users/Joy/JoyWorkSpace/skillsync
```

TokenViewer rules to preserve:

- Keep UI strings localized through `L10n`; do not hardcode visible English or Chinese strings in Swift views.
- Keep `AGENTS.md` untouched unless the user explicitly asks.
- Use XcodeGen project config in `macos/project.yml`; do not hand-edit a generated `.xcodeproj`.
- Build a testable `.app` after code changes.
- Do not push or release during this migration unless explicitly requested.

## Scope

Migrate these SkillSync capabilities:

- Skill scanning and manifest parsing.
- Built-in agent registry plus custom agents.
- Skill organize, restore, delete, and symlink control.
- Agent visibility and skill source root settings.
- Git status, pull, push, commit, remote/auth configuration.
- File watcher refresh for skill source changes.
- Main Skills, Agents, Sync, and Settings workflows, adapted into TokenViewer UI.

Do not migrate:

- SkillSync standalone app lifecycle.
- SkillSync app icon, About page, release scripts, website, or repository metadata.
- Separate SkillSync branding.

## Recommended Product Shape

Add a new `Skills` tab to TokenViewer's main window `TabView`.

```swift
SkillManagerView()
    .tag("skills")
    .tabItem { Label(l10n.skills, systemImage: "puzzlepiece.extension.fill") }
```

Keep the menu-bar popover focused on token usage. Skill management is too file-operation-heavy for the popover and should live in the full window.

## File Structure

### Rust Core

- Create: `core/src/skills/mod.rs`
  - Public module boundary for all Skill Manager Rust code.
- Create: `core/src/skills/models.rs`
  - Skill, agent, git, watcher, and command result structs.
- Create: `core/src/skills/agent_registry.rs`
  - Built-in and custom agent registry.
- Create: `core/src/skills/scanner.rs`
  - Skill directory scanning and manifest parsing.
- Create: `core/src/skills/symlink.rs`
  - Directory, single-file, and overlay link strategies.
- Create: `core/src/skills/git_engine.rs`
  - Git status, commit, pull, push, auth connectivity.
- Create: `core/src/skills/watcher.rs`
  - Optional file watcher with debounce.
- Create: `core/src/skills/storage.rs`
  - Skills-specific SQLite schema and CRUD.
- Modify: `core/src/lib.rs`
  - Add `pub mod skills;`.
- Modify: `core/src/ffi.rs`
  - Extend `CoreHandle` with `skills: skills::SkillsCore`.
  - Add `tt_skills_*` FFI functions.
- Modify: `core/src/storage/db.rs`
  - Add versioned migration for `skills_*` tables or delegate to `skills::storage`.
- Modify: `core/Cargo.toml`
  - Add SkillSync dependencies.

### Swift Bridge and Models

- Modify: `macos/TokenViewer/Bridge/TokenViewer-Bridging-Header.h`
  - Add `tt_skills_*` declarations.
- Modify: `macos/TokenViewer/Bridge/CoreBridge.swift`
  - Add typed skill bridge calls or split into extension file.
- Create: `macos/TokenViewer/Bridge/CoreBridge+Skills.swift`
  - Swift wrappers around skill FFI calls.
- Create: `macos/TokenViewer/ViewModels/SkillManagerViewModel.swift`
  - Main coordinator for skills, agents, git, and settings.
- Create: `macos/TokenViewer/Models/SkillModels.swift`
  - Codable DTOs matching Rust JSON.

### Swift UI

- Create directory: `macos/TokenViewer/Views/SkillManager/`
- Create: `macos/TokenViewer/Views/SkillManager/SkillManagerView.swift`
- Create: `macos/TokenViewer/Views/SkillManager/SkillListView.swift`
- Create: `macos/TokenViewer/Views/SkillManager/SkillAgentBadgesView.swift`
- Create: `macos/TokenViewer/Views/SkillManager/SkillActionColumn.swift`
- Create: `macos/TokenViewer/Views/SkillManager/AgentSettingsView.swift`
- Create: `macos/TokenViewer/Views/SkillManager/SkillGitSyncView.swift`
- Modify: `macos/TokenViewer/Views/MainWindowView.swift`
  - Add `Skills` tab.
- Modify: `macos/TokenViewer/Services/Localization.swift`
  - Add localized labels and messages for Skill Manager.

### Project and Build

- Modify: `macos/project.yml`
  - Ensure new Swift files are included by source glob.
  - Add linker settings required by `git2` if the build fails.
- Modify: `core/Cargo.toml`
  - Add dependencies:

```toml
git2 = { version = "0.19", features = ["ssh"] }
notify = { version = "6", features = ["macos_kqueue"] }
walkdir = "2"
dirs = "5"
uuid = { version = "1", features = ["v4"] }
tempfile = "3"
```

If `tempfile` is only used by tests, put it under `[dev-dependencies]`.

## Data Model Decision

Use TokenViewer's existing `~/.tokenviewer/data.db` and namespace every Skill Manager table with `skills_`.

Required tables:

```sql
CREATE TABLE IF NOT EXISTS skills_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skills_agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    skills_path TEXT NOT NULL,
    link_strategy TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    is_builtin INTEGER NOT NULL DEFAULT 0,
    icon_name TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS skills_links (
    skill_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    target_path TEXT NOT NULL,
    link_path TEXT NOT NULL,
    link_strategy TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')),
    PRIMARY KEY (skill_id, agent_id)
);

CREATE TABLE IF NOT EXISTS skills_git_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Default source root should remain compatible with SkillSync:

```text
~/.agent/skills
```

If the user has an existing `~/.agents/skills`, expose source root selection in settings and do not silently migrate paths.

---

## Task 1: Baseline Build and Inventory

**Files:**
- Read: `core/Cargo.toml`
- Read: `core/src/ffi.rs`
- Read: `macos/project.yml`
- Read: `/Users/Joy/JoyWorkSpace/skillsync/skills-core/src/*.rs`

- [ ] **Step 1: Confirm TokenViewer builds before migration**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
xcodegen generate --spec macos/project.yml

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
xcodebuild -project TokenViewer.xcodeproj -scheme TokenViewer -configuration Debug \
  -derivedDataPath DerivedData build
```

Expected:

```text
** BUILD SUCCEEDED **
```

- [ ] **Step 2: Run Rust tests before migration**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" cargo test --manifest-path core/Cargo.toml
```

Expected:

```text
test result: ok
```

- [ ] **Step 3: Capture SkillSync source module inventory**

Run:

```bash
find /Users/Joy/JoyWorkSpace/skillsync/skills-core/src -maxdepth 2 -type f | sort
```

Expected source modules:

```text
agent_registry.rs
ffi.rs
git_engine.rs
lib.rs
models.rs
scanner.rs
storage/db.rs
storage/mod.rs
symlink.rs
watcher.rs
```

Stop if baseline build fails. Fix TokenViewer baseline first before migrating.

## Task 2: Add Rust Skills Module Shell

**Files:**
- Create: `core/src/skills/mod.rs`
- Create: `core/src/skills/models.rs`
- Create: `core/src/skills/storage.rs`
- Modify: `core/src/lib.rs`
- Modify: `core/Cargo.toml`

- [ ] **Step 1: Add dependencies**

Modify `core/Cargo.toml`:

```toml
[dependencies]
git2 = { version = "0.19", features = ["ssh"] }
notify = { version = "6", features = ["macos_kqueue"] }
walkdir = "2"
dirs = "5"
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
tempfile = "3"
```

Keep existing dependencies unchanged.

- [ ] **Step 2: Add module exports**

Modify `core/src/lib.rs`:

```rust
pub mod models;
pub mod storage;
pub mod parsers;
pub mod pricing;
pub mod sync;
pub mod skills;
pub mod ffi;
```

- [ ] **Step 3: Create `core/src/skills/mod.rs`**

```rust
pub mod agent_registry;
pub mod git_engine;
pub mod models;
pub mod scanner;
pub mod storage;
pub mod symlink;
pub mod watcher;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::storage::Database;

use self::agent_registry::AgentRegistry;
use self::git_engine::GitEngine;
use self::scanner::Scanner;
use self::symlink::SymlinkManager;
use self::watcher::SkillWatcher;

pub struct SkillsCore {
    pub registry: AgentRegistry,
    pub scanner: Scanner,
    pub symlink: SymlinkManager,
    pub git: Option<GitEngine>,
    pub watcher: Option<SkillWatcher>,
    pub config_dir: PathBuf,
    pub source_root: PathBuf,
    pub known_skill_ids: HashSet<String>,
}

impl SkillsCore {
    pub fn new(db: &Database, source_root: PathBuf) -> Result<Self, String> {
        db.migrate_skills_schema().map_err(|e| e.to_string())?;

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let config_dir = home.join(".agent");
        let registry = AgentRegistry::new(&config_dir).map_err(|e| e.to_string())?;
        let scanner = Scanner::new(source_root.clone());
        let symlink = SymlinkManager::new(source_root.clone());
        let git = GitEngine::open(&source_root).ok();

        let known_skill_ids = scanner
            .scan_all()
            .unwrap_or_default()
            .into_iter()
            .map(|skill| skill.id)
            .collect();

        Ok(Self {
            registry,
            scanner,
            symlink,
            git,
            watcher: None,
            config_dir,
            source_root,
            known_skill_ids,
        })
    }
}
```

- [ ] **Step 4: Create minimal `models.rs`**

Copy SkillSync `skills-core/src/models.rs` into `core/src/skills/models.rs`, then update internal imports from `crate::models` to `crate::skills::models` where needed.

- [ ] **Step 5: Create storage migration entry**

Add this method to `core/src/storage/db.rs`:

```rust
impl Database {
    pub fn migrate_skills_schema(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS skills_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS skills_agents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                skills_path TEXT NOT NULL,
                link_strategy TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                icon_name TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS skills_links (
                skill_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                target_path TEXT NOT NULL,
                link_path TEXT NOT NULL,
                link_strategy TEXT NOT NULL,
                updated_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (skill_id, agent_id)
            );

            CREATE TABLE IF NOT EXISTS skills_git_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );"
        )
    }
}
```

If `Database.conn` is private and this method is inside the same file, no visibility change is needed.

- [ ] **Step 6: Build Rust**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" cargo build --manifest-path core/Cargo.toml --target aarch64-apple-darwin
```

Expected:

```text
Finished
```

## Task 3: Port SkillSync Rust Modules

**Files:**
- Create: `core/src/skills/agent_registry.rs`
- Create: `core/src/skills/scanner.rs`
- Create: `core/src/skills/symlink.rs`
- Create: `core/src/skills/git_engine.rs`
- Create: `core/src/skills/watcher.rs`
- Create: `core/src/skills/storage.rs`

- [ ] **Step 1: Copy implementation files**

Run:

```bash
cp /Users/Joy/JoyWorkSpace/skillsync/skills-core/src/agent_registry.rs core/src/skills/agent_registry.rs
cp /Users/Joy/JoyWorkSpace/skillsync/skills-core/src/scanner.rs core/src/skills/scanner.rs
cp /Users/Joy/JoyWorkSpace/skillsync/skills-core/src/symlink.rs core/src/skills/symlink.rs
cp /Users/Joy/JoyWorkSpace/skillsync/skills-core/src/git_engine.rs core/src/skills/git_engine.rs
cp /Users/Joy/JoyWorkSpace/skillsync/skills-core/src/watcher.rs core/src/skills/watcher.rs
cp /Users/Joy/JoyWorkSpace/skillsync/skills-core/src/storage/db.rs core/src/skills/storage.rs
```

- [ ] **Step 2: Fix module imports**

Replace imports in copied files:

```rust
use crate::models::
```

with:

```rust
use crate::skills::models::
```

Replace:

```rust
crate::agent_registry
crate::scanner
crate::symlink
crate::git_engine
crate::watcher
crate::storage
```

with:

```rust
crate::skills::agent_registry
crate::skills::scanner
crate::skills::symlink
crate::skills::git_engine
crate::skills::watcher
crate::skills::storage
```

- [ ] **Step 3: Remove standalone database ownership from copied storage**

`core/src/skills/storage.rs` should not open `~/.agent/skills.db` directly. Convert its functions into helpers that accept `&crate::storage::Database`, or remove duplicated DB code if all persistence fits into `core/src/storage/db.rs`.

The target pattern is:

```rust
pub fn save_skill_setting(db: &crate::storage::Database, key: &str, value: &str) -> rusqlite::Result<()> {
    db.execute_skills_setting(key, value)
}
```

If direct access to `Connection` is needed, add narrow methods to `Database` rather than making `conn` public.

- [ ] **Step 4: Compile after import fixes**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" cargo check --manifest-path core/Cargo.toml --target aarch64-apple-darwin
```

Expected:

```text
Finished
```

Do not continue until copied modules compile inside TokenViewer.

## Task 4: Add Skills FFI API

**Files:**
- Modify: `core/src/ffi.rs`
- Modify: `macos/TokenViewer/Bridge/TokenViewer-Bridging-Header.h`
- Create: `macos/TokenViewer/Bridge/CoreBridge+Skills.swift`

- [ ] **Step 1: Extend Rust CoreHandle**

Modify `core/src/ffi.rs`:

```rust
pub struct CoreHandle {
    pub db: Database,
    pub db_path: PathBuf,
    pub home_dir: PathBuf,
    pub skills: crate::skills::SkillsCore,
}
```

In `tt_init`, after `Database::open(&path)`:

```rust
let source_root = std::env::var("TOKENVIEWER_SKILLS_ROOT")
    .map(PathBuf::from)
    .unwrap_or_else(|_| home_dir.join(".agent").join("skills"));

let skills = match crate::skills::SkillsCore::new(&db, source_root) {
    Ok(skills) => skills,
    Err(_) => return std::ptr::null_mut(),
};

Box::into_raw(Box::new(CoreHandle { db, db_path: path, home_dir, skills }))
```

- [ ] **Step 2: Add FFI functions**

Add functions with this naming pattern:

```rust
#[no_mangle]
pub extern "C" fn tt_skills_list(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };

    match handle.skills.scanner.scan_all() {
        Ok(skills) => to_json_cstring(&skills),
        Err(e) => to_json_cstring(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[no_mangle]
pub extern "C" fn tt_skills_list_agents(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };

    let agents = handle.skills.registry.all();
    to_json_cstring(&agents)
}
```

Add the remaining operations after compile passes:

```text
tt_skills_organize
tt_skills_restore
tt_skills_delete
tt_skills_link
tt_skills_unlink
tt_skills_add_custom_agent
tt_skills_remove_custom_agent
tt_skills_git_status
tt_skills_git_pull
tt_skills_git_push
tt_skills_git_commit
tt_skills_watch_start
tt_skills_watch_stop
```

Each function must return either a domain JSON object or:

```json
{"ok":false,"error":"message"}
```

- [ ] **Step 3: Add C declarations**

Modify `macos/TokenViewer/Bridge/TokenViewer-Bridging-Header.h`:

```c
char *tt_skills_list(void *handle);
char *tt_skills_list_agents(void *handle);
char *tt_skills_organize(void *handle, const char *json);
char *tt_skills_restore(void *handle, const char *json);
char *tt_skills_delete(void *handle, const char *json);
char *tt_skills_git_status(void *handle);
```

Match pointer types to the existing declarations in the header.

- [ ] **Step 4: Add Swift bridge extension**

Create `macos/TokenViewer/Bridge/CoreBridge+Skills.swift`:

```swift
import Foundation

extension CoreBridge {
    func skillsList() -> Data? {
        call { tt_skills_list($0) }
    }

    func skillsListAgents() -> Data? {
        call { tt_skills_list_agents($0) }
    }

    func skillsGitStatus() -> Data? {
        call { tt_skills_git_status($0) }
    }

    func skillsOrganize(_ payload: Data) -> Data? {
        callWithJSON(payload) { tt_skills_organize($0, $1) }
    }
}
```

If `call` is private, change it to `fileprivate` or keep all skill methods inside `CoreBridge.swift`. Prefer `fileprivate` only if the extension lives in the same file; Swift `fileprivate` does not cross files. For a separate extension file, make a narrow internal helper:

```swift
func callCore(_ body: (OpaquePointer) -> UnsafeMutablePointer<CChar>?) -> Data? {
    call(body)
}
```

- [ ] **Step 5: Build Rust and Swift**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" cargo test --manifest-path core/Cargo.toml

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
xcodebuild -project TokenViewer.xcodeproj -scheme TokenViewer -configuration Debug \
  -derivedDataPath DerivedData build
```

Expected:

```text
test result: ok
** BUILD SUCCEEDED **
```

## Task 5: Add Swift Models and ViewModel

**Files:**
- Create: `macos/TokenViewer/Models/SkillModels.swift`
- Create: `macos/TokenViewer/ViewModels/SkillManagerViewModel.swift`

- [ ] **Step 1: Add Codable models**

Create `macos/TokenViewer/Models/SkillModels.swift`:

```swift
import Foundation

struct SkillEntry: Codable, Identifiable, Hashable {
    let id: String
    let name: String
    let description: String?
    let version: String?
    let path: String
    let sourceAgent: String?
    let linkedAgents: [String]
}

struct SkillAgent: Codable, Identifiable, Hashable {
    let id: String
    let name: String
    let skillsPath: String
    let linkStrategy: String
    let enabled: Bool
    let isBuiltin: Bool
    let exists: Bool?
    let linkedSkills: [String]?
}

struct SkillOperationResult: Codable {
    let ok: Bool
    let error: String?
}
```

Adjust property names with `CodingKeys` if Rust emits snake_case:

```swift
enum CodingKeys: String, CodingKey {
    case skillsPath = "skills_path"
    case linkStrategy = "link_strategy"
    case isBuiltin = "is_builtin"
    case linkedSkills = "linked_skills"
}
```

- [ ] **Step 2: Add main ViewModel**

Create `macos/TokenViewer/ViewModels/SkillManagerViewModel.swift`:

```swift
import Foundation

@MainActor
final class SkillManagerViewModel: ObservableObject {
    static let shared = SkillManagerViewModel()

    @Published private(set) var skills: [SkillEntry] = []
    @Published private(set) var agents: [SkillAgent] = []
    @Published var selectedFilter: String = "all"
    @Published var searchText: String = ""
    @Published var isLoading = false
    @Published var errorMessage: String?

    private let decoder = JSONDecoder()

    private init() {
        decoder.keyDecodingStrategy = .convertFromSnakeCase
    }

    func refresh() {
        isLoading = true
        errorMessage = nil

        Task.detached {
            let skillsData = CoreBridge.shared.skillsList()
            let agentsData = CoreBridge.shared.skillsListAgents()

            await MainActor.run {
                self.isLoading = false
                do {
                    self.skills = try self.decode([SkillEntry].self, from: skillsData)
                    self.agents = try self.decode([SkillAgent].self, from: agentsData)
                } catch {
                    self.errorMessage = error.localizedDescription
                }
            }
        }
    }

    var filteredSkills: [SkillEntry] {
        skills.filter { skill in
            let matchesSearch = searchText.isEmpty
                || skill.name.localizedCaseInsensitiveContains(searchText)
                || (skill.description ?? "").localizedCaseInsensitiveContains(searchText)

            guard selectedFilter != "all" else { return matchesSearch }
            return matchesSearch && skill.linkedAgents.contains(selectedFilter)
        }
    }

    private func decode<T: Decodable>(_ type: T.Type, from data: Data?) throws -> T {
        guard let data else {
            throw SkillManagerError.emptyResponse
        }
        return try decoder.decode(type, from: data)
    }
}

enum SkillManagerError: LocalizedError {
    case emptyResponse

    var errorDescription: String? {
        switch self {
        case .emptyResponse:
            return "Empty response from Skill Manager core"
        }
    }
}
```

Before final UI polish, replace visible error strings with `L10n`.

- [ ] **Step 3: Build**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
xcodebuild -project TokenViewer.xcodeproj -scheme TokenViewer -configuration Debug \
  -derivedDataPath DerivedData build
```

Expected:

```text
** BUILD SUCCEEDED **
```

## Task 6: Add Skill Manager UI

**Files:**
- Create: `macos/TokenViewer/Views/SkillManager/SkillManagerView.swift`
- Create: `macos/TokenViewer/Views/SkillManager/SkillListView.swift`
- Create: `macos/TokenViewer/Views/SkillManager/SkillActionColumn.swift`
- Create: `macos/TokenViewer/Views/SkillManager/SkillAgentBadgesView.swift`
- Modify: `macos/TokenViewer/Views/MainWindowView.swift`
- Modify: `macos/TokenViewer/Services/Localization.swift`

- [ ] **Step 1: Add localization keys**

Add `L10n` properties for:

```text
skills
skillSearchPlaceholder
skillFilter
skillFetch
skillOrganize
skillRestore
skillDelete
skillAgents
skillSync
skillSettings
```

Provide both English and Simplified Chinese values using the existing `Localization.swift` pattern.

- [ ] **Step 2: Create `SkillManagerView`**

```swift
import SwiftUI

struct SkillManagerView: View {
    @StateObject private var viewModel = SkillManagerViewModel.shared
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            Divider()
            SkillListView(viewModel: viewModel)
        }
        .onAppear {
            viewModel.refresh()
        }
    }

    private var toolbar: some View {
        HStack(spacing: 12) {
            TextField(l10n.skillSearchPlaceholder, text: $viewModel.searchText)
                .textFieldStyle(.roundedBorder)

            Picker(l10n.skillFilter, selection: $viewModel.selectedFilter) {
                Text("All").tag("all")
                ForEach(viewModel.agents) { agent in
                    Text(agent.name).tag(agent.id)
                }
            }
            .frame(width: 150)

            Button {
                viewModel.refresh()
            } label: {
                Label(l10n.skillFetch, systemImage: "arrow.clockwise")
            }
        }
        .padding(16)
    }
}
```

Replace `"All"` with a localized string before final build.

- [ ] **Step 3: Create three-column skill row**

`SkillListView` layout:

```swift
import SwiftUI

struct SkillListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                ForEach(viewModel.filteredSkills) { skill in
                    HStack(alignment: .center, spacing: 16) {
                        skillInfo(skill)
                            .frame(maxWidth: .infinity, alignment: .leading)

                        SkillActionColumn(skill: skill, viewModel: viewModel)
                            .frame(width: 140)

                        SkillAgentBadgesView(skill: skill, agents: viewModel.agents)
                            .frame(width: 420, alignment: .leading)
                    }
                    .padding(.horizontal, 24)
                    .padding(.vertical, 14)

                    Divider()
                        .padding(.leading, 24)
                }
            }
        }
    }

    private func skillInfo(_ skill: SkillEntry) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(skill.name)
                .font(.headline)
                .lineLimit(1)
            if let description = skill.description {
                Text(description)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            HStack(spacing: 6) {
                if let version = skill.version {
                    Text(version)
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 8)
                        .padding(.vertical, 2)
                        .background(.quaternary, in: Capsule())
                }
                if let source = skill.sourceAgent {
                    Text(source)
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 8)
                        .padding(.vertical, 2)
                        .background(.quaternary, in: Capsule())
                }
            }
        }
    }
}
```

- [ ] **Step 4: Create action column**

```swift
import SwiftUI

struct SkillActionColumn: View {
    let skill: SkillEntry
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                // Wire in Task 7.
            } label: {
                Label(l10n.skillOrganize, systemImage: "arrow.up.arrow.down")
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)

            Button(role: .destructive) {
                // Wire in Task 7.
            } label: {
                Label(l10n.skillDelete, systemImage: "trash")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
    }
}
```

- [ ] **Step 5: Create agent badges with two-line left alignment**

```swift
import SwiftUI

struct SkillAgentBadgesView: View {
    let skill: SkillEntry
    let agents: [SkillAgent]

    private let columns = [
        GridItem(.adaptive(minimum: 120, maximum: 150), alignment: .leading)
    ]

    var body: some View {
        LazyVGrid(columns: columns, alignment: .leading, spacing: 8) {
            ForEach(agents.prefix(8)) { agent in
                Text(agent.name)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 4)
                    .background(
                        skill.linkedAgents.contains(agent.id)
                        ? Color.green.opacity(0.14)
                        : Color.secondary.opacity(0.08),
                        in: Capsule()
                    )
            }
        }
        .frame(maxHeight: 52, alignment: .topLeading)
        .clipped()
    }
}
```

- [ ] **Step 6: Add main window tab**

Modify `macos/TokenViewer/Views/MainWindowView.swift`:

```swift
SkillManagerView()
    .tag("skills")
    .tabItem { Label(l10n.skills, systemImage: "puzzlepiece.extension.fill") }
```

Place it after Usage and before Limits.

- [ ] **Step 7: Build and open app**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
xcodebuild -project TokenViewer.xcodeproj -scheme TokenViewer -configuration Debug \
  -derivedDataPath DerivedData build

TV_OPEN_MAIN_WINDOW=1 open DerivedData/Build/Products/Debug/TokenViewer.app
```

Expected:

- Main window opens.
- `Skills` tab appears.
- Skills list renders without crashing.
- Agent badges align left and wrap to at most two rows.

## Task 7: Wire Skill Operations

**Files:**
- Modify: `core/src/ffi.rs`
- Modify: `macos/TokenViewer/Bridge/CoreBridge+Skills.swift`
- Modify: `macos/TokenViewer/ViewModels/SkillManagerViewModel.swift`
- Modify: `macos/TokenViewer/Views/SkillManager/SkillActionColumn.swift`

- [ ] **Step 1: Add request DTOs in Rust**

In `core/src/skills/models.rs`:

```rust
#[derive(Debug, serde::Deserialize)]
pub struct SkillIdRequest {
    pub skill_id: String,
}

#[derive(Debug, serde::Serialize)]
pub struct SkillCommandResult {
    pub ok: bool,
    pub error: Option<String>,
}

impl SkillCommandResult {
    pub fn ok() -> Self {
        Self { ok: true, error: None }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self { ok: false, error: Some(message.into()) }
    }
}
```

- [ ] **Step 2: Add organize/delete/restore FFI**

Use SkillSync's existing implementation methods as the behavior source. Keep all destructive operations behind explicit JSON requests.

Pattern:

```rust
#[no_mangle]
pub extern "C" fn tt_skills_delete(handle: *mut CoreHandle, json: *const c_char) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };

    let request: crate::skills::models::SkillIdRequest = match from_cstring_json(json) {
        Ok(request) => request,
        Err(e) => return to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    };

    match handle.skills.delete_skill(&request.skill_id) {
        Ok(()) => to_json_cstring(&crate::skills::models::SkillCommandResult::ok()),
        Err(e) => to_json_cstring(&crate::skills::models::SkillCommandResult::error(e.to_string())),
    }
}
```

Add `SkillsCore::delete_skill`, `organize_skill`, and `restore_skill` by delegating to the copied scanner/symlink logic.

- [ ] **Step 3: Add Swift ViewModel methods**

```swift
func delete(skill: SkillEntry) {
    runSkillCommand(skillID: skill.id, call: CoreBridge.shared.skillsDelete)
}

func organize(skill: SkillEntry) {
    runSkillCommand(skillID: skill.id, call: CoreBridge.shared.skillsOrganize)
}

func restore(skill: SkillEntry) {
    runSkillCommand(skillID: skill.id, call: CoreBridge.shared.skillsRestore)
}

private func runSkillCommand(skillID: String, call: @escaping (Data) -> Data?) {
    Task.detached {
        let payload = try? JSONEncoder().encode(["skill_id": skillID])
        let resultData = payload.flatMap(call)

        await MainActor.run {
            if let resultData,
               let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData),
               result.ok {
                self.refresh()
            } else {
                self.errorMessage = "Skill operation failed"
            }
        }
    }
}
```

Replace `"Skill operation failed"` with `L10n` before final.

- [ ] **Step 4: Add confirmation before delete**

In `SkillActionColumn`, use SwiftUI confirmation dialog:

```swift
@State private var showingDeleteConfirmation = false

Button(role: .destructive) {
    showingDeleteConfirmation = true
} label: {
    Label(l10n.skillDelete, systemImage: "trash")
}
.confirmationDialog(l10n.skillDelete, isPresented: $showingDeleteConfirmation) {
    Button(l10n.skillDelete, role: .destructive) {
        viewModel.delete(skill: skill)
    }
    Button(l10n.cancel, role: .cancel) {}
}
```

- [ ] **Step 5: Test destructive behavior manually with a temp skill**

Create a temp skill under the selected source root:

```bash
mkdir -p "$HOME/.agent/skills/tokenviewer-migration-test"
cat > "$HOME/.agent/skills/tokenviewer-migration-test/SKILL.md" <<'EOF'
---
name: tokenviewer-migration-test
description: Temporary migration test skill.
---

# TokenViewer Migration Test
EOF
```

Use the app:

- Refresh skills.
- Confirm the temp skill appears.
- Delete it.

Expected:

```bash
test ! -e "$HOME/.agent/skills/tokenviewer-migration-test"
```

## Task 8: Add Agent Settings and Custom Agents

**Files:**
- Create: `macos/TokenViewer/Views/SkillManager/AgentSettingsView.swift`
- Modify: `macos/TokenViewer/ViewModels/SkillManagerViewModel.swift`
- Modify: `core/src/ffi.rs`
- Modify: `macos/TokenViewer/Bridge/CoreBridge+Skills.swift`

- [ ] **Step 1: Port custom agent FFI**

From SkillSync `asm_add_custom_agent` and `asm_remove_custom_agent`, implement:

```text
tt_skills_add_custom_agent
tt_skills_remove_custom_agent
tt_skills_update_agent_visibility
```

Use JSON responses with `SkillCommandResult`.

- [ ] **Step 2: Add Swift custom agent form**

Create a compact settings panel:

```swift
struct AgentSettingsView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @State private var name = ""
    @State private var path = ""
    @State private var linkStrategy = "directory"

    var body: some View {
        Form {
            TextField(L10n.shared.agentName, text: $name)
            TextField(L10n.shared.agentSkillsPath, text: $path)
            Picker(L10n.shared.linkStrategy, selection: $linkStrategy) {
                Text("Directory").tag("directory")
                Text("Single File").tag("single_file")
                Text("Overlay").tag("overlay")
            }
            Button(L10n.shared.addAgent) {
                viewModel.addCustomAgent(name: name, path: path, linkStrategy: linkStrategy)
            }
        }
        .padding()
    }
}
```

Localize picker values before final.

- [ ] **Step 3: Verify agent CRUD**

Manual test:

- Add a custom agent pointing to a temp directory.
- Link one skill to it.
- Remove the custom agent.

Expected:

- App refreshes without crash.
- Built-in agents cannot be removed.
- Custom agent config persists across app relaunch.

## Task 9: Add Git Sync Panel

**Files:**
- Create: `macos/TokenViewer/Views/SkillManager/SkillGitSyncView.swift`
- Modify: `core/src/ffi.rs`
- Modify: `macos/TokenViewer/Bridge/CoreBridge+Skills.swift`
- Modify: `macos/TokenViewer/ViewModels/SkillManagerViewModel.swift`

- [ ] **Step 1: Port Git functions**

Use SkillSync `git_engine.rs` behavior to implement:

```text
tt_skills_git_status
tt_skills_git_commit
tt_skills_git_pull
tt_skills_git_push
tt_skills_git_connectivity
```

Keep auth data out of logs and error messages.

- [ ] **Step 2: Add Git status model**

Swift:

```swift
struct SkillGitStatus: Codable {
    let branch: String?
    let ahead: Int
    let behind: Int
    let hasChanges: Bool
    let changes: [SkillGitChange]
}

struct SkillGitChange: Codable, Identifiable {
    var id: String { path }
    let path: String
    let status: String
}
```

- [ ] **Step 3: Add Git sync view**

UI shape:

```swift
struct SkillGitSyncView: View {
    @ObservedObject var viewModel: SkillManagerViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Button {
                    viewModel.refreshGitStatus()
                } label: {
                    Label(L10n.shared.refresh, systemImage: "arrow.clockwise")
                }

                Button {
                    viewModel.pullSkills()
                } label: {
                    Label(L10n.shared.pull, systemImage: "arrow.down.circle")
                }

                Button {
                    viewModel.pushSkills()
                } label: {
                    Label(L10n.shared.push, systemImage: "arrow.up.circle")
                }
            }

            List(viewModel.gitChanges) { change in
                HStack {
                    Text(change.status)
                    Text(change.path)
                }
            }
        }
        .padding()
    }
}
```

- [ ] **Step 4: Verify Git build linkage**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
xcodebuild -project TokenViewer.xcodeproj -scheme TokenViewer -configuration Debug \
  -derivedDataPath DerivedData build
```

If linker fails around OpenSSL, libssh2, libz, or iconv, inspect SkillSync's current successful build settings and add only the needed linker flags to `macos/project.yml`.

## Task 10: Add Watcher and Runtime Controls

**Files:**
- Modify: `core/src/ffi.rs`
- Modify: `macos/TokenViewer/App/TokenViewerApp.swift`
- Modify: `macos/TokenViewer/ViewModels/SkillManagerViewModel.swift`

- [ ] **Step 1: Make watcher opt-in**

Do not start watcher in `applicationDidFinishLaunching`.

Start watcher when `SkillManagerView.onAppear` calls:

```swift
viewModel.startWatching()
```

Stop watcher when the view disappears or app terminates:

```swift
viewModel.stopWatching()
```

- [ ] **Step 2: Add FFI**

```text
tt_skills_watch_start
tt_skills_watch_stop
tt_skills_poll_events
```

Use SkillSync's existing 500ms debounce behavior.

- [ ] **Step 3: Verify idle behavior**

Manual checks:

- Launch TokenViewer and do not open main window.
- Confirm no continuous skill refresh loop runs.
- Open Skills tab.
- Add a new skill file under source root.
- Confirm list refreshes after debounce.

## Task 11: Polish, Tests, and Documentation

**Files:**
- Modify: `README.md`
- Modify: `macos/TokenViewer/Services/Localization.swift`
- Modify: `docs/` website content if TokenViewer website lists features.
- Add or modify Rust tests under `core/src/skills/*`.

- [ ] **Step 1: Add Rust tests**

Prioritize these tests:

```text
scanner parses SKILL.md manifest
agent registry loads built-in agents
symlink manager creates and removes links
delete removes original skill directory
storage migration creates skills_* tables
```

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" cargo test --manifest-path core/Cargo.toml
```

Expected:

```text
test result: ok
```

- [ ] **Step 2: Run full app build**

Run:

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
xcodegen generate --spec macos/project.yml

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
xcodebuild -project TokenViewer.xcodeproj -scheme TokenViewer -configuration Debug \
  -derivedDataPath DerivedData build
```

Expected:

```text
** BUILD SUCCEEDED **
```

- [ ] **Step 3: Update README**

Add a short feature section:

```markdown
### Skill Manager

TokenViewer includes a native Skill Manager for organizing AI coding agent skills across tools. It can scan a canonical skill library, organize existing skills, manage symlinks for supported agents, and sync the skill library with Git.
```

- [ ] **Step 4: Final manual acceptance**

Acceptance checklist:

- Token usage dashboard still works.
- Menu-bar popover still opens quickly.
- Main window has `Usage`, `Skills`, `Limits`, `Settings`, `About`.
- Skills list loads from `~/.agent/skills`.
- Agent badges wrap left-aligned and do not compress skill titles.
- Organize and restore work on a temp skill.
- Delete removes the original skill directory after confirmation.
- Git status works for a Git-backed source root.
- App quits cleanly without watcher crashes.

## Risk Register

### Git/linking risk

`git2` is the highest build risk. If linking fails, keep the scanner/symlink UI working and defer Git Sync behind a disabled UI state until linker settings are fixed.

### Database risk

Do not reuse SkillSync's standalone `~/.agent/skills.db` directly. TokenViewer should own its data under `~/.tokenviewer/data.db`; import later if needed.

### Product risk

TokenViewer must remain useful as a token monitor. Do not start heavyweight skill scanning or watching before the Skills tab is opened.

### Destructive file operation risk

All delete and organize operations must operate on explicit skill IDs and show confirmation before deleting original files.

## Suggested Commit Checkpoints

Only commit when the user has explicitly asked the execution session to commit.

```bash
git add core/Cargo.toml core/src/lib.rs core/src/skills core/src/storage/db.rs
git commit -m "feat: add skills core module"

git add core/src/ffi.rs macos/TokenViewer/Bridge
git commit -m "feat: expose skills manager bridge"

git add macos/TokenViewer/Models macos/TokenViewer/ViewModels macos/TokenViewer/Views/SkillManager macos/TokenViewer/Views/MainWindowView.swift macos/TokenViewer/Services/Localization.swift
git commit -m "feat: add skills manager UI"

git add README.md docs
git commit -m "docs: document skills manager"
```

## Suggested Skills for OpenCode Session

- `superpowers:subagent-driven-development` for task-by-task execution.
- `build-macos-apps:build-run-debug` for Xcode build/run failures.
- `build-macos-apps:swiftui-patterns` for the Skill Manager tab UI.
- `build-macos-apps:test-triage` for Rust or Xcode test failures.
- `diagnose` or `systematic-debugging` if git2/linking or watcher behavior fails.

## Self-Review

- Spec coverage: The plan covers Rust core migration, FFI, Swift bridge, SwiftUI module, agent management, destructive skill operations, Git sync, watcher, tests, docs, and TokenViewer build verification.
- Placeholder scan: No task depends on a future unspecified module; where SkillSync logic is reused, the exact source files and destination files are named.
- Type consistency: Rust FFI uses `tt_skills_*`; Swift bridge methods map to those names; Swift models match expected snake_case conversion.
