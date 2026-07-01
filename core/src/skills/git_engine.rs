use std::io::Write;
use std::path::Path;

use git2::{Cred, RemoteCallbacks, Repository, Signature, Status, StatusOptions};

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
        // Create directory if it doesn't exist
        std::fs::create_dir_all(repo_path)
            .map_err(|e| format!("Failed to create directory {}: {}", repo_path.display(), e))?;

        let repo = Repository::init(repo_path)
            .map_err(|e| format!("Failed to init git repo at {}: {}", repo_path.display(), e))?;

        // Configure local user for commits
        let mut config = repo
            .config()
            .map_err(|e| format!("Failed to get repo config: {}", e))?;
        config
            .set_str("user.name", "SkillSync")
            .map_err(|e| format!("Failed to set user.name: {}", e))?;
        config
            .set_str("user.email", "skillsync@local")
            .map_err(|e| format!("Failed to set user.email: {}", e))?;
        drop(config);

        // Create .gitignore if it doesn't exist
        let gitignore_path = repo_path.join(".gitignore");
        if !gitignore_path.exists() {
            std::fs::write(&gitignore_path, ".DS_Store\n*.swp\n")
                .map_err(|e| format!("Failed to write .gitignore: {}", e))?;
        }

        // Stage and make initial commit
        let mut index = repo
            .index()
            .map_err(|e| format!("Failed to get index: {}", e))?;
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| format!("Failed to stage files: {}", e))?;
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
        if repo_path.join(".git").exists() {
            Self::open(repo_path)
        } else {
            Self::init(repo_path)
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

        // Collect pending changes
        let changes = self.get_pending_changes().unwrap_or_default();
        let has_changes = !changes.is_empty();

        if statuses.is_empty() && !has_changes {
            let mut info = GitStatusInfo::idle();
            info.branch = branch;
            return Ok(info);
        }

        let has_conflicts = statuses.iter().any(|s| {
            let status = s.status();
            status.contains(Status::CONFLICTED)
        });

        if has_conflicts {
            let count = statuses
                .iter()
                .filter(|s| s.status().contains(Status::CONFLICTED))
                .count();
            let mut info =
                GitStatusInfo::conflicted(&format!("{} file(s) have merge conflicts", count));
            info.branch = branch;
            info.ahead = ahead;
            info.behind = behind;
            info.has_changes = true;
            info.changes = changes;
            return Ok(info);
        }

        let modified_count = statuses.len();
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

        // Try to resolve upstream branch
        let upstream = match self
            .repo
            .branch_upstream_name(head.shorthand().unwrap_or(""))
        {
            Ok(name) => name,
            Err(_) => return Ok((0, 0)),
        };

        let upstream_ref = match self.repo.find_reference(&upstream.as_str().unwrap_or("")) {
            Ok(r) => r,
            Err(_) => return Ok((0, 0)),
        };
        let upstream_oid = match upstream_ref.target() {
            Some(oid) => oid,
            None => return Ok((0, 0)),
        };

        let (ahead, behind) = self
            .repo
            .graph_ahead_behind(head_oid, upstream_oid)
            .map_err(|e| format!("Failed to compute ahead/behind: {}", e))?;

        Ok((ahead as i32, behind as i32))
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

        debug_log!(" fetch_origin: completed");
        Ok(())
    }

    /// Auto-commit any pending changes before sync operations.
    /// Handles both initial commit (unborn HEAD) and subsequent commits.
    fn auto_commit(
        &mut self,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<(), String> {
        debug_log!(" auto_commit: checking pending changes");
        let changes = self.get_pending_changes()?;
        if changes.is_empty() {
            debug_log!(" auto_commit: no pending changes, skipping");
            return Ok(());
        }

        debug_log!(" auto_commit: {} pending changes", changes.len());
        for c in &changes {
            debug_log!(" auto_commit:   {} - {}", c.change_type, c.file_path);
        }

        let file_count = changes.len();

        let mut index = self
            .repo
            .index()
            .map_err(|e| format!("Failed to get index: {}", e))?;

        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| format!("Failed to stage files: {}", e))?;

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

    /// Pull (fetch + rebase) from origin. Auto-commits pending changes first.
    pub fn pull(
        &mut self,
        token: Option<&str>,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<GitStatusInfo, String> {
        debug_log!(" pull: start");
        self.auto_commit(user_name, user_email)?;
        let branch = self.current_branch()?;

        // Fetch
        self.fetch_origin(&branch, token)?;

        // Check FETCH_HEAD validity before rebase
        let fetch_head_path = self.repo.path().join("FETCH_HEAD");
        let fetch_ok = fetch_head_path.exists()
            && std::fs::metadata(&fetch_head_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false);

        if !fetch_ok {
            debug_log!(" pull: FETCH_HEAD missing or empty, nothing to rebase");
            return Ok(GitStatusInfo::synced());
        }

        debug_log!(" pull: FETCH_HEAD found, starting rebase");

        // Rebase onto FETCH_HEAD
        {
            let fetch_head = self
                .repo
                .find_reference("FETCH_HEAD")
                .map_err(|e| format!("Failed to find FETCH_HEAD: {}", e))?;
            let upstream = self
                .repo
                .reference_to_annotated_commit(&fetch_head)
                .map_err(|e| format!("Failed to resolve FETCH_HEAD: {}", e))?;
            drop(fetch_head);

            let mut rebase = self
                .repo
                .rebase(
                    None,
                    Some(&upstream),
                    None,
                    Some(&mut git2::RebaseOptions::new()),
                )
                .map_err(|e| format!("Failed to start rebase: {}", e))?;

            let sig = self.signature(user_name, user_email)?;

            while let Some(op) = rebase.next() {
                op.map_err(|e| format!("Rebase step failed: {}", e))?;
                if rebase.commit(None, &sig, None).is_err() {
                    // Nothing to commit, skip
                }
            }

            rebase
                .finish(None)
                .map_err(|e| format!("Failed to finish rebase: {}", e))?;
        }

        debug_log!(" pull: done");
        Ok(GitStatusInfo::synced())
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
        self.auto_commit(user_name, user_email)?;

        if self.has_remote() {
            debug_log!(" stage_and_push: has remote, starting pull_rebase");
            if let Err(e) = self.pull_rebase(token, user_name, user_email) {
                debug_log!(" stage_and_push: pull_rebase failed: {}", e);
                return Ok(GitStatusInfo::error(&format!("Pull rebase failed: {}", e)));
            }
            debug_log!(" stage_and_push: pull_rebase ok, starting push");
            self.push(token)?;
            debug_log!(" stage_and_push: push ok");
        } else {
            debug_log!(" stage_and_push: no remote configured");
        }

        Ok(GitStatusInfo::synced())
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
    ) -> Result<(), String> {
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
        self.fetch_origin(&branch, token)?;

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
            .map_err(|e| format!("Failed to find FETCH_HEAD: {}", e))?;
        let upstream = self
            .repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(|e| format!("Failed to resolve FETCH_HEAD: {}", e))?;

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
            .map_err(|e| format!("Failed to start rebase: {}", e))?;

        let sig = self.signature(user_name, user_email)?;

        // Iterate rebase steps
        while let Some(op) = rebase.next() {
            op.map_err(|e| format!("Rebase step failed: {}", e))?;
            if rebase.commit(None, &sig, None).is_err() {
                // Nothing to commit, skip
            }
        }

        rebase
            .finish(None)
            .map_err(|e| format!("Failed to finish rebase: {}", e))?;

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
