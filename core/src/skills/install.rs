use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use git2::{build::RepoBuilder, FetchOptions};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::skills::models::{SkillInstallCandidate, SkillInstallRequest, SkillInstallResponse};
use crate::skills::scanner::Scanner;

pub struct SkillInstaller {
    source_root: PathBuf,
    config_dir: PathBuf,
}

impl SkillInstaller {
    pub fn new(source_root: PathBuf, config_dir: PathBuf) -> Self {
        Self {
            source_root,
            config_dir,
        }
    }

    pub fn install(&self, req: SkillInstallRequest) -> Result<SkillInstallResponse, String> {
        fs::create_dir_all(&self.source_root).map_err(|e| {
            format!(
                "Failed to create skills source root {}: {}",
                self.source_root.display(),
                e
            )
        })?;

        let temp_root = std::env::temp_dir()
            .join(format!("tokenviewer-skill-install-{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_root)
            .map_err(|e| format!("Failed to create temp dir {}: {}", temp_root.display(), e))?;
        let _cleanup = TempDirCleanup(temp_root.clone());

        let prepared = self.prepare_source(&req, &temp_root)?;
        let candidates = find_skill_dirs(&prepared.root_dir)?;
        if candidates.is_empty() {
            return Err("No SKILL.md or skill.md was found in the selected source.".to_string());
        }

        let candidate_models = candidates_to_models(&candidates)?;
        if req.selected_skill_ids.is_empty() && candidate_models.len() > 1 {
            return Ok(SkillInstallResponse::selection_required(candidate_models));
        }

        let selected = if req.selected_skill_ids.is_empty() {
            candidates
        } else {
            let selected_ids: HashSet<&str> =
                req.selected_skill_ids.iter().map(String::as_str).collect();
            candidates
                .into_iter()
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|id| selected_ids.contains(id))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>()
        };

        if selected.is_empty() {
            return Err("No selected skills to install.".to_string());
        }

        let mut seen = HashSet::new();
        let mut planned = Vec::new();
        for skill_dir in selected {
            let skill_id = skill_dir
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("Invalid skill folder name: {}", skill_dir.display()))?
                .to_string();
            validate_skill_id(&skill_id)?;
            if !seen.insert(skill_id.clone()) {
                return Err(format!("Duplicate skill id in source: {}", skill_id));
            }

            let destination = self.source_root.join(&skill_id);
            if same_path(&skill_dir, &destination) {
                planned.push((skill_id, skill_dir, destination, true));
                continue;
            }
            if destination.exists() && !req.replace_existing {
                return Err(format!(
                    "Skill \"{}\" already exists. Enable replace to overwrite it.",
                    skill_id
                ));
            }
            planned.push((skill_id, skill_dir, destination, false));
        }

        let mut installed = Vec::new();
        for (skill_id, skill_dir, destination, already_in_place) in planned {
            if !already_in_place {
                if destination.exists() {
                    fs::remove_dir_all(&destination).map_err(|e| {
                        format!("Failed to replace {}: {}", destination.display(), e)
                    })?;
                }
                copy_dir_recursive(&skill_dir, &destination)?;
            }
            self.record_install(
                &skill_id,
                &req.source_type,
                &prepared.source_value,
                &destination,
            )?;
            installed.push(skill_id);
        }

        Ok(SkillInstallResponse::installed(installed))
    }

    fn prepare_source(
        &self,
        req: &SkillInstallRequest,
        temp_root: &Path,
    ) -> Result<PreparedSource, String> {
        match req.source_type.as_str() {
            "folder" => {
                let path = req
                    .path
                    .as_deref()
                    .ok_or_else(|| "Select a valid folder.".to_string())?;
                Ok(PreparedSource {
                    root_dir: expand_tilde(path),
                    source_value: path.to_string(),
                })
            }
            "zip" => {
                let path = req
                    .path
                    .as_deref()
                    .ok_or_else(|| "Select a valid ZIP file.".to_string())?;
                let status = Command::new("/usr/bin/ditto")
                    .args(["-x", "-k", path])
                    .arg(temp_root)
                    .status()
                    .map_err(|e| format!("Failed to extract ZIP: {}", e))?;
                if !status.success() {
                    return Err("Failed to extract ZIP archive.".to_string());
                }
                Ok(PreparedSource {
                    root_dir: temp_root.to_path_buf(),
                    source_value: path.to_string(),
                })
            }
            "git" => {
                let raw = req
                    .git_url
                    .as_deref()
                    .ok_or_else(|| "Enter a valid Git URL.".to_string())?;
                let git_source = GitInstallSource::parse(raw);
                let cache_dir = git_cache_dir(raw);
                let clone_dir = cache_dir.join("repo");

                // First pass (candidate discovery) refreshes the cache. The
                // second pass (install selected skills) reuses it so selecting
                // one skill does not clone the same repository again.
                if req.selected_skill_ids.is_empty() && cache_dir.exists() {
                    let _ = fs::remove_dir_all(&cache_dir);
                }
                if !clone_dir.exists() {
                    fs::create_dir_all(&cache_dir).map_err(|e| {
                        format!(
                            "Failed to create Git install cache {}: {}",
                            cache_dir.display(),
                            e
                        )
                    })?;
                    let mut builder = RepoBuilder::new();
                    let mut fetch_options = FetchOptions::new();
                    fetch_options.depth(1);
                    builder.fetch_options(fetch_options);
                    if let Some(branch) = git_source.branch.as_deref() {
                        builder.branch(branch);
                    }
                    builder
                        .clone(&git_source.clone_url, &clone_dir)
                        .map_err(|e| format!("Failed to clone Git repository: {}", e))?;
                }
                let root_dir = git_source
                    .subpath
                    .iter()
                    .fold(clone_dir, |path, component| path.join(component));
                Ok(PreparedSource {
                    root_dir,
                    source_value: raw.to_string(),
                })
            }
            other => Err(format!("Unsupported install source type: {}", other)),
        }
    }

    fn record_install(
        &self,
        skill_id: &str,
        source_type: &str,
        source: &str,
        destination: &Path,
    ) -> Result<(), String> {
        let metadata_path = self.metadata_path();
        if let Some(parent) = metadata_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create install metadata dir {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }

        let mut metadata = if metadata_path.is_file() {
            let data = fs::read_to_string(&metadata_path).unwrap_or_default();
            serde_json::from_str::<InstallMetadata>(&data).unwrap_or_default()
        } else {
            InstallMetadata::default()
        };

        metadata.skills.insert(
            skill_id.to_string(),
            InstallRecord {
                id: skill_id.to_string(),
                installed_at: chrono::Utc::now().to_rfc3339(),
                source_type: source_type.to_string(),
                source: source.to_string(),
                destination: destination.to_string_lossy().to_string(),
            },
        );

        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("Failed to encode install metadata: {}", e))?;
        fs::write(&metadata_path, json).map_err(|e| {
            format!(
                "Failed to write install metadata {}: {}",
                metadata_path.display(),
                e
            )
        })?;
        Ok(())
    }

    fn metadata_path(&self) -> PathBuf {
        let home = self
            .config_dir
            .parent()
            .map(Path::to_path_buf)
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        home.join(".tokenviewer").join("install.json")
    }
}

struct PreparedSource {
    root_dir: PathBuf,
    source_value: String,
}

struct TempDirCleanup(PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Serialize, Deserialize)]
struct InstallMetadata {
    version: u32,
    #[serde(default)]
    skills: HashMap<String, InstallRecord>,
}

impl Default for InstallMetadata {
    fn default() -> Self {
        Self {
            version: 1,
            skills: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct InstallRecord {
    id: String,
    installed_at: String,
    source_type: String,
    source: String,
    destination: String,
}

struct GitInstallSource {
    clone_url: String,
    branch: Option<String>,
    subpath: Vec<String>,
}

impl GitInstallSource {
    fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let prefix = "https://github.com/";
        if !trimmed.starts_with(prefix) {
            return Self {
                clone_url: trimmed.to_string(),
                branch: None,
                subpath: Vec::new(),
            };
        }

        let without_prefix = &trimmed[prefix.len()..];
        let parts: Vec<&str> = without_prefix.split('/').collect();
        if parts.len() < 4 || parts[2] != "tree" {
            return Self {
                clone_url: trimmed.to_string(),
                branch: None,
                subpath: Vec::new(),
            };
        }

        let owner = parts[0];
        let repo = parts[1].trim_end_matches(".git");
        let branch = parts[3].to_string();
        let subpath = parts.iter().skip(4).map(|part| (*part).to_string()).collect();

        Self {
            clone_url: format!("https://github.com/{}/{}.git", owner, repo),
            branch: Some(branch),
            subpath,
        }
    }
}

fn find_skill_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut skills = Vec::new();
    if !root.exists() || !root.is_dir() {
        return Err(format!("Source directory does not exist: {}", root.display()));
    }
    find_skill_dirs_inner(root, 0, &mut skills)?;
    skills.sort();
    Ok(skills)
}

fn find_skill_dirs_inner(path: &Path, depth: usize, skills: &mut Vec<PathBuf>) -> Result<(), String> {
    if Scanner::validate_skill_dir(path) {
        skills.push(path.to_path_buf());
        return Ok(());
    }
    if depth >= 2 {
        return Ok(());
    }

    let entries =
        fs::read_dir(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    for entry in entries.filter_map(Result::ok) {
        let child = entry.path();
        if !child.is_dir() {
            continue;
        }
        if let Some(name) = child.file_name().and_then(|name| name.to_str()) {
            if matches!(name, ".git" | "node_modules" | "target" | "DerivedData") {
                continue;
            }
        }
        let _ = find_skill_dirs_inner(&child, depth + 1, skills);
    }
    Ok(())
}

fn candidates_to_models(paths: &[PathBuf]) -> Result<Vec<SkillInstallCandidate>, String> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for path in paths {
        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("Invalid skill folder name: {}", path.display()))?
            .to_string();
        validate_skill_id(&id)?;
        if !seen.insert(id.clone()) {
            return Err(format!("Duplicate skill id in source: {}", id));
        }
        candidates.push(SkillInstallCandidate {
            id,
            source_dir: path.to_string_lossy().to_string(),
        });
    }
    Ok(candidates)
}

fn validate_skill_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(format!("Invalid skill folder name: {}", id));
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|e| format!("Failed to create {}: {}", destination.display(), e))?;
    for entry in walkdir::WalkDir::new(source).min_depth(1) {
        let entry = entry.map_err(|e| format!("Failed to copy {}: {}", source.display(), e))?;
        let path = entry.path();
        let relative = path.strip_prefix(source).map_err(|e| e.to_string())?;
        if relative
            .components()
            .any(|component| component.as_os_str() == ".git")
        {
            continue;
        }
        let target = destination.join(relative);
        let file_type = entry.file_type();
        if file_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| format!("Failed to create {}: {}", target.display(), e))?;
        } else if file_type.is_symlink() {
            let link_target = fs::read_link(path)
                .map_err(|e| format!("Failed to read symlink {}: {}", path.display(), e))?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link_target, &target)
                .map_err(|e| format!("Failed to copy symlink {}: {}", target.display(), e))?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
            fs::copy(path, &target).map_err(|e| {
                format!(
                    "Failed to copy {} to {}: {}",
                    path.display(),
                    target.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

fn same_path(lhs: &Path, rhs: &Path) -> bool {
    let lhs = fs::canonicalize(lhs).unwrap_or_else(|_| lhs.to_path_buf());
    let rhs = fs::canonicalize(rhs).unwrap_or_else(|_| rhs.to_path_buf());
    lhs == rhs
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn git_cache_dir(source: &str) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.trim().hash(&mut hasher);
    std::env::temp_dir()
        .join("tokenviewer-skill-install-cache")
        .join(format!("{:016x}", hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn folder_with_multiple_skills_requires_selection() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("global");
        let source = dir.path().join("bundle");
        fs::create_dir_all(source.join("a")).unwrap();
        fs::create_dir_all(source.join("b")).unwrap();
        fs::write(source.join("a").join("SKILL.md"), "# A\n").unwrap();
        fs::write(source.join("b").join("SKILL.md"), "# B\n").unwrap();

        let installer = SkillInstaller::new(source_root, dir.path().join(".agents"));
        let response = installer
            .install(SkillInstallRequest {
                source_type: "folder".to_string(),
                path: Some(source.to_string_lossy().to_string()),
                git_url: None,
                replace_existing: false,
                selected_skill_ids: Vec::new(),
            })
            .unwrap();

        assert_eq!(response.status, "selection_required");
        assert_eq!(response.candidates.len(), 2);
    }

    #[test]
    fn installs_selected_skills_and_records_metadata() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("global");
        let source = dir.path().join("bundle");
        fs::create_dir_all(source.join("a")).unwrap();
        fs::create_dir_all(source.join("b")).unwrap();
        fs::write(source.join("a").join("SKILL.md"), "# A\n").unwrap();
        fs::write(source.join("b").join("SKILL.md"), "# B\n").unwrap();

        let installer = SkillInstaller::new(source_root.clone(), dir.path().join(".agents"));
        let response = installer
            .install(SkillInstallRequest {
                source_type: "folder".to_string(),
                path: Some(source.to_string_lossy().to_string()),
                git_url: None,
                replace_existing: false,
                selected_skill_ids: vec!["b".to_string()],
            })
            .unwrap();

        assert_eq!(response.status, "installed");
        assert_eq!(response.installed_skill_ids, vec!["b".to_string()]);
        assert!(!source_root.join("a").exists());
        assert!(source_root.join("b").join("SKILL.md").exists());
        assert!(dir.path().join(".tokenviewer").join("install.json").exists());
    }

    #[test]
    fn selected_git_install_reuses_discovery_cache() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("global");
        let cache_source = "https://example.test/repo.git";
        let cache_dir = git_cache_dir(cache_source);
        let _ = fs::remove_dir_all(&cache_dir);
        fs::create_dir_all(cache_dir.join("repo").join("a")).unwrap();
        fs::write(cache_dir.join("repo").join("a").join("SKILL.md"), "# A\n").unwrap();

        let installer = SkillInstaller::new(source_root.clone(), dir.path().join(".agents"));
        let response = installer
            .install(SkillInstallRequest {
                source_type: "git".to_string(),
                path: None,
                git_url: Some(cache_source.to_string()),
                replace_existing: false,
                selected_skill_ids: vec!["a".to_string()],
            })
            .unwrap();

        assert_eq!(response.status, "installed");
        assert!(source_root.join("a").join("SKILL.md").exists());
        let _ = fs::remove_dir_all(cache_dir);
    }
}
