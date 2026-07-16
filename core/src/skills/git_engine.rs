use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use git2::{Cred, Oid, RemoteCallbacks, Repository, Signature, Status, StatusOptions};

use crate::skills::models::{GitConnectivity, GitStatusInfo, PendingChange};

/// Write debug log to /tmp/asm-git.log and stderr
pub fn debug_log(msg: &str) {
    eprintln!("[asm] {}", msg);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/asm-git.log")
    {
        let _ = writeln!(f, "[asm] {}", msg);
    }
}

/// Macro version to use with format args
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        $crate::skills::git_engine::debug_log(&msg);
    }};
}

pub struct GitEngine {
    repo: Repository,
}

#[derive(Debug)]
enum RebaseFailure {
    Conflicted(Vec<String>),
    Error(String),
}

#[derive(Debug, Clone, Default)]
pub struct SkillSyncFilter {
    pub include_prefixes: Vec<String>,
    pub include_skill_ids: Vec<String>,
}

impl SkillSyncFilter {
    fn normalized_prefixes(&self) -> Vec<String> {
        self.include_prefixes
            .iter()
            .map(|value| value.trim().trim_end_matches('*').to_string())
            .filter(|value| !value.is_empty() && !value.contains('/'))
            .collect()
    }

    fn normalized_skill_ids(&self) -> Vec<String> {
        self.include_skill_ids
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && !value.contains('/'))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.normalized_prefixes().is_empty() && self.normalized_skill_ids().is_empty()
    }

    fn matches_skill_id(&self, skill_id: &str) -> bool {
        self.normalized_skill_ids()
            .iter()
            .any(|allowed| allowed == skill_id)
            || self
                .normalized_prefixes()
                .iter()
                .any(|prefix| skill_id.starts_with(prefix))
    }

    fn allows_path(&self, path: &str) -> bool {
        let top_level = path.split('/').next().unwrap_or(path);
        !top_level.is_empty() && !top_level.starts_with('.') && self.matches_skill_id(top_level)
    }

    fn included_worktree_skill_ids(&self, repo_path: &Path) -> Result<Vec<String>, String> {
        let mut ids = self.normalized_skill_ids();
        for entry in std::fs::read_dir(repo_path)
            .map_err(|e| format!("Failed to read skill root {}: {}", repo_path.display(), e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read skill root entry: {}", e))?;
            let file_type = entry
                .file_type()
                .map_err(|e| format!("Failed to read skill root entry type: {}", e))?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if self.matches_skill_id(&name) {
                ids.push(name);
            }
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

impl GitEngine {
    /// Open an existing git repository.
    pub fn open(repo_path: &Path) -> Result<Self, String> {
        let repo = Repository::open(repo_path).map_err(|e| {
            format!(
                "Failed to open git repository at {}: {}",
                repo_path.display(),
                e
            )
        })?;
        Ok(Self { repo })
    }

    /// Initialize a new git repository at the given path.
    /// Creates the repo, sets up .gitignore, and makes an initial commit.
    pub fn init(repo_path: &Path) -> Result<Self, String> {
        Self::init_with_identity(repo_path, None, None)
    }

    pub fn init_with_identity(
        repo_path: &Path,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<Self, String> {
        // Create directory if it doesn't exist
        std::fs::create_dir_all(repo_path)
            .map_err(|e| format!("Failed to create directory {}: {}", repo_path.display(), e))?;

        let repo = Repository::init(repo_path)
            .map_err(|e| format!("Failed to init git repo at {}: {}", repo_path.display(), e))?;

        // Configure local user for commits
        let configured_name = user_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("SkillSync");
        let configured_email = user_email
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("skillsync@local");
        let mut config = repo
            .config()
            .map_err(|e| format!("Failed to get repo config: {}", e))?;
        config
            .set_str("user.name", configured_name)
            .map_err(|e| format!("Failed to set user.name: {}", e))?;
        config
            .set_str("user.email", configured_email)
            .map_err(|e| format!("Failed to set user.email: {}", e))?;
        drop(config);

        // Create .gitignore if it doesn't exist
        let gitignore_path = repo_path.join(".gitignore");
        if !gitignore_path.exists() {
            std::fs::write(&gitignore_path, ".DS_Store\n*.swp\n")
                .map_err(|e| format!("Failed to write .gitignore: {}", e))?;
        }

        // Keep the bootstrap commit metadata-only. Full and filtered pushes
        // decide which skill directories enter history.
        let mut index = repo
            .index()
            .map_err(|e| format!("Failed to get index: {}", e))?;
        index
            .add_path(Path::new(".gitignore"))
            .map_err(|e| format!("Failed to stage .gitignore: {}", e))?;
        index
            .write()
            .map_err(|e| format!("Failed to write index: {}", e))?;

        let tree_oid = index
            .write_tree()
            .map_err(|e| format!("Failed to write tree: {}", e))?;
        drop(index);

        let tree = repo
            .find_tree(tree_oid)
            .map_err(|e| format!("Failed to find tree: {}", e))?;
        let sig = repo
            .signature()
            .map_err(|e| format!("Failed to get signature: {}", e))?;

        // Create initial commit (HEAD is unborn in a fresh repo, so no parent)
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .map_err(|e| format!("Failed to create initial commit: {}", e))?;

        drop(tree);
        drop(sig);

        Ok(Self { repo })
    }

    /// Open an existing repo, or initialize a new one if it doesn't exist.
    pub fn open_or_init(repo_path: &Path) -> Result<Self, String> {
        Self::open_or_init_with_identity(repo_path, None, None)
    }

    pub fn open_or_init_with_identity(
        repo_path: &Path,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<Self, String> {
        if repo_path.join(".git").exists() {
            Self::open(repo_path)
        } else {
            Self::init_with_identity(repo_path, user_name, user_email)
        }
    }

    /// Build RemoteCallbacks with PAT token authentication.
    fn make_remote_callbacks(token: &str) -> RemoteCallbacks<'static> {
        let token = token.to_string();
        let mut callbacks = RemoteCallbacks::new();

        let token_clone = token.clone();
        callbacks.credentials(move |_url, username_from_url, _allowed_types| {
            let user = username_from_url
                .map(|u| u.to_string())
                .unwrap_or_else(|| "x-access-token".to_string());
            Cred::userpass_plaintext(&user, &token_clone)
        });

        // Allow self-signed certs (needed for self-hosted GitLab / Other)
        callbacks.certificate_check(|_cert, _host| Ok(git2::CertificateCheckStatus::CertificateOk));

        callbacks
    }

    /// Return the repository's configured default git identity.
    pub fn default_identity(&self) -> (Option<String>, Option<String>) {
        match self.repo.signature() {
            Ok(sig) => (
                sig.name().map(ToString::to_string),
                sig.email().map(ToString::to_string),
            ),
            Err(_) => (None, None),
        }
    }

    fn signature(
        &self,
        name: Option<&str>,
        email: Option<&str>,
    ) -> Result<Signature<'static>, String> {
        let default = self.repo.signature().ok();
        let default_name = default.as_ref().and_then(|sig| sig.name());
        let default_email = default.as_ref().and_then(|sig| sig.email());

        let name = name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(default_name);
        let email = email
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(default_email);

        match (name, email) {
            (Some(name), Some(email)) => Signature::now(name, email)
                .map_err(|e| format!("Failed to create git signature: {}", e)),
            _ => self
                .repo
                .signature()
                .map_err(|e| format!("Failed to get signature: {}", e)),
        }
    }

    /// Get the current git status with branch info, ahead/behind, and pending changes.
    pub fn get_status(&self) -> Result<GitStatusInfo, String> {
        let statuses = self
            .repo
            .statuses(Some(
                StatusOptions::new()
                    .include_untracked(true)
                    .renames_head_to_index(true),
            ))
            .map_err(|e| format!("Failed to get status: {}", e))?;

        // Get branch name
        let branch = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()));

        // Get ahead/behind counts
        let (ahead, behind) = self.ahead_behind().unwrap_or((0, 0));

        // Collect committed-but-unpushed changes first, then let worktree
        // changes override them when the same path appears in both sets.
        let committed_changes = self.get_unpushed_changes().unwrap_or_default();
        let worktree_changes = self.get_pending_changes().unwrap_or_default();
        let changes = Self::merge_changes(committed_changes, worktree_changes);
        let has_changes = !changes.is_empty();
        let index_has_conflicts = self
            .repo
            .index()
            .map(|index| index.has_conflicts())
            .unwrap_or(false);
        let repository_is_clean = self.repo.state() == git2::RepositoryState::Clean;

        if statuses.is_empty()
            && !has_changes
            && ahead == 0
            && repository_is_clean
            && !index_has_conflicts
        {
            let mut info = GitStatusInfo::idle();
            info.branch = branch;
            info.ahead = ahead;
            info.behind = behind;
            return Ok(info);
        }

        let has_conflicts = index_has_conflicts
            || statuses.iter().any(|s| {
                let status = s.status();
                status.contains(Status::CONFLICTED)
            });

        if has_conflicts || !repository_is_clean {
            let conflict_paths = self.conflict_paths().unwrap_or_default();
            let mut info = GitStatusInfo::conflicted(if conflict_paths.is_empty() {
                "A Git operation is incomplete"
            } else {
                "Files have merge conflicts"
            });
            info.branch = branch;
            info.ahead = ahead;
            info.behind = behind;
            info.has_changes = true;
            info.changes = if conflict_paths.is_empty() {
                changes
            } else {
                conflict_paths
                    .into_iter()
                    .map(|file_path| PendingChange {
                        file_path,
                        change_type: "conflicted".to_string(),
                    })
                    .collect()
            };
            return Ok(info);
        }

        let modified_count = changes.len().max(statuses.len());
        let mut info = GitStatusInfo::modified(&format!("{} file(s) modified", modified_count));
        info.branch = branch;
        info.ahead = ahead;
        info.behind = behind;
        info.has_changes = true;
        info.changes = changes;
        Ok(info)
    }

    /// Compute ahead/behind counts vs the upstream tracking branch.
    fn ahead_behind(&self) -> Result<(i32, i32), String> {
        let head = match self.repo.head() {
            Ok(h) => h,
            Err(_) => return Ok((0, 0)),
        };
        let head_oid = match head.target() {
            Some(oid) => oid,
            None => return Ok((0, 0)),
        };

        let upstream_oid = match self.upstream_oid_for_head(&head) {
            Ok(oid) => oid,
            Err(_) => return Ok((0, 0)),
        };

        let (ahead, behind) = self
            .repo
            .graph_ahead_behind(head_oid, upstream_oid)
            .map_err(|e| format!("Failed to compute ahead/behind: {}", e))?;

        Ok((ahead as i32, behind as i32))
    }

    /// Resolve the configured upstream, falling back to origin/<branch> for
    /// repositories initialized by TokenViewer before tracking was configured.
    fn upstream_oid_for_head(&self, head: &git2::Reference<'_>) -> Result<Oid, String> {
        let head_ref = head.name().ok_or("HEAD is not on a branch")?;
        if let Ok(upstream_name) = self.repo.branch_upstream_name(head_ref) {
            let upstream_ref = self
                .repo
                .find_reference(upstream_name.as_str().unwrap_or(""))
                .map_err(|e| format!("Failed to find upstream branch: {}", e))?;
            return upstream_ref
                .target()
                .ok_or_else(|| "Upstream branch has no target".to_string());
        }

        let branch = head.shorthand().ok_or("HEAD is not on a branch")?;
        let fallback_ref = format!("refs/remotes/origin/{branch}");
        self.repo
            .find_reference(&fallback_ref)
            .map_err(|e| format!("Failed to find fallback upstream {}: {}", fallback_ref, e))?
            .target()
            .ok_or_else(|| format!("Fallback upstream {} has no target", fallback_ref))
    }

    /// Get a list of pending changes (modified, added, deleted files).
    pub fn get_pending_changes(&self) -> Result<Vec<PendingChange>, String> {
        let statuses = self
            .repo
            .statuses(Some(
                StatusOptions::new()
                    .include_untracked(true)
                    .renames_head_to_index(true),
            ))
            .map_err(|e| format!("Failed to get status: {}", e))?;

        let changes: Vec<PendingChange> = statuses
            .iter()
            .filter_map(|s| {
                let path = s.path()?.to_string();
                let status = s.status();

                let change_type =
                    if status.contains(Status::INDEX_NEW) || status.contains(Status::WT_NEW) {
                        "added"
                    } else if status.contains(Status::INDEX_DELETED)
                        || status.contains(Status::WT_DELETED)
                    {
                        "deleted"
                    } else {
                        "modified"
                    };

                Some(PendingChange {
                    file_path: path,
                    change_type: change_type.to_string(),
                })
            })
            .collect();

        Ok(changes)
    }

    /// Get changes committed locally after the upstream tracking branch.
    fn get_unpushed_changes(&self) -> Result<Vec<PendingChange>, String> {
        let head = self
            .repo
            .head()
            .map_err(|e| format!("Failed to get HEAD: {}", e))?;
        let head_oid = head.target().ok_or("HEAD has no target")?;
        let upstream_oid = self.upstream_oid_for_head(&head)?;

        let merge_base_oid = self
            .repo
            .merge_base(upstream_oid, head_oid)
            .map_err(|e| format!("Failed to find upstream merge base: {}", e))?;
        let merge_base_tree = self
            .repo
            .find_commit(merge_base_oid)
            .and_then(|commit| commit.tree())
            .map_err(|e| format!("Failed to read merge-base tree: {}", e))?;
        let head_tree = self
            .repo
            .find_commit(head_oid)
            .and_then(|commit| commit.tree())
            .map_err(|e| format!("Failed to read HEAD tree: {}", e))?;
        let diff = self
            .repo
            .diff_tree_to_tree(Some(&merge_base_tree), Some(&head_tree), None)
            .map_err(|e| format!("Failed to diff merge base and HEAD: {}", e))?;

        let changes = diff
            .deltas()
            .filter_map(|delta| {
                let (path, change_type) = match delta.status() {
                    git2::Delta::Added | git2::Delta::Untracked => {
                        (delta.new_file().path()?, "added")
                    }
                    git2::Delta::Deleted => (delta.old_file().path()?, "deleted"),
                    _ => (
                        delta
                            .new_file()
                            .path()
                            .or_else(|| delta.old_file().path())?,
                        "modified",
                    ),
                };
                Some(PendingChange {
                    file_path: path.to_string_lossy().to_string(),
                    change_type: change_type.to_string(),
                })
            })
            .collect();

        Ok(changes)
    }

    fn merge_changes(
        committed: Vec<PendingChange>,
        worktree: Vec<PendingChange>,
    ) -> Vec<PendingChange> {
        let mut changes = BTreeMap::new();
        for change in committed.into_iter().chain(worktree) {
            changes.insert(change.file_path.clone(), change);
        }
        changes.into_values().collect()
    }

    /// Set (or create) the "origin" remote URL.
    pub fn set_remote_url(&self, url: &str) -> Result<(), String> {
        if self.repo.find_remote("origin").is_ok() {
            self.repo
                .remote_set_url("origin", url)
                .map_err(|e| format!("Failed to set remote URL: {}", e))?;
        } else {
            self.repo
                .remote("origin", url)
                .map_err(|e| format!("Failed to create remote 'origin': {}", e))?;
        }
        Ok(())
    }

    /// Detect the current branch name.
    fn current_branch(&self) -> Result<String, String> {
        let head = self
            .repo
            .head()
            .map_err(|e| format!("Failed to get HEAD: {}", e))?;
        let name = head.shorthand().ok_or("HEAD is not on a branch")?;
        debug_log!(" current_branch: {}", name);
        Ok(name.to_string())
    }

    /// Fetch from origin using the given token for auth (or no auth if None).
    fn fetch_origin(&self, branch: &str, token: Option<&str>) -> Result<(), String> {
        debug_log!(
            " fetch_origin: branch={}, has_token={}",
            branch,
            token.is_some()
        );
        let mut remote = self
            .repo
            .find_remote("origin")
            .map_err(|e| format!("Failed to find remote 'origin': {}", e))?;

        let mut fetch_options = git2::FetchOptions::new();
        if let Some(tok) = token {
            fetch_options.remote_callbacks(Self::make_remote_callbacks(tok));
        }

        remote
            .fetch(&[branch], Some(&mut fetch_options), None)
            .map_err(|e| format!("Failed to fetch from origin: {}", e))?;
        drop(remote);

        if let Err(error) = self.ensure_upstream_tracking(branch) {
            debug_log!(" fetch_origin: unable to configure upstream: {}", error);
        }

        debug_log!(" fetch_origin: completed");
        Ok(())
    }

    fn ensure_upstream_tracking(&self, branch: &str) -> Result<(), String> {
        let local_ref = format!("refs/heads/{branch}");
        if self.repo.branch_upstream_name(&local_ref).is_ok() {
            return Ok(());
        }

        let remote_ref = format!("refs/remotes/origin/{branch}");
        self.repo
            .find_reference(&remote_ref)
            .map_err(|e| format!("Remote branch {} is unavailable: {}", remote_ref, e))?;

        let mut local_branch = self
            .repo
            .find_branch(branch, git2::BranchType::Local)
            .map_err(|e| format!("Failed to find local branch {}: {}", branch, e))?;
        local_branch
            .set_upstream(Some(&format!("origin/{branch}")))
            .map_err(|e| format!("Failed to track origin/{}: {}", branch, e))?;
        debug_log!(
            " fetch_origin: configured {} to track origin/{}",
            branch,
            branch
        );
        Ok(())
    }

    /// Auto-commit any pending changes before sync operations.
    /// Handles both initial commit (unborn HEAD) and subsequent commits.
    fn auto_commit(
        &mut self,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<(), String> {
        self.auto_commit_matching(None, user_name, user_email)
    }

    fn auto_commit_filtered(
        &mut self,
        filter: &SkillSyncFilter,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<(), String> {
        self.auto_commit_matching(Some(filter), user_name, user_email)
    }

    fn auto_commit_matching(
        &mut self,
        filter: Option<&SkillSyncFilter>,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<(), String> {
        debug_log!(" auto_commit: checking pending changes");
        let changes = self.get_pending_changes()?;
        let changes: Vec<PendingChange> = match filter {
            Some(filter) => changes
                .into_iter()
                .filter(|change| filter.allows_path(&change.file_path))
                .collect(),
            None => changes,
        };

        let mut index = self
            .repo
            .index()
            .map_err(|e| format!("Failed to get index: {}", e))?;

        if changes.is_empty() {
            debug_log!(" auto_commit: no pending changes, skipping");
            return Ok(());
        }

        debug_log!(" auto_commit: {} pending changes", changes.len());
        for c in &changes {
            debug_log!(" auto_commit:   {} - {}", c.change_type, c.file_path);
        }

        let file_count = changes.len();

        if let Some(filter) = filter {
            let mut pathspecs: Vec<String> = changes
                .iter()
                .filter_map(|change| change.file_path.split('/').next())
                .filter(|skill_id| filter.matches_skill_id(skill_id))
                .map(|skill_id| format!("{skill_id}/**"))
                .collect();
            pathspecs.sort();
            pathspecs.dedup();
            if !pathspecs.is_empty() {
                let pathspec_refs: Vec<&str> = pathspecs.iter().map(String::as_str).collect();
                index
                    .add_all(pathspec_refs, git2::IndexAddOption::DEFAULT, None)
                    .map_err(|e| format!("Failed to stage filtered files: {}", e))?;
            }
        } else {
            index
                .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                .map_err(|e| format!("Failed to stage files: {}", e))?;
        }

        index
            .write()
            .map_err(|e| format!("Failed to write index: {}", e))?;

        let tree_oid = index
            .write_tree()
            .map_err(|e| format!("Failed to write tree: {}", e))?;
        drop(index);

        let tree = self
            .repo
            .find_tree(tree_oid)
            .map_err(|e| format!("Failed to find tree: {}", e))?;

        let sig = self.signature(user_name, user_email)?;

        let message = format!(
            "Auto-commit before sync ({} file{})",
            file_count,
            if file_count == 1 { "" } else { "s" }
        );

        match self.repo.head() {
            Ok(head) => {
                let parent_oid = head.target().ok_or("HEAD has no target")?;
                drop(head);
                let parent = self
                    .repo
                    .find_commit(parent_oid)
                    .map_err(|e| format!("Failed to find parent commit: {}", e))?;
                self.repo
                    .commit(Some("HEAD"), &sig, &sig, &message, &tree, &[&parent])
                    .map_err(|e| format!("Failed to auto-commit: {}", e))?;
                debug_log!(" auto_commit: committed (parent) \"{}\"", message);
            }
            Err(_) => {
                // Unborn HEAD: initial commit with no parents
                self.repo
                    .commit(
                        Some("HEAD"),
                        &sig,
                        &sig,
                        &message,
                        &tree,
                        &[] as &[&git2::Commit],
                    )
                    .map_err(|e| format!("Failed to create initial auto-commit: {}", e))?;
                debug_log!(" auto_commit: committed (initial) \"{}\"", message);
            }
        }

        Ok(())
    }

    fn remove_matching_tracked_entries(
        index: &mut git2::Index,
        filter: &SkillSyncFilter,
    ) -> Result<usize, String> {
        let paths: Vec<String> = index
            .iter()
            .filter_map(|entry| String::from_utf8(entry.path.to_vec()).ok())
            .filter(|path| {
                let Some((top_level, _rest)) = path.split_once('/') else {
                    return false;
                };
                !top_level.starts_with('.') && filter.matches_skill_id(top_level)
            })
            .collect();

        for path in &paths {
            index
                .remove_path(Path::new(path))
                .map_err(|e| format!("Failed to replace filtered path {}: {}", path, e))?;
        }

        Ok(paths.len())
    }

    fn build_filtered_candidate_tree(
        &self,
        filter: &SkillSyncFilter,
        base_tree: &git2::Tree<'_>,
    ) -> Result<(Oid, Vec<String>), String> {
        let workdir = self
            .repo
            .workdir()
            .ok_or("Git repository has no working directory")?;
        let mut index = self
            .repo
            .index()
            .map_err(|e| format!("Failed to get index: {}", e))?;
        index
            .read_tree(base_tree)
            .map_err(|e| format!("Failed to load sync base tree: {}", e))?;
        Self::remove_matching_tracked_entries(&mut index, filter)?;

        let skill_ids = filter.included_worktree_skill_ids(workdir)?;
        for skill_id in &skill_ids {
            let pathspec = format!("{skill_id}/**");
            index
                .add_all([pathspec.as_str()], git2::IndexAddOption::DEFAULT, None)
                .map_err(|e| format!("Failed to stage filtered skill {}: {}", skill_id, e))?;
        }

        let tree_oid = index
            .write_tree()
            .map_err(|e| format!("Failed to write filtered candidate tree: {}", e))?;
        Ok((tree_oid, skill_ids))
    }

    fn commit_filtered_snapshot(
        &mut self,
        filter: &SkillSyncFilter,
        base_oid: Oid,
        parent_oid: Option<Oid>,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<Option<Oid>, RebaseFailure> {
        let base_commit = self
            .repo
            .find_commit(base_oid)
            .map_err(|e| RebaseFailure::Error(format!("Failed to find sync base: {}", e)))?;
        let base_tree = base_commit
            .tree()
            .map_err(|e| RebaseFailure::Error(format!("Failed to read sync base tree: {}", e)))?;
        let (candidate_oid, skill_ids) = self
            .build_filtered_candidate_tree(filter, &base_tree)
            .map_err(RebaseFailure::Error)?;
        let candidate_tree = self
            .repo
            .find_tree(candidate_oid)
            .map_err(|e| RebaseFailure::Error(format!("Failed to read candidate tree: {}", e)))?;

        let parent = parent_oid
            .map(|oid| self.repo.find_commit(oid))
            .transpose()
            .map_err(|e| RebaseFailure::Error(format!("Failed to find remote parent: {}", e)))?;
        let final_tree_oid = if let Some(parent) = parent.as_ref() {
            let remote_tree = parent
                .tree()
                .map_err(|e| RebaseFailure::Error(format!("Failed to read remote tree: {}", e)))?;
            let mut merge_index = self
                .repo
                .merge_trees(&base_tree, &candidate_tree, &remote_tree, None)
                .map_err(|e| {
                    RebaseFailure::Error(format!("Failed to merge filtered tree: {}", e))
                })?;
            if merge_index.has_conflicts() {
                let paths = Self::conflict_paths_from_index(&merge_index).unwrap_or_default();
                return Err(RebaseFailure::Conflicted(paths));
            }
            merge_index.write_tree_to(&self.repo).map_err(|e| {
                RebaseFailure::Error(format!("Failed to write merged filtered tree: {}", e))
            })?
        } else {
            candidate_oid
        };

        let final_tree = self
            .repo
            .find_tree(final_tree_oid)
            .map_err(|e| RebaseFailure::Error(format!("Failed to read filtered tree: {}", e)))?;
        if parent
            .as_ref()
            .and_then(|commit| commit.tree().ok())
            .is_some_and(|tree| tree.id() == final_tree.id())
        {
            return Ok(None);
        }

        let sig = self
            .signature(user_name, user_email)
            .map_err(RebaseFailure::Error)?;
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let message = format!(
            "Auto-commit filtered sync ({} skill{})",
            skill_ids.len(),
            if skill_ids.len() == 1 { "" } else { "s" }
        );
        let commit_oid = self
            .repo
            .commit(None, &sig, &sig, &message, &final_tree, &parents)
            .map_err(|e| {
                RebaseFailure::Error(format!("Failed to create filtered sync commit: {}", e))
            })?;
        debug_log!(" commit_filtered_snapshot: committed \"{}\"", message);
        debug_log!(" commit_filtered_snapshot: oid={}", commit_oid);
        Ok(Some(commit_oid))
    }

    fn update_current_branch(&self, oid: Oid) -> Result<(), String> {
        let branch = self.current_branch()?;
        let ref_name = format!("refs/heads/{branch}");
        if let Ok(mut reference) = self.repo.find_reference(&ref_name) {
            reference
                .set_target(oid, "filtered sync")
                .map_err(|e| format!("Failed to update branch {}: {}", branch, e))?;
        } else {
            self.repo
                .reference(&ref_name, oid, true, "filtered sync")
                .map_err(|e| format!("Failed to create branch {}: {}", branch, e))?;
        }
        Ok(())
    }

    fn adopt_filtered_sync_commit(&self, oid: Oid) -> Result<(), String> {
        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|e| format!("Failed to find filtered sync commit: {}", e))?;
        let tree = commit
            .tree()
            .map_err(|e| format!("Failed to read filtered sync tree: {}", e))?;
        self.update_current_branch(oid)?;
        let mut index = self
            .repo
            .index()
            .map_err(|e| format!("Failed to get index after filtered sync: {}", e))?;
        index
            .read_tree(&tree)
            .map_err(|e| format!("Failed to update index after filtered sync: {}", e))?;
        index
            .write()
            .map_err(|e| format!("Failed to persist index after filtered sync: {}", e))?;
        Ok(())
    }

    fn push_filtered_commit(&self, oid: Oid, token: Option<&str>) -> Result<(), String> {
        let branch = self.current_branch()?;
        let temporary_ref = "refs/tokenviewer/filtered-sync";
        self.repo
            .reference(temporary_ref, oid, true, "filtered sync candidate")
            .map_err(|e| format!("Failed to create filtered sync reference: {}", e))?;

        let result = (|| {
            let mut remote = self
                .repo
                .find_remote("origin")
                .map_err(|e| format!("Failed to find remote 'origin': {}", e))?;
            let mut push_options = git2::PushOptions::new();
            if let Some(token) = token {
                push_options.remote_callbacks(Self::make_remote_callbacks(token));
            }
            let refspec = format!("{temporary_ref}:refs/heads/{branch}");
            remote
                .push(&[&refspec], Some(&mut push_options))
                .map_err(|e| format!("Failed to push filtered sync: {}", e))?;
            self.repo
                .reference(
                    &format!("refs/remotes/origin/{branch}"),
                    oid,
                    true,
                    "filtered sync pushed",
                )
                .map_err(|e| format!("Failed to update remote tracking branch: {}", e))?;
            Ok(())
        })();

        if let Ok(mut reference) = self.repo.find_reference(temporary_ref) {
            let _ = reference.delete();
        }
        result
    }

    fn sync_blocked_status(&self) -> Result<Option<GitStatusInfo>, String> {
        let index_has_conflicts = self
            .repo
            .index()
            .map_err(|e| format!("Failed to inspect index: {}", e))?
            .has_conflicts();
        if self.repo.state() == git2::RepositoryState::Clean && !index_has_conflicts {
            return Ok(None);
        }
        self.get_status().map(Some)
    }

    fn conflict_paths(&self) -> Result<Vec<String>, String> {
        let index = self
            .repo
            .index()
            .map_err(|e| format!("Failed to inspect conflicts: {}", e))?;
        Self::conflict_paths_from_index(&index)
    }

    fn conflict_paths_from_index(index: &git2::Index) -> Result<Vec<String>, String> {
        let conflicts = index
            .conflicts()
            .map_err(|e| format!("Failed to enumerate conflicts: {}", e))?;
        let mut paths = Vec::new();
        for conflict in conflicts {
            let conflict = conflict.map_err(|e| format!("Failed to read conflict: {}", e))?;
            if let Some(entry) = conflict.our.or(conflict.their).or(conflict.ancestor) {
                paths.push(String::from_utf8_lossy(&entry.path).to_string());
            }
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    fn status_for_rebase_failure(&self, failure: RebaseFailure) -> GitStatusInfo {
        match failure {
            RebaseFailure::Conflicted(paths) => {
                let mut status = GitStatusInfo::conflicted(if paths.is_empty() {
                    "Git changes conflict with the remote repository"
                } else {
                    "Local and remote changes conflict"
                });
                status.changes = paths
                    .into_iter()
                    .map(|file_path| PendingChange {
                        file_path,
                        change_type: "conflicted".to_string(),
                    })
                    .collect();
                status
            }
            RebaseFailure::Error(message) => GitStatusInfo::error(&message),
        }
    }

    fn rebase_failure(repo: &Repository, context: &str, error: git2::Error) -> RebaseFailure {
        let is_conflict = matches!(
            error.code(),
            git2::ErrorCode::Conflict | git2::ErrorCode::MergeConflict | git2::ErrorCode::Unmerged
        ) || repo
            .index()
            .map(|index| index.has_conflicts())
            .unwrap_or(false);
        if is_conflict {
            let paths = repo
                .index()
                .ok()
                .and_then(|index| Self::conflict_paths_from_index(&index).ok())
                .unwrap_or_default();
            RebaseFailure::Conflicted(paths)
        } else {
            RebaseFailure::Error(format!("{}: {}", context, error))
        }
    }

    /// Pull (fetch + rebase) from origin. Auto-commits pending changes first.
    pub fn pull(
        &mut self,
        token: Option<&str>,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<GitStatusInfo, String> {
        debug_log!(" pull: start");
        if let Some(status) = self.sync_blocked_status()? {
            return Ok(status);
        }
        self.auto_commit(user_name, user_email)?;
        if let Err(failure) = self.pull_rebase(token, user_name, user_email) {
            return Ok(self.status_for_rebase_failure(failure));
        }

        debug_log!(" pull: done");
        self.get_status()
    }

    /// Auto-commit pending changes, pull-rebase, and push.
    pub fn stage_and_push(
        &mut self,
        _message: &str,
        token: Option<&str>,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<GitStatusInfo, String> {
        debug_log!(" stage_and_push: start");
        if let Some(status) = self.sync_blocked_status()? {
            return Ok(status);
        }
        self.auto_commit(user_name, user_email)?;

        if self.has_remote() {
            debug_log!(" stage_and_push: has remote, starting pull_rebase");
            if let Err(failure) = self.pull_rebase(token, user_name, user_email) {
                return Ok(self.status_for_rebase_failure(failure));
            }
            debug_log!(" stage_and_push: pull_rebase ok, starting push");
            self.push(token)?;
            debug_log!(" stage_and_push: push ok");
        } else {
            debug_log!(" stage_and_push: no remote configured");
        }

        self.get_status()
    }

    /// Auto-commit allowed pending skill changes, pull-rebase, and push.
    pub fn stage_and_push_filtered(
        &mut self,
        _message: &str,
        filter: &SkillSyncFilter,
        token: Option<&str>,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<GitStatusInfo, String> {
        debug_log!(" stage_and_push_filtered: start");
        if let Some(status) = self.sync_blocked_status()? {
            return Ok(status);
        }

        if self.has_remote() {
            let head_oid = self.repo.head().ok().and_then(|head| head.target());
            let previous_remote = self
                .repo
                .head()
                .ok()
                .and_then(|head| self.upstream_oid_for_head(&head).ok());
            let remote_parent = self.fetch_remote_head(token)?;
            let base_oid = match (previous_remote, head_oid, remote_parent) {
                (Some(previous), _, Some(remote)) => {
                    self.repo.merge_base(previous, remote).unwrap_or(previous)
                }
                (Some(previous), _, None) => previous,
                (None, Some(head), _) => head,
                (None, None, _) => return Err("Filtered sync requires an initial commit".into()),
            };
            match self.commit_filtered_snapshot(
                filter,
                base_oid,
                remote_parent,
                user_name,
                user_email,
            ) {
                Ok(Some(commit_oid)) => {
                    self.push_filtered_commit(commit_oid, token)?;
                    self.adopt_filtered_sync_commit(commit_oid)?;
                }
                Ok(None) => {
                    if let Some(remote_oid) = remote_parent {
                        self.adopt_filtered_sync_commit(remote_oid)?;
                    }
                }
                Err(failure) => return Ok(self.status_for_rebase_failure(failure)),
            }
        } else {
            debug_log!(" stage_and_push_filtered: no remote configured");
            self.auto_commit_filtered(filter, user_name, user_email)?;
        }

        self.get_status()
    }

    fn fetch_remote_head(&self, token: Option<&str>) -> Result<Option<Oid>, String> {
        let branch = match self.current_branch() {
            Ok(branch) => branch,
            Err(_) => return Ok(None),
        };
        self.fetch_origin(&branch, token)?;
        let fetch_head_path = self.repo.path().join("FETCH_HEAD");
        let is_empty = !fetch_head_path.exists()
            || std::fs::metadata(&fetch_head_path)
                .map(|m| m.len() == 0)
                .unwrap_or(true);
        if is_empty {
            return Ok(None);
        }
        let fetch_head = self
            .repo
            .find_reference("FETCH_HEAD")
            .map_err(|e| format!("Failed to find FETCH_HEAD: {}", e))?;
        Ok(fetch_head.target())
    }

    /// Check if the repository has a remote configured.
    fn has_remote(&self) -> bool {
        self.repo.find_remote("origin").is_ok()
    }

    /// Execute git pull --rebase (fetch + rebase).
    fn pull_rebase(
        &mut self,
        token: Option<&str>,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<(), RebaseFailure> {
        debug_log!(" pull_rebase: start");

        let branch = match self.current_branch() {
            Ok(b) => b,
            Err(_) => {
                debug_log!(" pull_rebase: HEAD is unborn, skipping");
                return Ok(());
            }
        };

        // Clean up stale empty FETCH_HEAD from previous failed attempts
        let fetch_head_path = self.repo.path().join("FETCH_HEAD");
        if fetch_head_path.exists() {
            let len = std::fs::metadata(&fetch_head_path)
                .map(|m| m.len())
                .unwrap_or(0);
            debug_log!(" pull_rebase: existing FETCH_HEAD size={}", len);
            if len == 0 {
                debug_log!(" pull_rebase: removing stale empty FETCH_HEAD");
                let _ = std::fs::remove_file(&fetch_head_path);
            }
        } else {
            debug_log!(" pull_rebase: no existing FETCH_HEAD");
        }

        // Fetch from origin
        self.fetch_origin(&branch, token)
            .map_err(RebaseFailure::Error)?;

        // Check if FETCH_HEAD has content (remote may be empty)
        let is_empty = !fetch_head_path.exists()
            || std::fs::metadata(&fetch_head_path)
                .map(|m| m.len() == 0)
                .unwrap_or(true);

        debug_log!(
            " pull_rebase: FETCH_HEAD after fetch: exists={}, is_empty={}",
            fetch_head_path.exists(),
            is_empty
        );

        if is_empty {
            debug_log!(" pull_rebase: nothing fetched, skipping rebase");
            return Ok(());
        }

        // Get the FETCH_HEAD
        debug_log!(" pull_rebase: finding FETCH_HEAD reference");
        let fetch_head = self
            .repo
            .find_reference("FETCH_HEAD")
            .map_err(|e| RebaseFailure::Error(format!("Failed to find FETCH_HEAD: {}", e)))?;
        let upstream = self
            .repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(|e| RebaseFailure::Error(format!("Failed to resolve FETCH_HEAD: {}", e)))?;

        // Drop fetch_head before rebase
        drop(fetch_head);

        // Rebase onto fetched commit
        let mut rebase = self
            .repo
            .rebase(
                None,
                Some(&upstream),
                None,
                Some(&mut git2::RebaseOptions::new()),
            )
            .map_err(|e| RebaseFailure::Error(format!("Failed to start rebase: {}", e)))?;

        let sig = self
            .signature(user_name, user_email)
            .map_err(RebaseFailure::Error)?;

        // Iterate rebase steps
        while let Some(op) = rebase.next() {
            if let Err(error) = op {
                let failure = Self::rebase_failure(&self.repo, "Rebase step failed", error);
                let _ = rebase.abort();
                return Err(failure);
            }
            if self
                .repo
                .index()
                .map(|index| index.has_conflicts())
                .unwrap_or(false)
            {
                let paths = self.conflict_paths().unwrap_or_default();
                let _ = rebase.abort();
                return Err(RebaseFailure::Conflicted(paths));
            }
            if let Err(error) = rebase.commit(None, &sig, None) {
                if error.code() != git2::ErrorCode::Applied {
                    let failure = Self::rebase_failure(&self.repo, "Rebase commit failed", error);
                    let _ = rebase.abort();
                    return Err(failure);
                }
            }
        }

        if let Err(error) = rebase.finish(None) {
            let _ = rebase.abort();
            return Err(RebaseFailure::Error(format!(
                "Failed to finish rebase: {}",
                error
            )));
        }

        Ok(())
    }

    /// Check connectivity to the remote repository using the provided token.
    /// Connects to the remote and immediately disconnects — no data transfer.
    pub fn check_connectivity(&self, token: Option<&str>) -> Result<GitConnectivity, String> {
        let token = match token {
            Some(t) if !t.is_empty() => t,
            _ => {
                return Ok(GitConnectivity {
                    status: "disconnected".into(),
                    message: Some("No token configured".into()),
                })
            }
        };

        // Check if remote exists
        if self.repo.find_remote("origin").is_err() {
            return Ok(GitConnectivity {
                status: "disconnected".into(),
                message: Some("No remote 'origin' configured".into()),
            });
        }

        let mut remote = self
            .repo
            .find_remote("origin")
            .map_err(|e| format!("Failed to find remote 'origin': {}", e))?;

        // Connect only — no data transfer, just auth handshake
        // Use a block to scope the RemoteConnection's lifetime
        let result = {
            remote
                .connect_auth(
                    git2::Direction::Fetch,
                    Some(Self::make_remote_callbacks(token)),
                    None,
                )
                .map(|_conn| ())
                .map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => Ok(GitConnectivity {
                status: "connected".into(),
                message: None,
            }),
            Err(e) => {
                let msg = if e.contains("403") {
                    "Access denied (403). Check that your token has 'repo' scope and is not expired.".into()
                } else if e.contains("401") {
                    "Authentication failed (401). The token may be invalid or revoked.".into()
                } else if e.contains("404") {
                    "Repository not found (404). Check the repository URL.".into()
                } else {
                    format!("Connection failed: {}", e)
                };
                Ok(GitConnectivity {
                    status: "disconnected".into(),
                    message: Some(msg),
                })
            }
        }
    }

    /// Push to origin.
    fn push(&self, token: Option<&str>) -> Result<(), String> {
        debug_log!(" push: start, has_token={}", token.is_some());
        let mut remote = self
            .repo
            .find_remote("origin")
            .map_err(|e| format!("Failed to find remote 'origin': {}", e))?;

        let branch = self.current_branch().unwrap_or_else(|_| "main".to_string());
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        debug_log!(" push: branch={}, refspec={}", branch, refspec);

        let mut push_options = git2::PushOptions::new();
        if let Some(tok) = token {
            push_options.remote_callbacks(Self::make_remote_callbacks(tok));
        }

        remote
            .push(&[&refspec], Some(&mut push_options))
            .map_err(|e| format!("Failed to push: {}", e))?;

        debug_log!(" push: completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo(path: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("Failed to git init");

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .unwrap();

        fs::write(path.join("README.md"), "# Test\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(path)
            .output()
            .unwrap();
    }

    fn configure_tracking_branch(path: &Path) -> String {
        let branch = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(path)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&branch.stdout).trim().to_string();
        Command::new("git")
            .args(["remote", "add", "origin", "."])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                "HEAD",
            ])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", &format!("branch.{branch}.remote"), "origin"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "config",
                &format!("branch.{branch}.merge"),
                &format!("refs/heads/{branch}"),
            ])
            .current_dir(path)
            .output()
            .unwrap();
        branch
    }

    fn remove_tracking_config(path: &Path, branch: &str) {
        Command::new("git")
            .args(["config", "--unset", &format!("branch.{branch}.remote")])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "--unset", &format!("branch.{branch}.merge")])
            .current_dir(path)
            .output()
            .unwrap();
    }

    #[test]
    fn test_open_existing_repo() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        let engine = GitEngine::open(dir.path());
        assert!(engine.is_ok());
    }

    #[test]
    fn test_get_idle_status() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        let engine = GitEngine::open(dir.path()).unwrap();
        let status = engine.get_status().unwrap();
        assert_eq!(status.status, "idle");
    }

    #[test]
    fn test_get_modified_status() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::write(dir.path().join("README.md"), "# Modified\n").unwrap();
        let engine = GitEngine::open(dir.path()).unwrap();
        let status = engine.get_status().unwrap();
        assert_eq!(status.status, "modified");
    }

    #[test]
    fn test_get_pending_changes() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::write(dir.path().join("README.md"), "# Modified\n").unwrap();
        fs::write(dir.path().join("new-file.txt"), "new\n").unwrap();
        let engine = GitEngine::open(dir.path()).unwrap();
        let changes = engine.get_pending_changes().unwrap();
        assert!(!changes.is_empty());
        assert!(changes.iter().any(|c| c.file_path == "README.md"));
    }

    #[test]
    fn test_get_status_includes_committed_unpushed_skill() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        configure_tracking_branch(dir.path());

        fs::create_dir_all(dir.path().join("baoyu-cover-image")).unwrap();
        fs::write(
            dir.path().join("baoyu-cover-image").join("SKILL.md"),
            "# Cover Image\n",
        )
        .unwrap();
        Command::new("git")
            .args(["add", "baoyu-cover-image"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Add cover image skill"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let engine = GitEngine::open(dir.path()).unwrap();
        let status = engine.get_status().unwrap();

        assert_eq!(status.status, "modified");
        assert_eq!(status.ahead, 1);
        assert_eq!(status.behind, 0);
        assert!(status.changes.iter().any(|change| {
            change.file_path == "baoyu-cover-image/SKILL.md" && change.change_type == "added"
        }));
    }

    #[test]
    fn test_get_status_falls_back_to_origin_branch_without_tracking_config() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        let branch = configure_tracking_branch(dir.path());
        remove_tracking_config(dir.path(), &branch);

        fs::create_dir_all(dir.path().join("local-skill")).unwrap();
        fs::write(
            dir.path().join("local-skill").join("SKILL.md"),
            "# Local Skill\n",
        )
        .unwrap();
        Command::new("git")
            .args(["add", "local-skill"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Add local skill"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let engine = GitEngine::open(dir.path()).unwrap();
        let status = engine.get_status().unwrap();

        assert_eq!(status.status, "modified");
        assert_eq!(status.ahead, 1);
        assert_eq!(status.behind, 0);
        assert!(status.changes.iter().any(|change| {
            change.file_path == "local-skill/SKILL.md" && change.change_type == "added"
        }));
    }

    #[test]
    fn test_ensure_upstream_tracking_repairs_missing_branch_config() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        let branch = configure_tracking_branch(dir.path());
        remove_tracking_config(dir.path(), &branch);

        let engine = GitEngine::open(dir.path()).unwrap();
        engine.ensure_upstream_tracking(&branch).unwrap();

        let local_ref = format!("refs/heads/{branch}");
        let upstream = engine.repo.branch_upstream_name(&local_ref).unwrap();
        assert_eq!(
            upstream.as_str(),
            Some(format!("refs/remotes/origin/{branch}").as_str())
        );
    }

    #[test]
    fn test_get_status_does_not_report_remote_only_changes_as_pending() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        let branch = configure_tracking_branch(dir.path());

        let engine = GitEngine::open(dir.path()).unwrap();
        let sig = engine
            .signature(Some("Remote User"), Some("remote@example.com"))
            .unwrap();
        let head_oid = engine.repo.head().unwrap().target().unwrap();
        let head_commit = engine.repo.find_commit(head_oid).unwrap();
        let head_tree = head_commit.tree().unwrap();
        let mut index = git2::Index::new().unwrap();
        index.read_tree(&head_tree).unwrap();
        let remote_blob = engine.repo.blob(b"remote only\n").unwrap();
        let entry = git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o100644,
            uid: 0,
            gid: 0,
            file_size: 12,
            id: remote_blob,
            flags: 0,
            flags_extended: 0,
            path: b"remote-only.txt".to_vec(),
        };
        index.add(&entry).unwrap();
        let remote_tree_oid = index.write_tree_to(&engine.repo).unwrap();
        let remote_tree = engine.repo.find_tree(remote_tree_oid).unwrap();
        let remote_commit_oid = engine
            .repo
            .commit(
                None,
                &sig,
                &sig,
                "Remote change",
                &remote_tree,
                &[&head_commit],
            )
            .unwrap();
        engine
            .repo
            .reference(
                &format!("refs/remotes/origin/{branch}"),
                remote_commit_oid,
                true,
                "test remote advance",
            )
            .unwrap();

        let status = engine.get_status().unwrap();

        assert_eq!(status.status, "idle");
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 1);
        assert!(status.changes.is_empty());
    }

    #[test]
    fn test_stage_and_commit() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::write(dir.path().join("README.md"), "# Updated\n").unwrap();
        let mut engine = GitEngine::open(dir.path()).unwrap();
        let result = engine.stage_and_push("test: update README", None, None, None);
        assert!(result.is_ok());
        let status = engine.get_status().unwrap();
        assert_eq!(status.status, "idle");
    }

    #[test]
    fn test_stage_and_push_filtered_only_commits_allowed_skill_prefix() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::create_dir_all(dir.path().join("webkong-one")).unwrap();
        fs::create_dir_all(dir.path().join("other-one")).unwrap();
        fs::write(dir.path().join("webkong-one").join("SKILL.md"), "webkong\n").unwrap();
        fs::write(dir.path().join("other-one").join("SKILL.md"), "other\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Add skills"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        fs::write(
            dir.path().join("webkong-one").join("SKILL.md"),
            "webkong updated\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("other-one").join("SKILL.md"),
            "other updated\n",
        )
        .unwrap();

        let filter = SkillSyncFilter {
            include_prefixes: vec!["webkong".to_string()],
            include_skill_ids: Vec::new(),
        };
        let mut engine = GitEngine::open(dir.path()).unwrap();
        let result = engine.stage_and_push_filtered("test: filtered", &filter, None, None, None);
        assert!(result.is_ok());

        let head_webkong = Command::new("git")
            .args(["show", "HEAD:webkong-one/SKILL.md"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let head_other = Command::new("git")
            .args(["show", "HEAD:other-one/SKILL.md"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        assert_eq!(
            String::from_utf8_lossy(&head_webkong.stdout),
            "webkong updated\n"
        );
        assert!(head_other.status.success());
        assert_eq!(String::from_utf8_lossy(&head_other.stdout), "other\n");
        assert!(dir.path().join("other-one").join("SKILL.md").exists());
    }

    #[test]
    fn test_open_or_init_does_not_commit_existing_skills_before_filtering() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("webkong-one")).unwrap();
        fs::create_dir_all(dir.path().join("other-one")).unwrap();
        fs::write(dir.path().join("webkong-one").join("SKILL.md"), "webkong\n").unwrap();
        fs::write(dir.path().join("other-one").join("SKILL.md"), "other\n").unwrap();

        let _engine = GitEngine::open_or_init(dir.path()).unwrap();

        let head_webkong = Command::new("git")
            .args(["show", "HEAD:webkong-one/SKILL.md"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let head_other = Command::new("git")
            .args(["show", "HEAD:other-one/SKILL.md"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let head_gitignore = Command::new("git")
            .args(["show", "HEAD:.gitignore"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        assert!(!head_webkong.status.success());
        assert!(!head_other.status.success());
        assert!(head_gitignore.status.success());
    }

    #[test]
    fn test_open_or_init_uses_configured_commit_identity_for_bootstrap_commit() {
        let dir = TempDir::new().unwrap();

        let _engine = GitEngine::open_or_init_with_identity(
            dir.path(),
            Some("Configured User"),
            Some("configured@example.com"),
        )
        .unwrap();

        let author = Command::new("git")
            .args(["log", "-1", "--format=%an <%ae>"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        assert_eq!(
            String::from_utf8_lossy(&author.stdout).trim(),
            "Configured User <configured@example.com>"
        );
    }

    #[test]
    fn test_filtered_snapshot_can_use_remote_parent_when_local_head_diverged() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::create_dir_all(dir.path().join("webkong-one")).unwrap();
        fs::write(dir.path().join("webkong-one").join("SKILL.md"), "webkong\n").unwrap();

        let mut engine = GitEngine::open(dir.path()).unwrap();
        let sig = engine
            .signature(Some("Remote User"), Some("remote@example.com"))
            .unwrap();
        let head_oid = engine.repo.head().unwrap().target().unwrap();
        let head_commit = engine.repo.find_commit(head_oid).unwrap();
        let head_tree = head_commit.tree().unwrap();
        let remote_parent_oid = engine
            .repo
            .commit(
                None,
                &sig,
                &sig,
                "Remote parent",
                &head_tree,
                &[&head_commit],
            )
            .unwrap();
        drop(head_tree);
        drop(head_commit);

        fs::write(dir.path().join("local-only.txt"), "local\n").unwrap();
        let local_result = engine.stage_and_push("local: diverge", None, None, None);
        assert!(local_result.is_ok());

        let filter = SkillSyncFilter {
            include_prefixes: vec!["webkong".to_string()],
            include_skill_ids: Vec::new(),
        };
        let result = engine.commit_filtered_snapshot(
            &filter,
            head_oid,
            Some(remote_parent_oid),
            Some("Configured User"),
            Some("configured@example.com"),
        );

        let commit_oid = result.unwrap().unwrap();
        let commit = engine.repo.find_commit(commit_oid).unwrap();
        assert_eq!(commit.parent_id(0).unwrap(), remote_parent_oid);
    }

    #[test]
    fn test_filtered_snapshot_reports_same_skill_conflict() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::create_dir_all(dir.path().join("webkong-one")).unwrap();
        fs::write(dir.path().join("webkong-one/SKILL.md"), "base\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Add base skill"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let mut engine = GitEngine::open(dir.path()).unwrap();
        let base_oid = engine.repo.head().unwrap().target().unwrap();
        let base_commit = engine.repo.find_commit(base_oid).unwrap();
        let base_tree = base_commit.tree().unwrap();
        let mut remote_index = engine.repo.index().unwrap();
        remote_index.read_tree(&base_tree).unwrap();
        let remote_blob = engine.repo.blob(b"remote\n").unwrap();
        let mut entry = remote_index
            .get_path(Path::new("webkong-one/SKILL.md"), 0)
            .unwrap();
        entry.id = remote_blob;
        entry.file_size = 7;
        remote_index.add(&entry).unwrap();
        let remote_tree_oid = remote_index.write_tree().unwrap();
        let remote_tree = engine.repo.find_tree(remote_tree_oid).unwrap();
        let sig = engine.signature(None, None).unwrap();
        let remote_oid = engine
            .repo
            .commit(
                None,
                &sig,
                &sig,
                "Remote edit",
                &remote_tree,
                &[&base_commit],
            )
            .unwrap();
        drop(remote_tree);
        drop(base_tree);
        drop(base_commit);

        fs::write(dir.path().join("webkong-one/SKILL.md"), "local\n").unwrap();
        let filter = SkillSyncFilter {
            include_prefixes: vec!["webkong".to_string()],
            include_skill_ids: Vec::new(),
        };
        let result =
            engine.commit_filtered_snapshot(&filter, base_oid, Some(remote_oid), None, None);

        match result {
            Err(RebaseFailure::Conflicted(paths)) => {
                assert_eq!(paths, vec!["webkong-one/SKILL.md"]);
            }
            _ => panic!("expected filtered merge conflict"),
        }
    }

    #[test]
    fn test_pull_conflict_aborts_rebase_and_restores_local_state() {
        let root = TempDir::new().unwrap();
        let remote = root.path().join("remote.git");
        let local = root.path().join("local");
        let peer = root.path().join("peer");
        fs::create_dir_all(&local).unwrap();
        Command::new("git")
            .args(["init", "--bare", remote.to_str().unwrap()])
            .output()
            .unwrap();
        init_git_repo(&local);
        Command::new("git")
            .args(["remote", "add", "origin", remote.to_str().unwrap()])
            .current_dir(&local)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "HEAD"])
            .current_dir(&local)
            .output()
            .unwrap();
        Command::new("git")
            .args(["clone", remote.to_str().unwrap(), peer.to_str().unwrap()])
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "peer@test.com"])
            .current_dir(&peer)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Peer User"])
            .current_dir(&peer)
            .output()
            .unwrap();

        fs::write(peer.join("README.md"), "# Remote\n").unwrap();
        Command::new("git")
            .args(["commit", "-am", "Remote edit"])
            .current_dir(&peer)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "origin", "HEAD"])
            .current_dir(&peer)
            .output()
            .unwrap();

        fs::write(local.join("README.md"), "# Local\n").unwrap();
        Command::new("git")
            .args(["commit", "-am", "Local edit"])
            .current_dir(&local)
            .output()
            .unwrap();
        let local_head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&local)
            .output()
            .unwrap();
        let local_head = String::from_utf8_lossy(&local_head.stdout)
            .trim()
            .to_string();

        let mut engine = GitEngine::open(&local).unwrap();
        let status = engine.pull(None, None, None).unwrap();

        assert_eq!(status.status, "conflicted");
        assert_eq!(status.changes.len(), 1);
        assert_eq!(status.changes[0].file_path, "README.md");
        assert_eq!(engine.repo.state(), git2::RepositoryState::Clean);
        assert!(!engine.repo.index().unwrap().has_conflicts());
        assert_eq!(
            engine.repo.head().unwrap().target().unwrap().to_string(),
            local_head
        );
        assert_eq!(
            fs::read_to_string(local.join("README.md")).unwrap(),
            "# Local\n"
        );
    }

    #[test]
    fn test_set_remote_url() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        let engine = GitEngine::open(dir.path()).unwrap();
        engine
            .set_remote_url("https://github.com/test/repo.git")
            .unwrap();
        let remote = engine.repo.find_remote("origin").unwrap();
        assert_eq!(remote.url().unwrap(), "https://github.com/test/repo.git");
    }

    #[test]
    fn test_update_remote_url() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        let engine = GitEngine::open(dir.path()).unwrap();
        engine
            .set_remote_url("https://github.com/test/repo.git")
            .unwrap();
        engine
            .set_remote_url("https://gitlab.com/test/repo.git")
            .unwrap();
        let remote = engine.repo.find_remote("origin").unwrap();
        assert_eq!(remote.url().unwrap(), "https://gitlab.com/test/repo.git");
    }
}
