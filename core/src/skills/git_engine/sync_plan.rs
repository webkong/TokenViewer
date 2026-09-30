//! Resumable, non-destructive planning. Live files are changed only by apply.
use super::{GitEngine, SkillSyncFilter};
use git2::{Index, IndexEntry, IndexTime, ObjectType, Oid};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize)]
pub struct SyncRequest {
    pub action: String,
    pub id: Option<String>,
    pub filter: Option<SkillSyncFilter>,
    pub path: Option<String>,
    pub choice: Option<String>,
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    oid: String,
    mode: u32,
}

type Files = BTreeMap<String, Entry>;

#[derive(Clone, Serialize, Deserialize)]
struct Conflict {
    path: String,
    base: Option<Entry>,
    local: Option<Entry>,
    remote: Option<Entry>,
    choice: Option<String>,
    text: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Plan {
    version: u32,
    id: String,
    branch: String,
    local_ref: String,
    remote_key: String,
    head: String,
    remote: Option<String>,
    original: String,
    original_index: String,
    candidate: String,
    base: String,
    filter: Option<SkillSyncFilter>,
    conflicts: Vec<Conflict>,
    uploads: Vec<String>,
    downloads: Vec<String>,
    phase: String,
    commit: Option<String>,
    applied: Vec<String>,
}

impl GitEngine {
    /// CoreBridge serializes callers; a file lock also excludes another app process.
    pub fn sync_task(
        &mut self,
        request: SyncRequest,
        token: Option<&str>,
        name: Option<&str>,
        email: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.repo.path().join("tokenviewer-sync.lock"))
            .map_err(err)?;
        lock.try_lock().map_err(|_| "sync_busy".to_string())?;
        let result = match request.action.as_str() {
            "load" => self.load_plan()?.map(|p| self.plan_view(&p)).transpose(),
            "prepare" => {
                if let Some(plan) = self.load_plan()? {
                    return self
                        .plan_view(&plan)
                        .map(Some)
                        .map(|p| serde_json::json!({"ok":true,"plan":p}));
                }
                let plan = self.prepare_plan(request.filter, token, name, email)?;
                self.plan_view(&plan).map(Some)
            }
            "resolve" | "apply" | "discard" => {
                let mut plan = self.load_plan()?.ok_or("sync_missing")?;
                if request.id.as_deref() != Some(&plan.id) {
                    return Err("sync_changed".into());
                }
                match request.action.as_str() {
                    "resolve" => {
                        if plan.phase != "prepared" {
                            return Err("sync_recovery".into());
                        }
                        let conflict = plan
                            .conflicts
                            .iter_mut()
                            .find(|c| Some(c.path.as_str()) == request.path.as_deref())
                            .ok_or("sync_invalid_choice")?;
                        match request.choice.as_deref() {
                            Some("local" | "remote") => {
                                conflict.choice = request.choice;
                                conflict.text = None;
                            }
                            Some("manual") => {
                                let text = request.text.ok_or("sync_invalid_choice")?;
                                if text.len() > 1024 * 1024
                                    || has_markers(&text)
                                    || conflict
                                        .local
                                        .as_ref()
                                        .or(conflict.remote.as_ref())
                                        .is_some_and(|e| e.mode == 0o120000)
                                {
                                    return Err("sync_invalid_choice".into());
                                }
                                conflict.choice = Some("manual".into());
                                conflict.text = Some(text);
                            }
                            _ => return Err("sync_invalid_choice".into()),
                        }
                        self.save_plan(&plan)?;
                        self.plan_view(&plan).map(Some)
                    }
                    "apply" => {
                        self.apply_plan(&mut plan, token, name, email)?;
                        self.plan_view(&plan).map(Some)
                    }
                    _ => {
                        // Discard never rolls back a publication. Before local
                        // application, starting over is safe even if push succeeded.
                        if plan.phase == "applying" {
                            return Err("sync_recovery".into());
                        }
                        // Keep manual resolutions and recovery metadata alongside
                        // the pinned snapshots even when starting a fresh plan.
                        fs::rename(
                            self.plan_path(),
                            self.repo
                                .path()
                                .join(format!("tokenviewer-sync-{}.json", plan.id)),
                        )
                        .map_err(err)?;
                        Ok(None)
                    }
                }
            }
            _ => Err("sync_invalid_action".into()),
        }?;
        Ok(serde_json::json!({"ok":true,"plan":result}))
    }

    fn plan_path(&self) -> PathBuf {
        self.repo.path().join("tokenviewer-sync.json")
    }

    pub(super) fn has_pending_sync_plan(&self) -> Result<bool, String> {
        Ok(self.load_plan()?.is_some_and(|p| p.phase != "complete"))
    }

    fn load_plan(&self) -> Result<Option<Plan>, String> {
        match fs::read(self.plan_path()) {
            Ok(data) => {
                let plan: Plan = serde_json::from_slice(&data).map_err(err)?;
                if plan.version != 1 || uuid::Uuid::parse_str(&plan.id).is_err() {
                    return Err("sync_invalid_plan".into());
                }
                Ok(Some(plan))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(err(e)),
        }
    }

    fn save_plan(&self, plan: &Plan) -> Result<(), String> {
        let tmp = self
            .repo
            .path()
            .join(format!("tokenviewer-sync-{}.tmp", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp).map_err(err)?;
        file.write_all(&serde_json::to_vec(plan).map_err(err)?)
            .map_err(err)?;
        file.sync_all().map_err(err)?;
        fs::rename(&tmp, self.plan_path()).map_err(err)
    }

    fn snapshot_worktree(&self) -> Result<Oid, String> {
        let mut index = self.repo.index().map_err(err)?;
        index.read(true).map_err(err)?;
        if index.has_conflicts() {
            return Err("sync_existing_conflict".into());
        }
        // libgit2 caches this index on Repository even after the wrapper drops.
        // Restore its in-memory contents as well as leaving .git/index untouched.
        let result = (|| {
            index
                .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
                .map_err(err)?;
            index.update_all(["*"], None).map_err(err)?;
            index.write_tree().map_err(err)
        })();
        index.read(true).map_err(err)?;
        result
    }

    fn index_tree(&self) -> Result<Oid, String> {
        let mut index = self.repo.index().map_err(err)?;
        index.read(true).map_err(err)?;
        index.write_tree().map_err(err)
    }

    fn remote_key(&self) -> Result<String, String> {
        let remote = self.repo.find_remote("origin").map_err(err)?;
        Ok(Oid::hash_object(
            ObjectType::Blob,
            remote.url().ok_or("sync_no_remote")?.as_bytes(),
        )
        .map_err(err)?
        .to_string())
    }

    fn files(&self, tree: &str) -> Result<Files, String> {
        let tree = self.repo.find_tree(oid(tree)?).map_err(err)?;
        let mut files = Files::new();
        let mut invalid = false;
        tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() == Some(ObjectType::Tree) {
                return git2::TreeWalkResult::Ok;
            }
            if let Some(name) = entry.name() {
                let path = format!("{root}{name}");
                if valid_path(&path).is_err()
                    || ![0o100644, 0o100755, 0o120000].contains(&entry.filemode())
                {
                    invalid = true;
                }
                files.insert(
                    path,
                    Entry {
                        oid: entry.id().to_string(),
                        mode: entry.filemode() as u32,
                    },
                );
            } else {
                invalid = true;
            }
            git2::TreeWalkResult::Ok
        })
        .map_err(err)?;
        if invalid {
            return Err("sync_unsupported_path".into());
        }
        // Portable repositories must not alias two paths on case-insensitive macOS.
        let mut keys = BTreeSet::new();
        for path in files.keys() {
            if !keys.insert(path.to_lowercase()) {
                return Err("sync_unsupported_path".into());
            }
        }
        Ok(files)
    }

    fn write_files(&self, files: &Files) -> Result<Oid, String> {
        let mut index = Index::new().map_err(err)?;
        for (path, entry) in files {
            valid_path(path)?;
            index
                .add(&IndexEntry {
                    ctime: IndexTime::new(0, 0),
                    mtime: IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: entry.mode,
                    uid: 0,
                    gid: 0,
                    file_size: 0,
                    id: oid(&entry.oid)?,
                    flags: 0,
                    flags_extended: 0,
                    path: path.as_bytes().to_vec(),
                })
                .map_err(err)?;
        }
        index.write_tree_to(&self.repo).map_err(err)
    }

    fn pin_tree(
        &self,
        plan: &Plan,
        label: &str,
        tree: Oid,
        name: Option<&str>,
        email: Option<&str>,
    ) -> Result<(), String> {
        let sig = self.signature(name, email)?;
        let tree = self.repo.find_tree(tree).map_err(err)?;
        self.repo
            .commit(
                Some(&format!("refs/tokenviewer/sync/{}/{label}", plan.id)),
                &sig,
                &sig,
                "TokenViewer sync recovery snapshot",
                &tree,
                &[],
            )
            .map_err(err)?;
        Ok(())
    }

    fn prepare_plan(
        &self,
        filter: Option<SkillSyncFilter>,
        token: Option<&str>,
        name: Option<&str>,
        email: Option<&str>,
    ) -> Result<Plan, String> {
        if self.sync_blocked_status()?.is_some() {
            return Err("sync_existing_conflict".into());
        }
        if filter.as_ref().is_some_and(SkillSyncFilter::is_empty) {
            return Err("sync_empty_scope".into());
        }
        let head = self.repo.head().map_err(err)?;
        if !head.is_branch() {
            return Err("sync_detached".into());
        }
        let head_oid = head.target().ok_or("sync_detached")?;
        let original_index = self.index_tree()?;
        let original = self.snapshot_worktree()?;
        let remote = self.fetch_remote_head(token)?;
        let base_commit = match remote {
            Some(remote) => self
                .repo
                .merge_base(head_oid, remote)
                .map_err(|_| "sync_unrelated_history")?,
            None => head_oid,
        };
        let base = if remote.is_none() {
            self.write_files(&Files::new())?
        } else {
            self.repo.find_commit(base_commit).map_err(err)?.tree_id()
        };
        let original_files = self.files(&original.to_string())?;
        let base_files = self.files(&base.to_string())?;
        let candidate_files = if let Some(filter) = &filter {
            let mut files = base_files.clone();
            files.retain(|path, _| !filter.allows_path(path));
            files.extend(
                original_files
                    .iter()
                    .filter(|(path, _)| filter.allows_path(path))
                    .map(|(p, e)| (p.clone(), e.clone())),
            );
            files
        } else {
            original_files
        };
        let candidate = self.write_files(&candidate_files)?;
        let remote_tree = match remote {
            Some(remote) => self.repo.find_commit(remote).map_err(err)?.tree_id(),
            None => self.write_files(&Files::new())?,
        };
        // An empty remote has no ancestry: publish the candidate unchanged.
        let merge_base = if remote.is_none() { remote_tree } else { base };
        let merged = self
            .repo
            .merge_trees(
                &self.repo.find_tree(merge_base).map_err(err)?,
                &self.repo.find_tree(candidate).map_err(err)?,
                &self.repo.find_tree(remote_tree).map_err(err)?,
                None,
            )
            .map_err(err)?;
        let mut conflicts = Vec::new();
        for conflict in merged.conflicts().map_err(err)? {
            let c = conflict.map_err(err)?;
            let entries = [&c.ancestor, &c.our, &c.their];
            let paths: BTreeSet<_> = entries
                .iter()
                .filter_map(|e| e.as_ref())
                .map(|e| String::from_utf8(e.path.clone()).map_err(err))
                .collect::<Result<_, _>>()?;
            // Rename and file/directory conflicts need a path-mapping UI. Fail
            // closed rather than silently losing one of the paths.
            if paths.len() != 1 {
                return Err("sync_complex_conflict".into());
            }
            let path = paths.into_iter().next().ok_or("sync_complex_conflict")?;
            valid_path(&path)?;
            let convert = |e: Option<IndexEntry>| {
                e.map(|e| Entry {
                    oid: e.id.to_string(),
                    mode: e.mode,
                })
            };
            conflicts.push(Conflict {
                path,
                base: convert(c.ancestor),
                local: convert(c.our),
                remote: convert(c.their),
                choice: None,
                text: None,
            });
        }
        conflicts.sort_by(|a, b| a.path.cmp(&b.path));
        let remote_files = self.files(&remote_tree.to_string())?;
        let plan = Plan {
            version: 1,
            id: uuid::Uuid::new_v4().to_string(),
            branch: self.sync_branch(),
            local_ref: head.name().ok_or("sync_detached")?.into(),
            remote_key: self.remote_key()?,
            head: head_oid.to_string(),
            remote: remote.map(|r| r.to_string()),
            original: original.to_string(),
            original_index: original_index.to_string(),
            candidate: candidate.to_string(),
            base: merge_base.to_string(),
            uploads: changed(&base_files, &candidate_files),
            downloads: changed(&base_files, &remote_files),
            filter,
            conflicts,
            phase: "prepared".into(),
            commit: None,
            applied: vec![],
        };
        // Durable references keep all input blobs alive across Git GC and restart.
        for (label, tree) in [
            ("local", original),
            ("index", original_index),
            ("candidate", candidate),
            ("base", merge_base),
            ("remote", remote_tree),
        ] {
            self.pin_tree(&plan, label, tree, name, email)?;
        }
        self.save_plan(&plan)?;
        Ok(plan)
    }

    fn text_version(&self, entry: &Option<Entry>) -> Result<serde_json::Value, String> {
        match entry {
            None => Ok(serde_json::json!({"exists":false,"text":null})),
            Some(entry) => {
                let blob = self.repo.find_blob(oid(&entry.oid)?).map_err(err)?;
                let text = if blob.size() <= 256 * 1024 && !blob.is_binary() {
                    std::str::from_utf8(blob.content()).ok()
                } else {
                    None
                };
                Ok(serde_json::json!({"exists":true,"text":text,"mode":entry.mode}))
            }
        }
    }

    fn plan_view(&self, plan: &Plan) -> Result<serde_json::Value, String> {
        let conflicts = plan.conflicts.iter().map(|c| Ok(serde_json::json!({
            "path":c.path, "base":self.text_version(&c.base)?, "local":self.text_version(&c.local)?,
            "remote":self.text_version(&c.remote)?, "choice":c.choice, "text":c.text
        }))).collect::<Result<Vec<_>, String>>()?;
        Ok(
            serde_json::json!({"id":plan.id,"phase":plan.phase,"branch":plan.branch,
            "uploads":plan.uploads,"downloads":plan.downloads,"conflicts":conflicts,
            "recovery_ref":format!("refs/tokenviewer/sync/{}/local",plan.id)}),
        )
    }

    fn resolved_tree(&self, plan: &Plan) -> Result<Oid, String> {
        let remote_tree = self
            .repo
            .find_reference(&format!("refs/tokenviewer/sync/{}/remote", plan.id))
            .map_err(err)?
            .peel_to_tree()
            .map_err(err)?;
        let index = self
            .repo
            .merge_trees(
                &self.repo.find_tree(oid(&plan.base)?).map_err(err)?,
                &self.repo.find_tree(oid(&plan.candidate)?).map_err(err)?,
                &remote_tree,
                None,
            )
            .map_err(err)?;
        let mut files = Files::new();
        for entry in index.iter().filter(|e| e.flags & 0x3000 == 0) {
            files.insert(
                String::from_utf8(entry.path).map_err(err)?,
                Entry {
                    oid: entry.id.to_string(),
                    mode: entry.mode,
                },
            );
        }
        for c in &plan.conflicts {
            let selected = match c.choice.as_deref() {
                Some("local") => c.local.clone(),
                Some("remote") => c.remote.clone(),
                Some("manual") => {
                    let text = c.text.as_deref().ok_or("sync_invalid_choice")?;
                    if has_markers(text) {
                        return Err("sync_invalid_choice".into());
                    }
                    Some(Entry {
                        oid: self.repo.blob(text.as_bytes()).map_err(err)?.to_string(),
                        mode: c
                            .local
                            .as_ref()
                            .or(c.remote.as_ref())
                            .map(|e| e.mode)
                            .unwrap_or(0o100644),
                    })
                }
                _ => return Err("sync_unresolved".into()),
            };
            files.remove(&c.path);
            if let Some(entry) = selected {
                files.insert(c.path.clone(), entry);
            }
        }
        self.write_files(&files)
    }

    fn validate_local(&self, plan: &Plan) -> Result<(), String> {
        let head = self.repo.head().map_err(err)?;
        if head.name() != Some(&plan.local_ref)
            || head.target().map(|h| h.to_string()).as_deref() != Some(&plan.head)
            || self.repo.state() != git2::RepositoryState::Clean
            || self.index_tree()?.to_string() != plan.original_index
            || self.snapshot_worktree()?.to_string() != plan.original
        {
            return Err("sync_changed".into());
        }
        Ok(())
    }

    fn apply_plan(
        &self,
        plan: &mut Plan,
        token: Option<&str>,
        name: Option<&str>,
        email: Option<&str>,
    ) -> Result<(), String> {
        if plan.phase == "complete" {
            return Ok(());
        }
        if self.sync_branch() != plan.branch || self.remote_key()? != plan.remote_key {
            return Err("sync_changed".into());
        }
        let remote = self.fetch_remote_head(token)?.map(|r| r.to_string());
        if plan.phase == "prepared" {
            self.validate_local(plan)?;
            if remote != plan.remote {
                return Err("sync_changed".into());
            }
            let tree = self.resolved_tree(plan)?;
            let tree = self.repo.find_tree(tree).map_err(err)?;
            let sig = self.signature(name, email)?;
            let parent = plan
                .remote
                .as_deref()
                .map(|p| self.repo.find_commit(oid(p)?).map_err(err))
                .transpose()?;
            let local_parent = self.repo.find_commit(oid(&plan.head)?).map_err(err)?;
            let mut parents: Vec<_> = parent.iter().collect();
            if plan.filter.is_none()
                && !parent.as_ref().is_some_and(|p| {
                    p.id() == local_parent.id()
                        || self
                            .repo
                            .graph_descendant_of(p.id(), local_parent.id())
                            .unwrap_or(false)
                })
            {
                parents.push(&local_parent);
            }
            let commit = if parent.as_ref().is_some_and(|p| p.tree_id() == tree.id()) {
                parent.as_ref().unwrap().id()
            } else {
                self.repo
                    .commit(
                        None,
                        &sig,
                        &sig,
                        "Sync Skills with TokenViewer",
                        &tree,
                        &parents,
                    )
                    .map_err(err)?
            };
            plan.commit = Some(commit.to_string());
            self.preflight_files(plan, commit)?;
            self.repo
                .reference(
                    &format!("refs/tokenviewer/sync/{}/result", plan.id),
                    commit,
                    true,
                    "Sync result",
                )
                .map_err(err)?;
            plan.phase = "publishing".into();
            self.save_plan(plan)?;
        }
        let commit = oid(plan.commit.as_deref().ok_or("sync_invalid_plan")?)?;
        if plan.phase == "publishing" {
            // A previous push may have succeeded before the response/journal write.
            if remote.as_deref() != Some(&commit.to_string()) {
                self.validate_local(plan)?;
                if remote != plan.remote {
                    return Err("sync_changed".into());
                }
                self.push_filtered_commit(commit, token, false)?;
            }
            plan.phase = "published".into();
            self.save_plan(plan)?;
        }
        if plan.phase == "published" {
            self.validate_local(plan)?;
            plan.phase = "applying".into();
            self.save_plan(plan)?;
        }
        if plan.phase != "applying" {
            return Err("sync_invalid_plan".into());
        }
        self.apply_files(plan, commit)?;
        plan.phase = "complete".into();
        self.save_plan(plan)
    }

    fn target_files(&self, plan: &Plan, commit: Oid) -> Result<Files, String> {
        let before = self.files(&plan.original)?;
        let result = self.files(
            &self
                .repo
                .find_commit(commit)
                .map_err(err)?
                .tree_id()
                .to_string(),
        )?;
        let mut target = result.clone();
        if let Some(filter) = &plan.filter {
            // Outside upload scope, apply incoming updates only to files unchanged locally.
            let baseline = self.files(&plan.base)?;
            for path in changed(&baseline, &before)
                .into_iter()
                .filter(|p| !filter.allows_path(p))
            {
                target.remove(&path);
                if let Some(entry) = before.get(&path) {
                    target.insert(path, entry.clone());
                }
            }
        }
        Ok(target)
    }

    fn preflight_files(&self, plan: &Plan, commit: Oid) -> Result<(), String> {
        let before = self.files(&plan.original)?;
        let target = self.target_files(plan, commit)?;
        let workdir = self.repo.workdir().ok_or("sync_no_workdir")?;
        // Preflight all paths before making any changes; never follow parent symlinks.
        for path in &changed(&before, &target) {
            let full = safe_file(workdir, path)?;
            let current = disk_entry(&full)?;
            if current.as_ref() != before.get(path) && current.as_ref() != target.get(path) {
                return Err("sync_recovery".into());
            }
        }
        Ok(())
    }

    fn target_index(&self, plan: &Plan, commit: Oid) -> Result<Oid, String> {
        let tree = self.repo.find_commit(commit).map_err(err)?.tree_id();
        let Some(filter) = &plan.filter else {
            return Ok(tree);
        };
        let mut files = self.files(&tree.to_string())?;
        let old_head = self.files(
            &self
                .repo
                .find_commit(oid(&plan.head)?)
                .map_err(err)?
                .tree_id()
                .to_string(),
        )?;
        let staged = self.files(&plan.original_index)?;
        for path in changed(&old_head, &staged)
            .into_iter()
            .filter(|p| !filter.allows_path(p))
        {
            files.remove(&path);
            if let Some(entry) = staged.get(&path) {
                files.insert(path, entry.clone());
            }
        }
        self.write_files(&files)
    }

    fn apply_files(&self, plan: &mut Plan, commit: Oid) -> Result<(), String> {
        self.preflight_files(plan, commit)?;
        let before = self.files(&plan.original)?;
        let target = self.target_files(plan, commit)?;
        let workdir = self.repo.workdir().ok_or("sync_no_workdir")?;
        let paths = changed(&before, &target);
        let head = self.repo.head().map_err(err)?;
        if head.name() != Some(&plan.local_ref)
            || ![Some(oid(&plan.head)?), Some(commit)].contains(&head.target())
        {
            return Err("sync_recovery".into());
        }
        let current_index = self.index_tree()?;
        let final_tree = self.target_index(plan, commit)?;
        if current_index != oid(&plan.original_index)? && current_index != final_tree {
            return Err("sync_recovery".into());
        }
        // Every write is journaled and idempotent. Recovery never overwrites a
        // path whose content is neither the captured input nor the output.
        for path in &paths {
            let full = safe_file(workdir, path)?;
            let current = disk_entry(&full)?;
            if current.as_ref() == target.get(path) {
                continue;
            }
            if current.as_ref() != before.get(path) {
                return Err("sync_recovery".into());
            }
            match target.get(path) {
                Some(entry) => {
                    let parent = full.parent().ok_or("sync_unsupported_path")?;
                    fs::create_dir_all(parent).map_err(err)?;
                    let blob = self.repo.find_blob(oid(&entry.oid)?).map_err(err)?;
                    let tmp = parent.join(format!(".tokenviewer-{}.tmp", uuid::Uuid::new_v4()));
                    write_blob(&tmp, blob.content(), entry.mode)?;
                    if disk_entry(&full)?.as_ref() != before.get(path) {
                        let _ = fs::remove_file(&tmp);
                        return Err("sync_recovery".into());
                    }
                    fs::rename(&tmp, &full).map_err(err)?;
                }
                None => {
                    fs::remove_file(&full).map_err(err)?;
                }
            }
            plan.applied.push(path.clone());
            self.save_plan(plan)?;
        }
        // Recheck after the file writes: another Git client may have staged or
        // committed while the application was busy applying this task.
        let head_now = self.repo.head().map_err(err)?;
        let expected_head = head_now.target().ok_or("sync_recovery")?;
        if head_now.name() != Some(&plan.local_ref)
            || ![oid(&plan.head)?, commit].contains(&expected_head)
            || ![oid(&plan.original_index)?, final_tree].contains(&self.index_tree()?)
        {
            return Err("sync_recovery".into());
        }
        let mut index = self.repo.index().map_err(err)?;
        index
            .read_tree(&self.repo.find_tree(final_tree).map_err(err)?)
            .map_err(err)?;
        index.write().map_err(err)?;
        self.repo
            .reference_matching(
                &plan.local_ref,
                commit,
                true,
                expected_head,
                "TokenViewer applied sync plan",
            )
            .map_err(err)?;
        Ok(())
    }
}

fn oid(value: &str) -> Result<Oid, String> {
    Oid::from_str(value).map_err(err)
}
fn err(value: impl std::fmt::Display) -> String {
    value.to_string()
}
fn changed(before: &Files, after: &Files) -> Vec<String> {
    before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|p| before.get(*p) != after.get(*p))
        .cloned()
        .collect()
}
fn has_markers(text: &str) -> bool {
    text.lines().any(|line| {
        line.starts_with("<<<<<<< ") || line.starts_with(">>>>>>> ") || line == "======="
    })
}
fn valid_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path
            .split('/')
            .any(|p| p.eq_ignore_ascii_case(".git") || p.contains(':'))
    {
        return Err("sync_unsupported_path".into());
    }
    Ok(())
}
fn safe_file(root: &Path, path: &str) -> Result<PathBuf, String> {
    valid_path(path)?;
    let mut full = root.to_path_buf();
    let parts: Vec<_> = Path::new(path).components().collect();
    for part in &parts[..parts.len() - 1] {
        full.push(part);
        match fs::symlink_metadata(&full) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err("sync_unsupported_path".into())
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(err(e)),
            _ => {}
        }
    }
    Ok(root.join(path))
}
fn disk_entry(path: &Path) -> Result<Option<Entry>, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(err(e)),
    };
    let (bytes, mode) = if meta.file_type().is_symlink() {
        let link = fs::read_link(path).map_err(err)?;
        (
            link.to_str()
                .ok_or("sync_unsupported_path")?
                .as_bytes()
                .to_vec(),
            0o120000,
        )
    } else if meta.is_file() {
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        (
            fs::read(path).map_err(err)?,
            if executable { 0o100755 } else { 0o100644 },
        )
    } else {
        return Err("sync_unsupported_path".into());
    };
    Ok(Some(Entry {
        oid: Oid::hash_object(ObjectType::Blob, &bytes)
            .map_err(err)?
            .to_string(),
        mode,
    }))
}
fn write_blob(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    if mode == 0o120000 {
        #[cfg(unix)]
        {
            return std::os::unix::fs::symlink(std::str::from_utf8(bytes).map_err(err)?, path)
                .map_err(err);
        }
        #[cfg(not(unix))]
        {
            return Err("sync_unsupported_path".into());
        }
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if mode == 0o100755 { 0o755 } else { 0o644 });
    }
    let mut file = options.open(path).map_err(err)?;
    file.write_all(bytes).map_err(err)?;
    file.sync_all().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn put(root: &Path, path: &str, text: &str) {
        fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        fs::write(root.join(path), text).unwrap();
    }

    fn fixture() -> (TempDir, GitEngine, PathBuf) {
        let root = TempDir::new().unwrap();
        let local = root.path().join("local");
        let remote = root.path().join("remote.git");
        let peer = root.path().join("peer");
        let mut engine = GitEngine::init(&local).unwrap();
        put(&local, "one/SKILL.md", "base\n");
        put(&local, "two/SKILL.md", "untouched\n");
        engine.auto_commit(None, None).unwrap();
        git(
            root.path(),
            &[
                "init",
                "--bare",
                "--initial-branch=main",
                remote.to_str().unwrap(),
            ],
        );
        engine.set_remote_url(remote.to_str().unwrap()).unwrap();
        engine.push(None, false).unwrap();
        git(
            root.path(),
            &["clone", remote.to_str().unwrap(), peer.to_str().unwrap()],
        );
        git(&peer, &["config", "user.name", "Peer"]);
        git(&peer, &["config", "user.email", "peer@test.local"]);
        (root, engine, peer)
    }

    fn publish(peer: &Path) {
        git(peer, &["add", "--all"]);
        git(peer, &["commit", "-m", "Peer change"]);
        git(peer, &["push"]);
    }

    #[test]
    fn conflict_plan_survives_restart_without_touching_live_files() {
        let (_root, mut engine, peer) = fixture();
        let local = engine.repo.workdir().unwrap().to_path_buf();
        put(&local, "one/SKILL.md", "local\n");
        put(&peer, "one/SKILL.md", "remote\n");
        publish(&peer);
        let before = engine.repo.head().unwrap().target();
        let plan = engine.prepare_plan(None, None, None, None).unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(
            fs::read_to_string(local.join("one/SKILL.md")).unwrap(),
            "local\n"
        );
        assert_eq!(engine.repo.head().unwrap().target(), before);
        assert!(!engine.repo.index().unwrap().has_conflicts());
        drop(engine);
        engine = GitEngine::open(&local).unwrap();
        let request = SyncRequest {
            action: "resolve".into(),
            id: Some(plan.id),
            filter: None,
            path: Some("one/SKILL.md".into()),
            choice: Some("manual".into()),
            text: Some("local and remote\n".into()),
        };
        engine.sync_task(request, None, None, None).unwrap();
        let mut plan = engine.load_plan().unwrap().unwrap();
        engine.apply_plan(&mut plan, None, None, None).unwrap();
        assert_eq!(plan.phase, "complete");
        assert_eq!(
            fs::read_to_string(local.join("one/SKILL.md")).unwrap(),
            "local and remote\n"
        );
        assert!(!engine.get_status().unwrap().has_changes);
        let backup = engine
            .repo
            .find_reference(&format!("refs/tokenviewer/sync/{}/local", plan.id))
            .unwrap()
            .peel_to_tree()
            .unwrap();
        let entry = backup.get_path(Path::new("one/SKILL.md")).unwrap();
        assert_eq!(
            engine.repo.find_blob(entry.id()).unwrap().content(),
            b"local\n"
        );
    }

    #[test]
    fn stale_local_or_remote_plan_does_not_publish() {
        let (_root, engine, peer) = fixture();
        let local = engine.repo.workdir().unwrap();
        put(local, "one/SKILL.md", "local\n");
        let mut plan = engine.prepare_plan(None, None, None, None).unwrap();
        put(local, "one/SKILL.md", "new edit\n");
        assert_eq!(
            engine.apply_plan(&mut plan, None, None, None).unwrap_err(),
            "sync_changed"
        );
        assert_eq!(plan.phase, "prepared");
        put(local, "one/SKILL.md", "local\n");
        put(&peer, "two/SKILL.md", "remote changed\n");
        publish(&peer);
        assert_eq!(
            engine.apply_plan(&mut plan, None, None, None).unwrap_err(),
            "sync_changed"
        );
        assert_eq!(
            fs::read_to_string(local.join("one/SKILL.md")).unwrap(),
            "local\n"
        );
    }

    #[test]
    fn filtered_sync_writes_remote_edits_and_preserves_excluded_local_edits() {
        let (_root, engine, peer) = fixture();
        let local = engine.repo.workdir().unwrap();
        put(local, "one/local.md", "local addition\n");
        put(local, "two/SKILL.md", "staged excluded edit\n");
        git(local, &["add", "two/SKILL.md"]);
        put(local, "two/SKILL.md", "excluded edit\n");
        put(&peer, "one/SKILL.md", "remote update\n");
        publish(&peer);
        let filter = SkillSyncFilter {
            include_prefixes: vec![],
            include_skill_ids: vec!["one".into()],
        };
        let mut plan = engine.prepare_plan(Some(filter), None, None, None).unwrap();
        assert!(plan.conflicts.is_empty());
        engine.apply_plan(&mut plan, None, None, None).unwrap();
        assert_eq!(
            fs::read_to_string(local.join("one/SKILL.md")).unwrap(),
            "remote update\n"
        );
        assert_eq!(
            fs::read_to_string(local.join("two/SKILL.md")).unwrap(),
            "excluded edit\n"
        );
        assert_eq!(
            engine
                .get_pending_changes()
                .unwrap()
                .iter()
                .map(|p| p.file_path.as_str())
                .collect::<Vec<_>>(),
            vec!["two/SKILL.md"]
        );
        let remote = engine.fetch_remote_head(None).unwrap().unwrap();
        let tree = engine.repo.find_commit(remote).unwrap().tree().unwrap();
        let blob = tree.get_path(Path::new("two/SKILL.md")).unwrap();
        assert_eq!(
            engine.repo.find_blob(blob.id()).unwrap().content(),
            b"untouched\n"
        );
        let staged = engine
            .repo
            .index()
            .unwrap()
            .get_path(Path::new("two/SKILL.md"), 0)
            .unwrap();
        assert_eq!(
            engine.repo.find_blob(staged.id).unwrap().content(),
            b"staged excluded edit\n"
        );
    }

    #[test]
    fn empty_remote_filtered_sync_never_uploads_or_deletes_excluded_committed_files() {
        let root = TempDir::new().unwrap();
        let local = root.path().join("local");
        let remote = root.path().join("remote.git");
        let mut engine = GitEngine::init(&local).unwrap();
        put(&local, "one/SKILL.md", "selected\n");
        put(&local, "two/SKILL.md", "private\n");
        engine.auto_commit(None, None).unwrap();
        git(
            root.path(),
            &[
                "init",
                "--bare",
                "--initial-branch=main",
                remote.to_str().unwrap(),
            ],
        );
        engine.set_remote_url(remote.to_str().unwrap()).unwrap();
        let filter = SkillSyncFilter {
            include_prefixes: vec![],
            include_skill_ids: vec!["one".into()],
        };
        let mut plan = engine.prepare_plan(Some(filter), None, None, None).unwrap();
        assert!(plan.downloads.is_empty());
        engine.apply_plan(&mut plan, None, None, None).unwrap();
        assert_eq!(
            fs::read_to_string(local.join("two/SKILL.md")).unwrap(),
            "private\n"
        );
        let published = engine.repo.head().unwrap().peel_to_tree().unwrap();
        assert!(published.get_path(Path::new("two/SKILL.md")).is_err());
        assert_eq!(
            engine
                .repo
                .head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .parent_count(),
            0
        );
    }

    #[test]
    fn published_task_resumes_without_republishing_or_overwriting_new_edits() {
        let (_root, engine, _peer) = fixture();
        let local = engine.repo.workdir().unwrap();
        put(local, "one/SKILL.md", "local\n");
        let mut plan = engine.prepare_plan(None, None, None, None).unwrap();
        // Simulate a lost response after a completed push but before local apply.
        let tree = engine
            .repo
            .find_tree(engine.resolved_tree(&plan).unwrap())
            .unwrap();
        let parent = engine
            .repo
            .find_commit(oid(plan.remote.as_deref().unwrap()).unwrap())
            .unwrap();
        let sig = engine.signature(None, None).unwrap();
        let commit = engine
            .repo
            .commit(None, &sig, &sig, "Sync", &tree, &[&parent])
            .unwrap();
        engine.push_filtered_commit(commit, None, false).unwrap();
        plan.phase = "publishing".into();
        plan.commit = Some(commit.to_string());
        engine.save_plan(&plan).unwrap();
        put(local, "one/SKILL.md", "later edit\n");
        assert_eq!(
            engine.apply_plan(&mut plan, None, None, None).unwrap_err(),
            "sync_changed"
        );
        assert_eq!(
            fs::read_to_string(local.join("one/SKILL.md")).unwrap(),
            "later edit\n"
        );
        put(local, "one/SKILL.md", "local\n");
        engine.apply_plan(&mut plan, None, None, None).unwrap();
        assert_eq!(engine.repo.head().unwrap().target(), Some(commit));
        assert_eq!(plan.phase, "complete");
    }

    #[test]
    fn deleting_vs_editing_requires_explicit_choice() {
        let (_root, engine, peer) = fixture();
        let local = engine.repo.workdir().unwrap();
        fs::remove_file(local.join("one/SKILL.md")).unwrap();
        put(&peer, "one/SKILL.md", "remote edit\n");
        publish(&peer);
        let mut plan = engine.prepare_plan(None, None, None, None).unwrap();
        assert_eq!(
            engine.apply_plan(&mut plan, None, None, None).unwrap_err(),
            "sync_unresolved"
        );
        assert!(plan.conflicts[0].local.is_none());
        plan.conflicts[0].choice = Some("remote".into());
        engine.apply_plan(&mut plan, None, None, None).unwrap();
        assert_eq!(
            fs::read_to_string(local.join("one/SKILL.md"))
                .unwrap()
                .replace("\r\n", "\n"),
            "remote edit\n"
        );
    }

    #[test]
    fn legacy_push_refuses_stash_conflict_markers() {
        let (_root, mut engine, peer) = fixture();
        let local = engine.repo.workdir().unwrap().to_path_buf();
        put(&local, "one/SKILL.md", "local\n");
        put(&peer, "one/SKILL.md", "remote\n");
        publish(&peer);
        assert_eq!(engine.pull(None, None, None).unwrap().status, "conflicted");
        let head = engine.repo.head().unwrap().target();
        assert_eq!(
            engine
                .stage_and_push("sync", None, None, None)
                .unwrap()
                .status,
            "conflicted"
        );
        assert_eq!(engine.repo.head().unwrap().target(), head);
        assert!(engine.repo.index().unwrap().has_conflicts());
    }

    #[test]
    fn legacy_filtered_push_applies_remote_edits_to_existing_selected_files() {
        let (_root, mut engine, peer) = fixture();
        let local = engine.repo.workdir().unwrap().to_path_buf();
        put(&local, "one/local.md", "local addition\n");
        put(&peer, "one/SKILL.md", "remote edit\n");
        publish(&peer);
        let filter = SkillSyncFilter {
            include_prefixes: vec![],
            include_skill_ids: vec!["one".into()],
        };
        engine
            .stage_and_push_filtered("sync", &filter, None, None, None)
            .unwrap();
        assert_eq!(
            fs::read_to_string(local.join("one/SKILL.md")).unwrap(),
            "remote edit\n"
        );
        assert!(engine.get_pending_changes().unwrap().is_empty());
        let head = engine.repo.head().unwrap().target();
        engine
            .stage_and_push_filtered("sync", &filter, None, None, None)
            .unwrap();
        assert_eq!(engine.repo.head().unwrap().target(), head);
    }

    #[test]
    fn interrupted_local_application_recognizes_completed_file_writes() {
        let (_root, engine, peer) = fixture();
        let local = engine.repo.workdir().unwrap();
        put(&peer, "one/SKILL.md", "remote one\n");
        put(&peer, "two/SKILL.md", "remote two\n");
        publish(&peer);
        let mut plan = engine.prepare_plan(None, None, None, None).unwrap();
        plan.commit = plan.remote.clone();
        plan.phase = "applying".into();
        engine.save_plan(&plan).unwrap();
        // Simulate process termination after the first rename, before journaling.
        put(local, "one/SKILL.md", "remote one\n");
        let mut resumed = engine.load_plan().unwrap().unwrap();
        engine.apply_plan(&mut resumed, None, None, None).unwrap();
        assert_eq!(
            fs::read_to_string(local.join("two/SKILL.md")).unwrap(),
            "remote two\n"
        );
        assert_eq!(resumed.phase, "complete");
        assert!(!engine.get_status().unwrap().has_changes);
    }
}
